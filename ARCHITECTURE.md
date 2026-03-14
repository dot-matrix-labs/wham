# Architecture

## System Overview

wasm-ui is a GPU-rendered forms library. All UI widgets, text, and interaction render via WebGL2 on a single `<canvas>`. There are zero DOM form elements. The architecture is split between a platform-agnostic Rust core and a thin browser-specific WASM binding layer.

```
┌─────────────────────────────────────────────────────────┐
│                      Browser                            │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────┐  │
│  │  DOM / A11y  │  │ Canvas / GL  │  │  JS Runtime   │  │
│  │   Mirror     │  │   Context    │  │  (events, sw) │  │
│  └──────┬───────┘  └──────┬───────┘  └───────┬───────┘  │
│         │                 │                  │          │
│  ┌──────┴─────────────────┴──────────────────┴───────┐  │
│  │               ui-wasm (wasm-bindgen)               │  │
│  │  renderer.rs │ atlas.rs │ demo.rs │ lib.rs         │  │
│  └──────────────────────┬────────────────────────────┘  │
│                         │                               │
│  ┌──────────────────────┴────────────────────────────┐  │
│  │                    ui-core                         │  │
│  │  ui.rs │ text.rs │ form.rs │ batch.rs │ hit_test  │  │
│  │  validation.rs │ accessibility.rs │ state.rs      │  │
│  │  input.rs │ theme.rs │ types.rs │ rest.rs         │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

## Crate Boundaries

### `ui-core` (platform-agnostic)

Pure Rust. No `wasm-bindgen`, no browser APIs. Testable with `cargo test`.

| Module | Responsibility |
|---|---|
| `ui.rs` | Immediate-mode widget API (`begin_frame` / `end_frame` loop), focus management, layout, ID generation |
| `text.rs` | `TextBuffer` — grapheme-aware text editing, selection, caret, undo/redo, IME composition spans |
| `form.rs` | Form model — field tree, groups, repeatable collections, `History<FormState>` for undo/redo |
| `validation.rs` | Field-level and cross-field validation rules (required, regex, email, numeric ranges) |
| `batch.rs` | `Batcher` — collects draw items (quads + text runs) into vertex/index buffers and draw commands |
| `hit_test.rs` | Spatial grid (48px cells) for O(1) pointer-to-widget hit testing |
| `accessibility.rs` | Generates accessibility tree (roles, labels, values, states, bounds) for the DOM mirror |
| `input.rs` | Input event types and routing |
| `state.rs` | Copy-on-write state management |
| `theme.rs` | Color/spacing/font tokens, dark mode support |
| `types.rs` | Shared types (Rect, Color, etc.) |
| `rest.rs` | Trait-based HTTP client for form submission (optimistic updates, retry, rollback) |

### `ui-wasm` (browser-specific)

Depends on `ui-core`. Uses `wasm-bindgen` for JS interop.

| Module | Responsibility |
|---|---|
| `renderer.rs` | WebGL2 renderer — VBO/IBO management, shader compilation, draw call dispatch, atlas texture upload |
| `atlas.rs` | Glyph texture atlas — `fontdue`-based rasterization, dirty-rect tracking, partial GPU upload |
| `demo.rs` | `DemoApp` — reference application wiring form schemas, text buffers, and rendering loop |
| `lib.rs` | `wasm-bindgen` entry points, event forwarding from JS |

### `examples/web` (host page)

| File | Responsibility |
|---|---|
| `app.js` | Event listeners (pointer, keyboard, IME, clipboard), `AccessibilityMirror`, `requestAnimationFrame` loop |
| `index.html` | Canvas element, script loading |
| `sw.js` | Service worker — cache-first static, network-first API, offline submission queue |

## Data Flow Per Frame

```
1. JS event listeners capture pointer/keyboard/IME events
2. Events forwarded to WASM via wasm-bindgen calls
3. ui-core begin_frame():
   a. Process input events → update focus, selection, text buffers
   b. Run immediate-mode widget tree → widgets submit quads to Batcher
   c. Run validation on dirty fields
   d. Generate accessibility tree diff
4. ui-core end_frame():
   a. Batcher produces final vertex/index buffers + draw commands
   b. Accessibility tree serialized to JSON
5. ui-wasm renderer.render():
   a. Rasterize new glyphs into atlas (fontdue)
   b. Upload dirty atlas region to GPU (texSubImage2D)
   c. Upload vertex/index data to GPU (bufferData)
   d. Execute draw commands (drawElements with scissor rects)
6. JS reads accessibility JSON, patches DOM mirror
```

## Memory Architecture

Three memory domains exist simultaneously:

1. **Wasm linear memory** — Rust heap (vertex buffers, text buffers, form state, atlas pixel data). Grows via `memory.grow`, never shrinks. Managed by dlmalloc.
2. **JS heap** — DOM mirror nodes, event objects, wasm-bindgen prevent handle table. Managed by browser GC.
3. **GPU memory** — Atlas texture(s), VBO, IBO. Managed by WebGL driver. Subject to context loss.

Cross-boundary data copies happen at:
- Event forwarding (JS → Wasm): small, per-event
- Vertex buffer upload (Wasm → GPU): per-frame, proportional to visible widget count
- Atlas upload (Wasm → GPU): per-frame, only dirty rect
- Accessibility tree (Wasm → JS): per-frame, serialized JSON

See `docs/technical/rust-wasm-memory-constraints.md` for the full memory analysis.

## Key Architectural Decisions

| Decision | Rationale |
|---|---|
| Immediate-mode UI | Avoids persistent widget trees that fragment Wasm linear memory over time. Arena-friendly allocation pattern. |
| Single canvas, zero DOM widgets | Full rendering control, consistent cross-platform appearance, eliminates DOM layout cost. |
| `fontdue` for glyph rasterization | Pure Rust, no system font dependencies, works in Wasm without FFI. |
| Spatial hit-test grid (not tree) | O(1) lookup, cache-friendly, simpler than quad-tree for flat widget layouts. |
| Copy-on-write form state | Enables undo/redo without deep cloning on every mutation (uses `Arc` sharing). |
| Hidden textarea for mobile input | Required for iOS Safari virtual keyboard activation; native `<textarea>` gets OS-level IME support for free. |
| Accessibility DOM mirror | Canvas has no semantic structure; a hidden DOM tree provides screen reader access via ARIA. |
