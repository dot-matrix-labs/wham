# Chromium-Driven Testing (No Third Party)

This project includes a minimal Chromium driver (`cdp-runner`) that uses the Chrome DevTools Protocol over a raw WebSocket implementation built with Rust stdlib only.

## Build WASM
```bash
cargo install wasm-pack
cd crates/ui-wasm
wasm-pack build --target web --out-dir ../../examples/web/pkg
```

## Run the Chromium Test Runner
```bash
cargo run -p cdp-runner
```

The runner will:
- Start a local HTTP server (`python3 -m http.server`) serving `examples/web`.
- Launch Chromium headless with remote debugging enabled.
- Connect to CDP and drive input events directly.
- Assert accessibility JSON contains expected text.
- Trigger optimistic submit and rollback behavior.

## Cargo Test Integration
```bash
cargo test -p cdp-runner
```

This launches Chromium in a clean temporary profile per run and fails if Chromium is not found.

## Environment Variables
- `CDP_CHROME_BIN`: path to Chromium/Chrome binary.
- `CDP_URL`: page URL (default `http://127.0.0.1:8000/index.html`).
- `CDP_PORT`: remote debugging port (default `9222`).
- `CDP_HEADLESS`: `1` or `0`.
- `CDP_NO_SERVER`: `1` to skip the HTTP server.
- `CDP_NO_CHROME`: `1` to skip launching Chromium (tests will fail if set).

## Notes
- The test runner uses coordinate-based clicks based on the immediate-mode layout (1280x720).
- No DOM form elements are used; all interactions are through GPU-rendered widgets.
