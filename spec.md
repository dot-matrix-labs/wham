# GPU Forms UI (Rust + WASM) Specification

## 1. Goals
- Provide a Rust + WASM library for GPU-rendered web forms with no DOM widgets (canvas-only).
- Immediate-mode UI for all widgets: inputs, selects, checkboxes, radio buttons, buttons, labels, tooltips, validation messages.
- Render widgets as GPU quads with batched vertex buffers.
- Text rendering via texture atlas (optionally SDF).
- Full text input: caret, selection, copy/paste, IME, multi-language.
- Accessibility metadata for screen readers, keyboard navigation, and high-contrast/font scaling.
- REST API integration: optimistic updates, validation, rollback, retries, timeouts.
- Support per-field validation, multi-field forms, dynamic/repeatable field groups, nested forms.
- Provide loading, success/error feedback, tooltips.
- Efficient state management (immutable or copy-on-write) with undo/redo stacks.
- Input events with instant hit-testing for mouse, touch, keyboard.
- Maintain 60 FPS for 100+ widgets with minimal memory usage on mobile and desktop.

## 2. Non-Goals
- No dependency on DOM form elements; only a canvas and optional accessibility mirror.
- No WYSIWYG layout editor (library APIs are code-first).

## 3. High-Level Architecture
- `ui-core` crate (Rust):
  - Immediate-mode UI layer (widgets, layout, styling).
  - Form model and validation.
  - State management with copy-on-write snapshots and undo/redo.
  - Input routing and hit-testing.
  - Accessibility tree generation.
  - REST integration (trait-based HTTP client with wasm and native implementations).
  - Renderer-agnostic batch builder (quads, text runs).
- `ui-wasm` crate (Rust + wasm-bindgen):
  - Canvas/WebGL2 renderer and resource management.
  - Web events (pointer, wheel, key, IME composition, clipboard).
  - Bridge to JS for clipboard and accessibility mirror.
- `examples`:
  - Login/register form.
  - Dynamic validation and repeatable groups.
  - Nested forms with tooltips and async submit.
- `docs`:
  - API and theming guide.
  - Accessibility guide (screen reader mirror, keyboard navigation, high-contrast).
- `tests` and `benches`:
  - Validation, optimistic updates, rollback, accessibility tree invariants.
  - Benchmark batching, layout, hit-testing, validation throughput.

## 4. Rendering Pipeline
- Immediate-mode building phase:
  - Widgets submit draw items to `Batcher`.
  - `Batcher` produces:
    - Quad vertex buffer (position, uv, color, flags).
    - Index buffer (two triangles per quad).
    - Draw ranges per material (solid, atlas text).
- Text rendering:
  - Texture atlas of glyph bitmaps (alpha) generated via `fontdue`.
  - Optional SDF path for sharper scaling.
  - Text runs cached per font/size to reduce allocations.
- WebGL2 renderer:
  - Single dynamic VBO + IBO, updated each frame (or ring buffer).
  - Single atlas texture, optional material textures for icons.
  - Scissor/clip rects for inputs and scroll containers.

## 5. Input and Text Editing
- Event types:
  - Pointer: down/up/move, touch, wheel, drag.
  - Keyboard: key down/up, text input.
  - IME: composition start/update/end.
  - Clipboard: copy/paste.
- Text edit model:
  - `TextBuffer`: UTF-8 string with grapheme navigation.
  - Selection range, caret position, anchor.
  - Composition span with temporary underline.
  - Undo/redo via text diff operations.

## 6. Accessibility
- Accessibility tree generated from widgets (no DOM UI elements):
  - Role, name/label, value, states (focused, disabled, invalid).
  - Bounds in logical pixels for screen-reader hit testing.
- Keyboard navigation:
  - Tab order and spatial navigation.
- High-contrast and font scaling:
  - Theme tokens adapt to user scaling settings.
  - Text uses atlas keyed by scale factor.
- Integration:
  - Expose accessibility tree to JS for a DOM mirror (offscreen).

## 7. Form Model and REST Integration
- Form is a tree of fields and groups:
  - Supports nested groups and repeatable collections.
- Validation:
  - Field-level (required, regex, numeric ranges, email).
  - Cross-field and group-level validation.
- REST:
  - Submissions queued with optimistic updates.
  - Retry policy with backoff.
  - Timeout handling.
  - Rollback on server error and conflict resolution.

## 8. Performance Targets
- 100+ widgets at 60 FPS on mid-range mobile.
- Batched draw calls (single digit per frame).
- Memory usage bounded by:
  - Shared atlas texture.
  - COW state snapshots.
  - Hit-test grid with fixed cell size.

## 9. Deliverables
- Full library source (workspace with `ui-core` and `ui-wasm`).
- Example forms (login/register, dynamic validation, nested groups).
- Tests for validation, optimistic updates, rollback, accessibility.
- Documentation for API, theming, accessibility.
- Benchmarks for CPU, GPU, memory overhead.
