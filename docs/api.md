# API Overview

This library provides a Rust core (`ui-core`) and a WASM/WebGL renderer (`ui-wasm`) for GPU-rendered forms without DOM widgets. All UI is rendered as GPU quads and text atlas glyphs on a canvas.

## Core Types
- `Ui`: immediate-mode UI builder. Call `begin_frame`, draw widgets, then `end_frame`.
- `TextBuffer`: text editing with caret, selection, IME composition, multiline, undo/redo.
- `Form`: schema-driven form model with validation and optimistic submit.
- `Batch`: batched quads + text runs for GPU rendering.

## Immediate-Mode Flow
1. Create `Ui` with a `Theme`.
2. Call `begin_frame(events, width, height, scale, time_ms)`.
3. Emit widgets: `label`, `label_colored`, `text_input`, `text_input_multiline`, `checkbox`, `radio_group`, `select`, `button`.
4. Call `end_frame` to get an accessibility tree.
5. Render `ui.batch` with the WASM renderer.

## Forms
- Build a `FormSchema` with `FieldSchema` and `ValidationRule`.
- Update values with `Form::set_value`.
- Call `Form::start_submit` to start an optimistic request.
- Apply responses with `Form::apply_success` or `Form::apply_error`.

## WASM
- `WasmApp` exposes `frame()` and input event handlers.
- `frame()` returns accessibility JSON for the host app to mirror in a separate layer (no DOM UI elements).
- `take_clipboard_request()` allows the host to integrate with the system clipboard.
