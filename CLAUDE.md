# CLAUDE.md — Agent Instructions

## Project

GPU-rendered forms library. Rust + WebAssembly. All UI renders on a `<canvas>` via WebGL2. Zero DOM form elements.

## Build

```bash
# Build WASM
cd crates/ui-wasm && wasm-pack build --target web --out-dir ../../examples/web/pkg

# Run tests
cargo test -p ui-core
cargo test -p cdp-runner  # browser tests (needs Chromium)

# Benchmarks
cargo bench -p ui-core

# Serve demo
cd examples/web && python3 -m http.server 8080
```

## Workspace Structure

```
crates/ui-core/src/    # Platform-agnostic: widgets, forms, validation, text editing, batching
crates/ui-wasm/src/    # Browser-specific: WebGL2 renderer, glyph atlas, wasm-bindgen bindings
examples/web/          # Host page: app.js (events, a11y mirror), index.html, sw.js
docs/                  # API, testing, accessibility, theming, performance docs
docs/technical/        # Wasm memory constraints paper and review
```

## Key Files

- `crates/ui-core/src/ui.rs` — Immediate-mode widget API, focus management, layout, ID stack
- `crates/ui-core/src/text.rs` — TextBuffer (grapheme-aware editing, selection, undo/redo, IME)
- `crates/ui-core/src/batch.rs` — Batcher (vertex/index buffer builder, draw commands)
- `crates/ui-core/src/form.rs` — Form model, field tree, History<FormState>
- `crates/ui-wasm/src/renderer.rs` — WebGL2 renderer (shaders, VBO/IBO, draw dispatch)
- `crates/ui-wasm/src/atlas.rs` — Glyph texture atlas (fontdue rasterization, dirty-rect tracking)
- `examples/web/app.js` — JS event handling, AccessibilityMirror, requestAnimationFrame loop

## Architecture Rules

1. **`ui-core` must not depend on browser APIs.** No `wasm-bindgen`, no `web-sys`, no JS types. It must compile and test with `cargo test` on the host.
2. **Immediate-mode UI pattern.** Widgets are function calls, not persistent objects. The widget tree is rebuilt every frame from application state.
3. **Buffer reuse, not buffer cloning.** Hot-path buffers (`Batch`, events, text runs) are `clear()`ed and refilled each frame. Never `clone()` a buffer on the rendering hot path.
4. **Widget IDs use an ID stack.** `push_id()`/`pop_id()` build a path; widget ID = hash of full path. Never derive IDs from label text alone.
5. **Atlas must never silently corrupt.** When the atlas is full, it must evict or rebuild — never overwrite cached UV coordinates with different pixel data.
6. **Re-acquire JS views after Wasm calls.** Any JS code that holds a typed array view into Wasm memory must re-acquire it after calling into Wasm, because `memory.grow` detaches the underlying `ArrayBuffer`.

## Code Conventions

- Rust 2021 edition
- `#[derive(Debug, Clone)]` on public types
- Error handling: `Result<T, E>` for fallible operations; `panic!` only for programmer errors / invariant violations
- Naming: `snake_case` for functions and variables, `CamelCase` for types, `SCREAMING_SNAKE` for constants
- Tests go in `#[cfg(test)] mod tests` within the same file
- Shaders: GLSL ES 3.00 (`#version 300 es`), use `in`/`out` not `attribute`/`varying`

## Memory Rules (from Wasm constraints paper)

- Target < 64 MB linear memory working set on mobile
- Pre-allocate buffers at known sizes; avoid small scattered allocations
- Use arena/bump allocation for per-frame transient data
- Monitor memory high-water mark; warn at 75% of budget
- Handle `webglcontextlost` — GPU memory is separate and can be lost at any time
- Never cache `wasm.memory.buffer` across Wasm call boundaries in JS

## Reference Documents

- `ARCHITECTURE.md` — System architecture, module boundaries, data flow
- `ENGINEERING.md` — Goals, non-goals, constraints, patterns, implementation plan, risk register
- `spec.md` — Original product specification
- `plan.md` — Phase-by-phase engineering roadmap
- `critique.md` — Engineering review with identified defects
- `docs/technical/rust-wasm-memory-constraints.md` — Wasm memory constraints paper
- `docs/technical/rust-wasm-memory-constraints-review.md` — Technical review of the paper
