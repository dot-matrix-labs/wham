import init, { WasmApp } from "./pkg/ui_wasm.js";

const canvas = document.getElementById("app");
const dpr = window.devicePixelRatio || 1;

function resize() {
  canvas.width = window.innerWidth * dpr;
  canvas.height = window.innerHeight * dpr;
  canvas.style.width = `${window.innerWidth}px`;
  canvas.style.height = `${window.innerHeight}px`;
}

/**
 * Create a hidden textarea overlaying the canvas for mobile keyboard input.
 *
 * iOS Safari scrolls the viewport to bring focused elements into view, even
 * when they are positioned offscreen (e.g. `left:-9999px`). To prevent this
 * we keep the textarea at `opacity:0` directly over the focused canvas widget
 * instead of hiding it offscreen.
 */
function createHiddenTextarea() {
  const ta = document.createElement("textarea");
  ta.setAttribute("autocapitalize", "off");
  ta.setAttribute("autocomplete", "off");
  ta.setAttribute("autocorrect", "off");
  ta.setAttribute("spellcheck", "false");
  ta.setAttribute("aria-hidden", "true");
  ta.setAttribute("tabindex", "-1");
  Object.assign(ta.style, {
    position: "absolute",
    top: "0px",
    left: "0px",
    width: "1px",
    height: "1px",
    opacity: "0",
    pointerEvents: "none",
    zIndex: "-1",
    padding: "0",
    border: "none",
    outline: "none",
    resize: "none",
    overflow: "hidden",
    // Prevent iOS zoom on focus
    fontSize: "16px",
  });
  document.body.appendChild(ta);
  return ta;
}

/**
 * Reposition the hidden textarea to overlay the focused widget so that iOS
 * Safari does not scroll the viewport when the textarea receives focus.
 *
 * Coordinates from the WASM side are in canvas (physical) pixels; we convert
 * to CSS pixels by dividing by `dpr`.
 */
function repositionTextarea(ta, rectArray, dpr) {
  if (!rectArray) {
    // No focused widget — park the textarea at origin so it stays harmless.
    ta.style.top = "0px";
    ta.style.left = "0px";
    ta.style.width = "1px";
    ta.style.height = "1px";
    return;
  }
  const x = rectArray[0] / dpr;
  const y = rectArray[1] / dpr;
  const w = rectArray[2] / dpr;
  const h = rectArray[3] / dpr;
  ta.style.left = `${x}px`;
  ta.style.top = `${y}px`;
  ta.style.width = `${Math.max(w, 1)}px`;
  ta.style.height = `${Math.max(h, 1)}px`;
}

async function main() {
  await init();
  resize();
  const app = new WasmApp(canvas, canvas.width, canvas.height, dpr);
  window.__app = app;

  const hiddenTextarea = createHiddenTextarea();

  window.addEventListener("resize", () => {
    resize();
    app.resize(canvas.width, canvas.height, dpr);
  });

  // --- visualViewport resize (virtual keyboard open/close) ---
  if (window.visualViewport) {
    window.visualViewport.addEventListener("resize", () => {
      // When the virtual keyboard opens the visual viewport shrinks.  Scroll
      // the focused widget into the visible area so it is not hidden behind
      // the keyboard.
      const rect = app.focused_widget_rect();
      if (!rect) return;

      const widgetBottomCSS = rect[1] / dpr + rect[3] / dpr;
      const viewportHeight = window.visualViewport.height;
      const viewportOffsetTop = window.visualViewport.offsetTop;
      const visibleBottom = viewportOffsetTop + viewportHeight;

      if (widgetBottomCSS > visibleBottom) {
        const scrollBy = widgetBottomCSS - visibleBottom + 20; // 20px padding
        window.scrollBy({ top: scrollBy, behavior: "smooth" });
      }
    });
  }

  // --- Clipboard helpers ---
  //
  // Safari requires Clipboard API calls to happen synchronously within a
  // user-gesture (keydown / pointerdown) context.  If a Wasm call or an
  // async gap sits between the browser event and the clipboard call, Safari
  // revokes the transient activation and the write/read silently fails.
  //
  // Strategy:
  //   Copy (Ctrl/Cmd+C) & Cut (Ctrl/Cmd+X):
  //     1. Detect the shortcut in the keydown handler.
  //     2. Forward the key event to Wasm so it updates internal state (e.g.
  //        sets clipboard_request with the selected text).
  //     3. Immediately (still in the same event handler — synchronous with
  //        the user gesture) call take_clipboard_request() and write to the
  //        clipboard.
  //   Paste (Ctrl/Cmd+V):
  //     1. Detect the shortcut in the keydown handler.
  //     2. Read from the clipboard *first* (while still in the user gesture).
  //     3. Pass the result to Wasm via handle_paste().
  //     4. Suppress the default keydown so we don't double-paste from the
  //        native "paste" event listener.
  //
  // On browsers without navigator.clipboard (e.g. non-secure contexts) we
  // fall back to document.execCommand.

  /**
   * Write text to the system clipboard.
   *
   * Prefers the async Clipboard API (works in all modern browsers when called
   * inside a user gesture).  Falls back to execCommand('copy') via a
   * temporary textarea for older browsers or insecure contexts.
   *
   * @param {string} text
   */
  function clipboardWrite(text) {
    if (navigator.clipboard && typeof navigator.clipboard.writeText === "function") {
      navigator.clipboard.writeText(text).catch((err) => {
        if (typeof console !== "undefined" && console.warn) {
          console.warn("[wham] clipboard write failed:", err);
        }
        // Attempt execCommand fallback on permission failure.
        clipboardWriteFallback(text);
      });
      return;
    }
    clipboardWriteFallback(text);
  }

  /** execCommand('copy') fallback for environments without Clipboard API. */
  function clipboardWriteFallback(text) {
    const ta = document.createElement("textarea");
    ta.value = text;
    // Position off-screen so it is invisible but still selectable.
    Object.assign(ta.style, {
      position: "fixed",
      left: "-9999px",
      top: "-9999px",
      opacity: "0",
    });
    document.body.appendChild(ta);
    ta.select();
    try {
      document.execCommand("copy");
    } catch (err) {
      if (typeof console !== "undefined" && console.warn) {
        console.warn("[wham] execCommand('copy') fallback failed:", err);
      }
    }
    document.body.removeChild(ta);
  }

  /**
   * Read text from the system clipboard.
   *
   * Returns a Promise<string>.  Prefers the async Clipboard API, falls back
   * to execCommand('paste') (which only works in some browsers).
   */
  function clipboardRead() {
    if (navigator.clipboard && typeof navigator.clipboard.readText === "function") {
      return navigator.clipboard.readText().catch((err) => {
        if (typeof console !== "undefined" && console.warn) {
          console.warn("[wham] clipboard read failed:", err);
        }
        return "";
      });
    }
    // execCommand('paste') fallback — limited browser support.
    return Promise.resolve("");
  }

  /**
   * Returns true if the event represents a copy/cut/paste keyboard shortcut.
   * Accounts for Ctrl (Windows/Linux) and Cmd (macOS).
   */
  function isClipboardShortcut(e) {
    const mod = e.ctrlKey || e.metaKey;
    if (!mod) return null;
    const key = e.key?.toLowerCase();
    if (key === "c") return "copy";
    if (key === "x") return "cut";
    if (key === "v") return "paste";
    return null;
  }

  canvas.addEventListener("pointerdown", (e) => {
    app.handle_pointer_down(e.offsetX * dpr, e.offsetY * dpr, e.button, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
    // On touch devices, focus the hidden textarea so the virtual keyboard opens.
    if (e.pointerType === "touch") {
      hiddenTextarea.focus({ preventScroll: true });
    }
  });
  canvas.addEventListener("pointerup", (e) => {
    app.handle_pointer_up(e.offsetX * dpr, e.offsetY * dpr, e.button, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
  });
  canvas.addEventListener("pointermove", (e) => {
    app.handle_pointer_move(e.offsetX * dpr, e.offsetY * dpr, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
  });
  canvas.addEventListener("wheel", (e) => {
    app.handle_wheel(e.offsetX * dpr, e.offsetY * dpr, e.deltaX, e.deltaY, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
  });

  window.addEventListener("keydown", (e) => {
    const clipboardAction = isClipboardShortcut(e);

    if (clipboardAction === "copy" || clipboardAction === "cut") {
      // Forward the key event to Wasm first so it populates the clipboard
      // request (selected text) and, for cut, deletes the selection.
      app.handle_key_down(e.keyCode, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
      // NOTE: After calling into Wasm, any cached typed-array views into
      // wasm.memory.buffer may be detached (memory.grow).  We only read a
      // JS string from take_clipboard_request(), so no view re-acquisition
      // is needed here.

      // Still inside the user gesture — clipboard write will succeed on Safari.
      const text = app.take_clipboard_request();
      if (text) {
        clipboardWrite(text);
      }
      // Prevent the browser from firing its own copy/cut (there is no real
      // selection in the DOM to copy).
      e.preventDefault();
      return;
    }

    if (clipboardAction === "paste") {
      // Initiate the clipboard read while still inside the user gesture so
      // Safari grants permission.  The Wasm handle_paste() call happens in
      // the .then() callback — this is fine because Wasm does not require a
      // user gesture; only the browser clipboard API does.
      e.preventDefault();
      clipboardRead().then((text) => {
        if (text) {
          app.handle_paste(text);
        }
      });
      return;
    }

    app.handle_key_down(e.keyCode, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
  });
  window.addEventListener("keyup", (e) => {
    app.handle_key_up(e.keyCode, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
  });
  window.addEventListener("beforeinput", (e) => {
    if (e.data) {
      app.handle_text_input(e.data);
    }
  });
  window.addEventListener("compositionstart", () => app.handle_composition_start());
  window.addEventListener("compositionupdate", (e) => app.handle_composition_update(e.data || ""));
  window.addEventListener("compositionend", (e) => app.handle_composition_end(e.data || ""));

  // The native "paste" event fires when the user pastes via the browser
  // context menu or on mobile long-press.  Keyboard paste (Ctrl/Cmd+V) is
  // handled in the keydown listener above and preventDefault()ed so this
  // handler will not double-fire for keyboard paste.
  window.addEventListener("paste", (e) => {
    const text = e.clipboardData?.getData("text/plain") || "";
    if (text) app.handle_paste(text);
  });

  function frame(ts) {
    const a11y = app.frame(ts);
    window.__a11y = a11y;

    // Drain any clipboard request that was produced outside a user gesture
    // (e.g. programmatic copy triggered by a button widget).  On Safari this
    // write may be rejected — clipboardWrite() logs a warning instead of
    // silently swallowing the error.
    const pendingClip = app.take_clipboard_request();
    if (pendingClip) {
      clipboardWrite(pendingClip);
    }

    // Reposition the hidden textarea over the currently focused widget so
    // that iOS Safari focus behaviour does not cause viewport scrolling.
    const focusedRect = app.focused_widget_rect();
    repositionTextarea(hiddenTextarea, focusedRect, dpr);

    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}

main();
