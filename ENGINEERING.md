# Engineering Specification

## 1. Goals

### 1.1 Product Goals

- **G1: Canvas-only forms library.** Ship a Rust + WASM library that renders production-quality web forms entirely on a GPU-driven `<canvas>`, with zero DOM form elements.
- **G2: Mobile-first text input.** Full text editing on iOS Safari and Android Chrome — virtual keyboard activation, IME composition (CJK, Korean Hangul), copy/paste, autofill.
- **G3: Accessible by default.** Meet WCAG 2.1 AA. Screen readers (NVDA, VoiceOver, TalkBack) navigate the form via a hidden DOM mirror with correct ARIA semantics.
- **G4: 60 FPS on mid-range mobile.** 100+ widgets rendered within an 8.3ms frame budget on devices like Pixel 6a. Single-digit draw calls per frame.
- **G5: Offline-capable PWA.** Installable, works offline, queues form submissions and replays on reconnect.
- **G6: Memory-safe under Wasm constraints.** Operate within Wasm's grow-only linear memory model without unbounded growth, fragmentation-induced OOM, or stale typed-array corruption.

### 1.2 Engineering Quality Goals

- **G7: Platform-agnostic core.** `ui-core` compiles and tests with `cargo test` — no browser, no wasm-bindgen, no JS runtime.
- **G8: Zero-allocation steady state.** After initialization, the hot rendering loop allocates zero heap memory. Buffers are reused, not cloned.
- **G9: Correct widget identity.** Every widget instance has a unique, stable ID derived from an explicit ID stack, not from label text.

## 2. Non-Goals

- **NG1: General-purpose GUI framework.** This is a forms library. No rich text editor, no diagram tool, no game engine.
- **NG2: DOM hybrid rendering.** No mixing of DOM `<input>` elements with canvas-rendered widgets (except the hidden textarea proxy and accessibility mirror).
- **NG3: WYSIWYG layout editor.** API-first, code-first. No drag-and-drop form builder.
- **NG4: Server-side rendering.** Wasm runs in the browser. No SSR, no Node.js target.
- **NG5: Backward compatibility with existing form libraries.** No React/Vue adapter layer. This is a standalone library.
- **NG6: WASI / non-browser runtimes.** Browser-only target. Wasmtime/Wasmer are out of scope.
- **NG7: Custom font loading at runtime.** Ship with bundled fonts. System font fallback via JS is acceptable; dynamic font download is not a goal.

## 3. Constraints

### 3.1 Wasm Memory Constraints

These constraints are derived from the analysis in `docs/technical/rust-wasm-memory-constraints.md` and its review.

| Constraint | Impact | Mitigation Pattern |
|---|---|---|
| **Grow-only linear memory.** `memory.grow` adds pages; no `memory.shrink`. | Long-running sessions accumulate committed-but-unused pages. | Arena allocation for per-frame data. Buffer reuse. Worker lifecycle resets for extreme cases. |
| **No `madvise(MADV_DONTNEED)`.** Cannot decommit pages back to the OS. | Fragmentation is permanent. Peak memory is the high-water mark. | Avoid small, scattered allocations. Use bump allocators for transient data. Pre-allocate buffers at known sizes. |
| **`memory.grow` returns -1 on failure → Rust aborts.** No recovery from OOM. | A single allocation failure anywhere (including third-party code) kills the module. | Memory budgeting. Pre-allocate conservatively. Monitor `performance.measureUserAgentSpecificMemory()`. |
| **`ArrayBuffer` detachment on grow.** JS typed array views are invalidated when Wasm memory grows. | Stale views cause silent data corruption or TypeError. | Re-acquire views after any call that might allocate on the Rust side. Never cache `wasm.memory.buffer` across calls. |
| **4 GB address space limit (memory32).** | Theoretical max; practical limit is lower due to browser and device constraints. | Target < 64 MB working set for mobile. |
| **GPU memory is separate and untracked.** Atlas textures, VBOs, IBOs consume GPU memory outside Wasm's accounting. | GPU OOM manifests as context loss, not a grow failure. | Track GPU allocations manually. Handle `webglcontextlost`. Budget atlas size per device class. |

### 3.2 Browser Constraints

| Constraint | Impact |
|---|---|
| iOS Safari requires a focused DOM element for virtual keyboard | Hidden textarea must be positioned over the active widget, not offscreen |
| WebGL context can be lost at any time | Must handle `webglcontextlost` — recreate shaders, textures, buffers |
| No native accessibility for canvas content | Must maintain a parallel hidden DOM tree with ARIA attributes |
| Clipboard API requires user gesture and secure context | Copy/paste must be triggered from user-initiated events |
| Cross-origin restrictions on font loading | Bundled fonts preferred over CDN |

### 3.3 Performance Budgets

| Metric | Budget |
|---|---|
| Frame time (100 widgets) | < 8.3 ms (120 Hz) |
| Wasm binary size (gzipped) | < 500 KB |
| Linear memory working set (mobile) | < 64 MB |
| Atlas texture size (mobile) | 1024×1024 (4 MB GPU) |
| Atlas texture size (desktop) | 2048×2048 (16 MB GPU) |
| Draw calls per frame | < 10 |
| Heap allocations per frame (steady state) | 0 |

## 4. Engineering Patterns

### 4.1 Immediate-Mode UI Loop

```
begin_frame()
  → process events
  → widgets emit draw items to Batcher
  → validation runs on dirty fields
  → accessibility tree generated
end_frame()
  → Batcher produces vertex/index buffers
renderer.render()
  → upload buffers, draw
```

**Why this pattern:** Immediate-mode avoids persistent widget tree allocations that fragment Wasm's grow-only memory. The entire UI state is rebuilt each frame from application state. Transient allocations (vertex buffers, text runs) are reused via `clear()`, not `drop()` + `new()`.

### 4.2 Buffer Reuse (Zero-Allocation Rendering)

```rust
// WRONG: allocates every frame
let batch = Batch::new();

// RIGHT: reuse, clear, refill
self.batch.clear();  // sets len=0, keeps capacity
// ... widgets fill batch ...
```

All hot-path buffers (`Batch::vertices`, `Batch::indices`, `Batch::commands`, `events`) must be long-lived fields that are `clear()`ed each frame. Never `clone()` a buffer on the hot path.

**Applies to:** `Batch`, `Vec<InputEvent>`, `Vec<TextRun>`, `Vec<AccessibilityNode>`.

### 4.3 Widget ID Stack

```rust
ui.push_id("user_form");
  ui.push_id("row_0");
    ui.text_input("email", &mut buf);  // ID = hash("user_form/row_0/email")
  ui.pop_id();
ui.pop_id();
```

Widget IDs are derived from the full ID stack path, not from the label alone. This prevents collisions when the same label appears multiple times (e.g., repeated form rows). Every immediate-mode framework (Dear ImGui, egui) uses this pattern.

### 4.4 Atlas Lifecycle

```
Glyph requested → check cache
  → HIT: return cached UV rect
  → MISS: rasterize with fontdue
    → fits in atlas? allocate row, upload dirty rect
    → doesn't fit? evict LRU page (or clear + rebuild if single-page)
```

The atlas must never silently wrap around. When full, it must either evict or rebuild — never overwrite cached glyph UVs with different pixel data.

### 4.5 Cross-Boundary Safety

After any call from JS into Wasm that might trigger allocation:
```javascript
// Re-acquire the view — the old one may be detached
const view = new Uint8Array(wasm.memory.buffer);
```

The wrapper layer must ensure JS never holds a stale `ArrayBuffer` reference across a Wasm call boundary.

### 4.6 Accessibility Mirror Sync

Each frame, the Rust side produces an accessibility tree (flat list of nodes with roles, labels, bounds). The JS side diffs this against the current DOM mirror and patches:
- Added nodes → `createElement` + set ARIA attributes
- Removed nodes → `removeChild`
- Changed nodes → update attributes

Nodes must be spatially positioned to match canvas widget bounds for screen reader spatial navigation. `tabindex` must reflect the focus order.

### 4.7 Form State with Copy-on-Write

```rust
// Arc-based COW: clone is cheap (refcount bump) until mutation
let snapshot = Arc::clone(&current_state);
// On mutation: Arc::make_mut clones only if refcount > 1
Arc::make_mut(&mut current_state).set_field(path, value);
history.push(snapshot);
```

Undo/redo stores `Arc<FormState>` snapshots. Most snapshots share structure. Only the mutated state pays for a clone.

### 4.8 Error Handling Boundaries

| Boundary | Strategy |
|---|---|
| Wasm → JS interop | `console_error_panic_hook` converts panics to console errors with stack traces. Never panic silently. |
| GPU operations | Check for context loss before and after GL calls. Log and recover. |
| Network (REST) | Timeout + retry with backoff. Optimistic rollback on failure. Queue offline. |
| Memory growth | Pre-check budget before large allocations. Log warnings at 75% of budget. |

## 5. Known Defects (Current State)

These are documented defects from the engineering review. They must be resolved before the project exits prototype phase.

| ID | Defect | Severity | Status |
|---|---|---|---|
| D1 | Widget IDs derived from label text → collisions | Critical | Fixed (PR #50) |
| D2 | Per-frame `clone()` of Batch, text_runs, events | Critical | Fixed (PR #53) |
| D3 | Atlas overflow wraps to (1,1) → garbled text | Critical | Fixed (PR #54) |
| D4 | Shaders use GLSL ES 1.0 syntax in WebGL2 context | Major | Fixed (PR #56) |
| D5 | Hidden textarea at `left:-9999px` breaks iOS Safari | Major | Fixed (PR #55) |
| D6 | Accessibility mirror has no spatial positioning, no tabindex, O(n²) diff | Major | Open |
| D7 | Form History clones entire FormState per keystroke | Major | Open |
| D8 | Text rasterization happens on render path, not layout path | Moderate | Open |
| D9 | No VAO usage — vertex attrib setup every frame | Moderate | Open |
| D10 | BiDi module is a stub (returns single run) — adds 50KB+ to binary | Moderate | Open |
| D11 | No `webglcontextlost` handling | Major | Open |
| D12 | Form<->TextBuffer sync is manual, not automatic | Moderate | Open |

## 6. Implementation Plan

### Phase 0: Stabilize Foundation (Current → Stable Prototype)

**Goal:** Fix all critical and major defects. After this phase, the prototype is honest about what it can and cannot do.

| Task | Defect | Effort | Depends On |
|---|---|---|---|
| Rebuild accessibility mirror with spatial positioning, tabindex, structural ARIA | D6 | XL | — |
| Replace FormState full-clone with structural sharing (per-field `Arc`) | D7 | L | — |
| Move text rasterization from render path to layout path | D8 | M | — |
| Add VAO usage in renderer | D9 | S | — |
| Remove BiDi stub or implement UAX#9 | D10 | M (remove) / XL (implement) | — |
| Handle `webglcontextlost` / `webglcontextrestored` | D11 | M | — |
| Build automatic Form↔TextBuffer binding layer | D12 | L | D7 |

### Phase 1: Make It Correct

**Goal:** The app works on desktop and mobile with assistive technology. CJK input works. Passwords are masked.

| Task | Effort | Depends On |
|---|---|---|
| Validate hidden textarea positioning on iOS Safari + Android Chrome | M | D5 (done) |
| Fix IME double-insertion (gate `beforeinput` on `!isComposing`) | M | — |
| Add `preventDefault()` to handled events (Tab, Backspace, Space, Enter, arrows) | S | — |
| Pointer capture for drag selection | S | — |
| Password masking (`masked: bool` on text_input) | M | — |
| Test with NVDA + Firefox, VoiceOver + Safari, TalkBack + Chrome | L | D6 |

### Phase 2: Text Rendering & Internationalization

**Goal:** Text that looks good and works for non-Latin scripts.

| Task | Effort | Depends On |
|---|---|---|
| SDF text atlas (signed distance field glyph rendering) | L | — |
| Font-size-aware glyph cache (quantized to 2px buckets) | M | — |
| Proportional text metrics (replace hardcoded `char_width = font_size * 0.6`) | M | — |
| Multi-page atlas with LRU eviction | L | D3 (done) |
| Font fallback chain (primary + fallback fonts) | M | — |
| BiDi text support (if decided to implement in Phase 0) | XL | D10 |

### Phase 3: Layout & Interaction

**Goal:** Layouts that work for real applications. Interactions that feel right.

| Task | Effort | Depends On |
|---|---|---|
| Horizontal layout / row containers (`begin_row` / `end_row`) | M | — |
| Scroll containers with touch inertia | L | — |
| Real dropdown/select widget (floating panel, keyboard nav, type-ahead) | L | — |
| Touch target sizing (44×44pt minimum on mobile) | S | — |
| Focus ring rendering | S | — |
| Safe area insets | S | — |

### Phase 4: Performance

**Goal:** 60fps on Pixel 6a with 100+ widgets.

| Task | Effort | Depends On |
|---|---|---|
| Dirty-region tracking (rebuild only changed widget quads) | L | Phase 0 (D8) |
| Instanced rendering for solid-color quads | M | Phase 0 (D9) |
| Double-buffered VBO | M | — |
| Frame budget monitoring (warn if >12ms) | M | — |
| `wasm-opt -Oz` + `twiggy` profiling for binary size | S | — |

### Phase 5: PWA Infrastructure

**Goal:** Installable, offline-capable, app-store-free distribution.

_(Tasks as defined in plan.md — manifest, service worker, offline queue, push notifications, install prompt.)_

### Phase 6: Production Readiness

**Goal:** Ship to real users with confidence.

_(Tasks as defined in plan.md — autofill integration, cross-browser testing, performance CI, error reporting, theming, documentation.)_

### Phase 7: Headless Scenario Test Coverage

**Goal:** Prove the library works for real downstream use cases by running realistic full-form scenarios in a headless software renderer, without a browser or WebGL.

| Task | Issue | Effort | Depends On | Status |
|---|---|---|---|---|
| `Session` harness + `FrameResult` in `wham-test` | [#120](https://github.com/dot-matrix-labs/wham/issues/120) | S | — | ✅ Done (PR #123) |
| Sign-in form headless scenario (email, password, button) | [#120](https://github.com/dot-matrix-labs/wham/issues/120) | S | Session harness | ✅ Done (PR #123) |
| Checkout form headless scenario (multi-column, select, button) | [#121](https://github.com/dot-matrix-labs/wham/issues/121) | S | — | ✅ Done (PR #124) |
| Notification settings scenario (icons, checkboxes, radio, button) | [#122](https://github.com/dot-matrix-labs/wham/issues/122) | S | — | ✅ Done (PR #125) |
| Real glyph rendering in headless software rasterizer (fontdue) | [#126](https://github.com/dot-matrix-labs/wham/issues/126) | M | — | ✅ Done (PR #128) |
| CI `screenshots` job with artifact upload | [#127](https://github.com/dot-matrix-labs/wham/issues/127) | S | #126 | ✅ Done (PR #129) |

These tests run with `cargo test -p wham-test` and require no browser. They serve as living documentation for downstream consumers of the library. Screenshots of every form scenario are uploaded as CI artifacts on every push.

### Phase Dependencies

```
Phase 0 (Stabilize) ─┬──> Phase 1 (Correct) ──> Phase 2 (Text) ──┐
                      │                                             │
                      └──> Phase 3 (Layout) ───────────────────────┤
                                                                    v
                                                            Phase 4 (Perf)
                                                                    │
                                                    Phase 5 (PWA) ──┤
                                                                    v
                                                            Phase 6 (Prod)

Phase 7 (Headless Scenarios) ── runs in parallel, no blocking dependencies
```

## 7. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| iOS Safari blocks hidden textarea focus | Medium | Critical | Test early in Phase 1; fallback to contenteditable div. Already partially addressed in PR #55. |
| Wasm linear memory fragmentation causes OOM on long sessions | Medium | High | Arena allocation for per-frame data. Monitor high-water mark. Worker lifecycle reset as escape hatch. |
| Atlas texture exceeds GPU memory budget on mobile | Medium | High | Adaptive atlas sizing via `navigator.deviceMemory`. LRU eviction. SDF reduces per-glyph atlas footprint. |
| `memory.grow` failure kills module with no recovery | Low | Critical | Pre-allocate conservatively. Memory budget monitoring. Warn user at 75% threshold. |
| Accessibility mirror causes layout reflow | Low | Medium | `position: fixed; opacity: 0; pointer-events: none` prevents layout participation. |
| CJK glyph count (10K+) thrashes single-page atlas | High | Medium | Multi-page atlas with LRU eviction (Phase 2). |
| WebGL context loss during active form editing | Low | High | Handle `webglcontextlost` — serialize form state, restore on `webglcontextrestored`. |
| Wasm binary exceeds 500KB gzipped | Low | Medium | Profile with `twiggy`, strip debug info, `wasm-opt -Oz`, remove dead code (BiDi stub). |
| Form state undo/redo diverges from TextBuffer undo/redo | Medium | Medium | Unify undo stacks or clearly scope them (form-level vs. field-level). |
