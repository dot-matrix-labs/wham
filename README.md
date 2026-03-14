# wham

GPU-rendered forms library. Rust + WebAssembly. All UI renders on a `<canvas>` via WebGL2. Zero DOM form elements.

Wham targets production web forms that need pixel-identical rendering across browsers, sub-millisecond interaction on mobile, and WCAG 2.1 AA accessibility — a combination no existing framework provides.

## Why build this?

We evaluated [12 frameworks](docs/prior-art.md) across Rust, C++, and Go that can target WebAssembly. The ecosystem splits cleanly into two camps:

**DOM-based frameworks** (Leptos, Dioxus, Yew) get accessibility and text rendering for free from the browser, but surrender control over rendering, layout consistency, and per-frame performance. Browser-specific form control styling and layout engine overhead make pixel-identical cross-platform forms impractical.

**GPU/canvas frameworks** (egui, Makepad, Dear ImGui) give full rendering control and can achieve zero-allocation steady-state rendering, but none of them solve accessibility or mobile text input on the web:

| Framework | GPU rendering | Web accessibility | Mobile input (IME, virtual keyboard) |
|---|---|---|---|
| egui | Yes | No (AccessKit is native-only) | No |
| Makepad | Yes | No | Partial |
| Dear ImGui | Yes | No | No |
| **wham** | Yes | Yes (DOM mirror) | Yes (hidden textarea proxy) |

No existing framework satisfies all three requirements simultaneously:
1. **GPU-rendered canvas** — pixel-identical rendering, batched draw calls, single-digit frame times
2. **Accessible via DOM mirror** — hidden DOM tree with ARIA semantics for screen readers (NVDA, VoiceOver, TalkBack)
3. **Mobile-first text input** — hidden textarea proxy for virtual keyboard activation, IME composition (CJK), iOS Safari compatibility

This is the same architectural pattern used by Figma (C++→Wasm with DOM accessibility overlay) and Google Docs (canvas rendering with ARIA mirror). Wham applies it specifically to forms.

### What we learned from prior art

| Lesson | From | Applied in wham |
|---|---|---|
| Immediate-mode avoids widget tree fragmentation in Wasm's grow-only memory | egui, Dear ImGui | Core architecture |
| SDF text atlas for resolution-independent glyph rendering | Makepad | Planned (Phase 2) |
| DOM mirror with ARIA provides canvas accessibility | Figma, Google Docs | `AccessibilityMirror` in app.js |
| Fine-grained reactivity beats VDOM diffing for memory | Leptos | No virtual DOM; immediate-mode rebuild per frame |
| Embedded memory budgets (~300 KB) inform buffer pre-allocation | Slint | Target < 64 MB linear memory on mobile |
| WAI-ARIA component patterns for widget semantics | Dioxus (Radix-based) | Widget roles and states in a11y tree |

Full evaluation with scores: [`docs/prior-art.md`](docs/prior-art.md)

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                      Browser                        │
│  ┌──────────────┐  ┌────────────┐  ┌─────────────┐  │
│  │ DOM / A11y   │  │ Canvas /   │  │ JS Runtime  │  │
│  │   Mirror     │  │ WebGL2     │  │ (events)    │  │
│  └──────┬───────┘  └──────┬─────┘  └──────┬──────┘  │
│  ┌──────┴──────────────────┴───────────────┴──────┐  │
│  │              ui-wasm (wasm-bindgen)             │  │
│  │  renderer · atlas · lib                        │  │
│  └────────────────────┬───────────────────────────┘  │
│  ┌────────────────────┴───────────────────────────┐  │
│  │                  ui-core                        │  │
│  │  ui · text · form · batch · validation         │  │
│  │  hit_test · accessibility · input · theme      │  │
│  └────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

- **`ui-core`** — Platform-agnostic. Immediate-mode widgets, text editing, form model, validation, batching, accessibility tree. Compiles and tests with `cargo test` on the host — no browser, no wasm-bindgen.
- **`ui-wasm`** — Browser-specific. WebGL2 renderer, glyph atlas (fontdue), wasm-bindgen bindings.
- **`examples/web`** — Host page. JS event handling, accessibility DOM mirror, `requestAnimationFrame` loop.

Details: [`ARCHITECTURE.md`](ARCHITECTURE.md)

## Build

```bash
# Build WASM
cd crates/ui-wasm && wasm-pack build --target web --out-dir ../../examples/web/pkg

# Run tests
cargo test -p ui-core

# Browser tests (needs Chromium)
cargo test -p cdp-runner

# Benchmarks
cargo bench -p ui-core

# Serve demo
cd examples/web && python3 -m http.server 8080
```

## Status

**Phase: Architecture & API Design** — see [tracking issue #39](https://github.com/dot-matrix-labs/wasm-ui/issues/39) for the full roadmap.

Phase 0 (critical foundation fixes) is complete. The prototype validates the rendering pipeline, text editing, and form model. Current work focuses on extracting a clean public API, building test infrastructure, and resolving remaining architectural defects before feature work begins.

### What works today

- Immediate-mode widget rendering (labels, buttons, checkboxes, radios, text inputs, selects, groups)
- Grapheme-aware text editing with undo/redo, selection, clipboard, IME composition
- Form model with validation (required, regex, email, numeric range), undo/redo, optimistic submit
- Glyph atlas with dirty-rect GPU upload
- Spatial hit-test grid (O(1) pointer-to-widget lookup)
- Hidden textarea proxy for mobile keyboard activation
- Accessibility tree generation (roles, labels, states, bounds)

### What's in progress

- Clean public API and framework extraction (#63)
- WebGL context loss handling (#65)
- Test infrastructure (#72)
- Accessibility mirror rebuild (#5)

## Documentation

| Document | Description |
|---|---|
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | System architecture, module boundaries, data flow, memory model |
| [`ENGINEERING.md`](ENGINEERING.md) | Goals, constraints, patterns, known defects, implementation plan |
| [`docs/prior-art.md`](docs/prior-art.md) | Evaluation of 12 Wasm UI frameworks and why we build from scratch |
| [`docs/api.md`](docs/api.md) | Widget API, text editing, form model, batching |
| [`docs/accessibility.md`](docs/accessibility.md) | Screen reader support, keyboard navigation, high contrast |
| [`docs/testing.md`](docs/testing.md) | Chromium-driven browser testing via CDP |
| [`docs/performance.md`](docs/performance.md) | Frame budgets, batching strategy, memory targets |
| [`docs/theming.md`](docs/theming.md) | Theme tokens, dark mode, high contrast |
| [`docs/technical/`](docs/techincal/) | Wasm memory constraints paper and review |
