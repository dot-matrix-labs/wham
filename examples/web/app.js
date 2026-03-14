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
  window.addEventListener("paste", (e) => {
    const text = e.clipboardData?.getData("text/plain") || "";
    if (text) app.handle_paste(text);
  });

  async function handleClipboard() {
    const request = app.take_clipboard_request();
    if (request) {
      try {
        await navigator.clipboard.writeText(request);
      } catch {}
    }
  }

  function frame(ts) {
    const a11y = app.frame(ts);
    window.__a11y = a11y;
    handleClipboard();

    // Reposition the hidden textarea over the currently focused widget so
    // that iOS Safari focus behaviour does not cause viewport scrolling.
    const focusedRect = app.focused_widget_rect();
    repositionTextarea(hiddenTextarea, focusedRect, dpr);

    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}

main();
