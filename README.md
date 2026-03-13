# GPU Forms UI (Rust + WASM)

GPU-rendered forms with no DOM UI elements. All widgets, text, and interaction are rendered via WebGL on a canvas and driven by a Rust immediate-mode UI core.

## Workspace
- `crates/ui-core`: Immediate-mode UI, forms, validation, accessibility tree, batching, text editing.
- `crates/ui-wasm`: WebGL2 renderer and WASM bindings.
- `examples/web`: Minimal browser demo wiring the WASM module to a canvas.

## Build (WASM)
```bash
cargo install wasm-pack
cd crates/ui-wasm
wasm-pack build --target web --out-dir ../../examples/web/pkg
```

Then open `examples/web/index.html` with a local server.

## Clipboard
The WASM layer exposes `take_clipboard_request()` for copy/cut. The host app must call `navigator.clipboard.writeText()`.

## Testing (Chromium)
See `docs/testing.md` for the no‑third‑party Chromium driver and instructions. `cargo test -p cdp-runner` launches Chromium with a fresh temporary profile each run.
