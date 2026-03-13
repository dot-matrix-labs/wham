import init, { WasmApp } from "./pkg/ui_wasm.js";

const canvas = document.getElementById("app");
const dpr = window.devicePixelRatio || 1;

function resize() {
  canvas.width = window.innerWidth * dpr;
  canvas.height = window.innerHeight * dpr;
  canvas.style.width = `${window.innerWidth}px`;
  canvas.style.height = `${window.innerHeight}px`;
}

async function main() {
  await init();
  resize();
  const app = new WasmApp(canvas, canvas.width, canvas.height, dpr);
  window.__app = app;

  window.addEventListener("resize", () => {
    resize();
    app.resize(canvas.width, canvas.height, dpr);
  });

  canvas.addEventListener("pointerdown", (e) => {
    app.handle_pointer_down(e.offsetX * dpr, e.offsetY * dpr, e.button, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
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
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}

main();
