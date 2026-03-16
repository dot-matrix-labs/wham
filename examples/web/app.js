import init, { WasmApp } from "./pkg/ui_wasm.js";

const canvas = document.getElementById("app");
const dpr = window.devicePixelRatio || 1;

// ---------------------------------------------------------------------------
// AccessibilityMirror — hidden DOM tree that mirrors canvas widgets for
// screen readers.  Elements are visually hidden (sr-only pattern) but remain
// in the accessibility tree so that NVDA, VoiceOver, TalkBack, etc. can
// navigate and interact with the GPU-rendered UI.
// ---------------------------------------------------------------------------
class AccessibilityMirror {
  /**
   * @param {HTMLCanvasElement} canvas  The rendering canvas.
   * @param {WasmApp}           app     Reference to the Wasm application.
   * @param {number}            dpr     Device pixel ratio.
   */
  constructor(canvas, app, dpr) {
    this._app = app;
    this._dpr = dpr;
    this._nodes = new Map(); // id -> DOM element
    this._suppressFocusSync = false;

    // Container — positioned over the canvas, transparent to pointer events,
    // but visible to the accessibility tree.
    this._container = document.createElement("div");
    this._container.setAttribute("role", "application");
    this._container.setAttribute("aria-label", "GPU Forms UI");
    this._container.setAttribute("aria-hidden", "false");
    Object.assign(this._container.style, {
      position: "absolute",
      top: "0",
      left: "0",
      width: "100%",
      height: "100%",
      pointerEvents: "none",
      overflow: "hidden",
      // Do not affect document layout.
      contain: "strict",
    });
    // Insert immediately after the canvas so the tab order is natural.
    canvas.parentNode.insertBefore(this._container, canvas.nextSibling);

    // Live region for status announcements (form errors, submission results).
    this._liveRegion = document.createElement("div");
    this._liveRegion.setAttribute("aria-live", "polite");
    this._liveRegion.setAttribute("aria-atomic", "true");
    Object.assign(this._liveRegion.style, {
      position: "absolute",
      width: "1px",
      height: "1px",
      overflow: "hidden",
      clip: "rect(0 0 0 0)",
      clipPath: "inset(50%)",
      whiteSpace: "nowrap",
    });
    this._container.appendChild(this._liveRegion);
  }

  /**
   * Update the mirror to match the latest accessibility tree from Wasm.
   *
   * @param {object} a11yTree  Deserialized A11yTree (root: A11yNode).
   */
  update(a11yTree) {
    if (!a11yTree || !a11yTree.root) return;
    const children = a11yTree.root.children || [];
    const seenIds = new Set();

    for (const node of children) {
      seenIds.add(this._idKey(node.id));
      this._upsertNode(node);
    }

    // Remove stale nodes that are no longer in the tree.
    for (const [key, el] of this._nodes) {
      if (!seenIds.has(key)) {
        el.remove();
        this._nodes.delete(key);
      }
    }
  }

  /**
   * Sync DOM focus to match the Wasm-side focused widget id.
   * @param {number|null} focusedId  The id of the focused widget (BigInt or Number), or null.
   */
  syncFocus(focusedId) {
    if (focusedId == null) return;
    const key = this._idKey(focusedId);
    const el = this._nodes.get(key);
    if (el && document.activeElement !== el) {
      this._suppressFocusSync = true;
      el.focus({ preventScroll: true });
      this._suppressFocusSync = false;
    }
  }

  /**
   * Announce a message to screen readers via the live region.
   * @param {string} message
   */
  announce(message) {
    // Clear and re-set so the same message can be announced twice in a row.
    this._liveRegion.textContent = "";
    // Use a microtask break so the browser registers the empty → filled change.
    requestAnimationFrame(() => {
      this._liveRegion.textContent = message;
    });
  }

  // -- private helpers ------------------------------------------------------

  _idKey(id) {
    // A11y ids may arrive as BigInt from serde_wasm_bindgen; normalise to string.
    return String(id);
  }

  _upsertNode(node) {
    const key = this._idKey(node.id);
    let el = this._nodes.get(key);
    const role = node.role;

    if (!el) {
      el = this._createElement(role);
      this._attachFocusHandler(el, node.id);
      this._container.appendChild(el);
      this._nodes.set(key, el);
    }

    // Update content & ARIA attributes.
    this._updateElement(el, node);
    this._positionElement(el, node.bounds);
  }

  _createElement(role) {
    let el;
    switch (role) {
      case "TextBox":
        el = document.createElement("input");
        el.type = "text";
        el.setAttribute("role", "textbox");
        break;
      case "Button":
        el = document.createElement("button");
        break;
      case "CheckBox":
        el = document.createElement("input");
        el.type = "checkbox";
        el.setAttribute("role", "checkbox");
        break;
      case "RadioButton":
        el = document.createElement("input");
        el.type = "radio";
        el.setAttribute("role", "radio");
        break;
      case "ComboBox":
        el = document.createElement("select");
        el.setAttribute("role", "combobox");
        break;
      case "Label":
        el = document.createElement("span");
        el.setAttribute("role", "note");
        break;
      case "Group":
        el = document.createElement("fieldset");
        el.setAttribute("role", "group");
        break;
      default:
        el = document.createElement("span");
        break;
    }

    // sr-only styling: visually hidden but accessible.
    Object.assign(el.style, {
      position: "absolute",
      overflow: "hidden",
      clip: "rect(0 0 0 0)",
      clipPath: "inset(50%)",
      width: "1px",
      height: "1px",
      whiteSpace: "nowrap",
      border: "0",
      padding: "0",
      margin: "-1px",
      pointerEvents: "auto", // allow screen reader interaction
    });
    el.tabIndex = 0;

    return el;
  }

  _updateElement(el, node) {
    el.setAttribute("aria-label", node.name || "");
    el.setAttribute("data-wham-id", this._idKey(node.id));

    if (node.value != null) {
      if (el.tagName === "INPUT" && el.type === "text") {
        el.value = node.value;
      } else if (el.tagName === "SELECT") {
        // nothing — options would need to be populated separately
      } else {
        el.setAttribute("aria-valuenow", node.value);
      }
    }

    const st = node.state || {};
    if (st.disabled) {
      el.setAttribute("aria-disabled", "true");
      el.disabled = true;
    } else {
      el.removeAttribute("aria-disabled");
      el.disabled = false;
    }

    if (st.invalid) {
      el.setAttribute("aria-invalid", "true");
    } else {
      el.removeAttribute("aria-invalid");
    }

    if (st.required) {
      el.setAttribute("aria-required", "true");
    } else {
      el.removeAttribute("aria-required");
    }

    if (node.role === "CheckBox") {
      el.checked = !!st.selected;
      el.setAttribute("aria-checked", st.selected ? "true" : "false");
    }

    if (node.role === "RadioButton") {
      el.checked = !!st.selected;
      el.setAttribute("aria-checked", st.selected ? "true" : "false");
    }

    if (node.role === "ComboBox") {
      el.setAttribute("aria-expanded", st.expanded ? "true" : "false");
    }
  }

  _positionElement(el, bounds) {
    if (!bounds) return;
    // bounds are in canvas (physical) pixels — convert to CSS pixels.
    const x = bounds.x / this._dpr;
    const y = bounds.y / this._dpr;
    const w = bounds.w / this._dpr;
    const h = bounds.h / this._dpr;
    el.style.left = `${x}px`;
    el.style.top = `${y}px`;
    // Override sr-only 1px sizing with actual widget bounds so screen reader
    // spatial navigation knows where elements are, but keep clip to hide them.
    el.style.width = `${Math.max(w, 1)}px`;
    el.style.height = `${Math.max(h, 1)}px`;
  }

  _attachFocusHandler(el, nodeId) {
    el.addEventListener("focus", () => {
      if (this._suppressFocusSync) return;
      // Coerce BigInt to Number for the Wasm FFI boundary.
      const id = typeof nodeId === "bigint" ? Number(nodeId) : nodeId;
      this._app.set_focus(id);
    });
  }
}

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
  const a11yMirror = new AccessibilityMirror(canvas, app, dpr);

  // --- Load fallback font (optional) ---
  // To support emoji or additional scripts, load a fallback font after the
  // primary font.  The fallback chain is tried in order when a glyph is
  // missing from the primary font.
  //
  // Example:
  //   fetch("NotoColorEmoji-Regular.ttf")
  //     .then(r => r.arrayBuffer())
  //     .then(buf => app.add_fallback_font(new Uint8Array(buf)));

  // --- IME composition state ---
  // Track whether an IME composition session is active so we can suppress
  // redundant text-input events that would cause double-insertion of the
  // composed string.  We maintain our own flag in addition to checking
  // e.isComposing on individual events because the relative ordering of
  // compositionend vs beforeinput differs across browsers (Chrome fires
  // compositionend first; Firefox fires beforeinput first).
  let isComposing = false;

  // --- WebGL context loss / restoration ---
  let contextLost = false;

  canvas.addEventListener("webglcontextlost", (e) => {
    e.preventDefault();
    contextLost = true;
    app.notify_context_lost();
    console.warn("[wham] WebGL context lost — rendering paused, form state preserved");
  });

  canvas.addEventListener("webglcontextrestored", () => {
    console.info("[wham] WebGL context restored — reinitializing renderer");
    try {
      app.reinitialize_renderer();
      contextLost = false;
    } catch (err) {
      console.error("[wham] Failed to reinitialize renderer after context restore:", err);
    }
  });

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
    // Capture the pointer so that pointermove/pointerup events continue to
    // fire even when the pointer leaves the canvas (e.g. during text
    // selection drag).  The capture is released automatically on pointerup
    // or via the explicit releasePointerCapture call below.
    canvas.setPointerCapture(e.pointerId);
    app.handle_pointer_down(e.offsetX * dpr, e.offsetY * dpr, e.button, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
    // On touch devices, focus the hidden textarea so the virtual keyboard opens.
    if (e.pointerType === "touch") {
      hiddenTextarea.focus({ preventScroll: true });
    }
  });
  canvas.addEventListener("pointerup", (e) => {
    canvas.releasePointerCapture(e.pointerId);
    app.handle_pointer_up(e.offsetX * dpr, e.offsetY * dpr, e.button, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
  });
  canvas.addEventListener("pointermove", (e) => {
    app.handle_pointer_move(e.offsetX * dpr, e.offsetY * dpr, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
  });
  canvas.addEventListener("wheel", (e) => {
    app.handle_wheel(e.offsetX * dpr, e.offsetY * dpr, e.deltaX, e.deltaY, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
    // Prevent the page from scrolling when the canvas has a focused widget.
    if (app.has_focused_widget()) {
      e.preventDefault();
    }
  }, { passive: false });

  window.addEventListener("keydown", (e) => {
    // During an active IME composition the OS/browser is in control of the
    // editing session.  Forwarding key events (arrows, backspace, etc.) to
    // Wasm would interfere with the composition window.
    if (e.isComposing || isComposing) {
      return;
    }

    const clipboardAction = isClipboardShortcut(e);

    if (clipboardAction === "copy" || clipboardAction === "cut") {
      // Forward the key event to Wasm first so it populates the clipboard
      // request (selected text) and, for cut, deletes the selection.
      app.handle_key_down(e.code, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
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

    app.handle_key_down(e.code, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);

    // Prevent browser defaults for keys that widgets handle when focused.
    // We check focus state AFTER forwarding to Wasm because the key event
    // may itself change focus (e.g. Tab moves focus to the next widget).
    if (app.has_focused_widget()) {
      const PREVENT_KEYS = new Set([
        "Tab", "Backspace", "Enter", "Space",
        "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight",
      ]);
      if (PREVENT_KEYS.has(e.key) || PREVENT_KEYS.has(e.code)) {
        e.preventDefault();
      }
    }
  });
  window.addEventListener("keyup", (e) => {
    app.handle_key_up(e.code, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
  });
  window.addEventListener("beforeinput", (e) => {
    // During IME composition the intermediate text is managed by the
    // compositionupdate/compositionend handlers.  If we also forwarded the
    // beforeinput data we would insert the composed string twice.
    //
    // We check both our manual flag AND e.isComposing because:
    //   - Chrome fires compositionend *before* the final beforeinput, so
    //     isComposing is already false but e.isComposing is still true.
    //   - Firefox fires beforeinput *before* compositionend, so
    //     e.isComposing may be false but isComposing is still true.
    if (!e.isComposing && !isComposing && e.data) {
      app.handle_text_input(e.data);
    }
    // Prevent the browser from inserting text into the hidden textarea (or
    // any other focused DOM element) when a canvas text widget owns focus.
    const kind = app.focused_widget_kind();
    if (kind === "textinput" || kind === "select") {
      e.preventDefault();
    }
  });
  window.addEventListener("compositionstart", () => {
    isComposing = true;
    app.handle_composition_start();
  });
  window.addEventListener("compositionupdate", (e) => app.handle_composition_update(e.data || ""));
  window.addEventListener("compositionend", (e) => {
    isComposing = false;
    app.handle_composition_end(e.data || "");
  });

  // The native "paste" event fires when the user pastes via the browser
  // context menu or on mobile long-press.  Keyboard paste (Ctrl/Cmd+V) is
  // handled in the keydown listener above and preventDefault()ed so this
  // handler will not double-fire for keyboard paste.
  window.addEventListener("paste", (e) => {
    const text = e.clipboardData?.getData("text/plain") || "";
    if (text) app.handle_paste(text);
  });

  function frame(ts) {
    if (contextLost) {
      requestAnimationFrame(frame);
      return;
    }
    const a11y = app.frame(ts);
    // NOTE: After calling app.frame() any typed-array views into
    // wasm.memory.buffer may have been detached by memory.grow.
    // We only use the JS object `a11y` (not a typed-array view) so no
    // re-acquisition is needed here.
    window.__a11y = a11y;

    // Update the accessibility shadow DOM mirror.
    a11yMirror.update(a11y);

    // Sync focus from Wasm to the DOM mirror so screen readers track
    // the canvas focus state.
    if (a11y && a11y.root) {
      const focusedNode = (a11y.root.children || []).find(
        (n) => n.state && n.state.focused
      );
      if (focusedNode) {
        a11yMirror.syncFocus(focusedNode.id);
      }
    }

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
