# Engineering Review: wasm-ui

**Reviewer perspective**: someone who has shipped browsers, web standards, and billion-user web applications.

---

## The Thesis: Honest Assessment

The spec is well-written and refreshingly honest about the hard problems. The competitive analysis is accurate. The bet — GPU-rendered forms via WASM, bypassing the DOM — is a real design point in the solution space, not a fantasy. Figma, Google Docs, and VS Code's editor prove the canvas-based approach works at scale. Targeting *forms specifically* is the smartest scoping decision in this project.

**But the gap between "proven techniques exist" and "this codebase implements them" is enormous.** What I see here is a Phase 0.5 prototype being dressed in Phase 1-7 clothing.

---

## Critical Issues (Ship-Stoppers)

### 1. The text input architecture is fundamentally incomplete

The hidden `<textarea>` proxy (app.js:113-151) is positioned at `left:-9999px`. This will **break on iOS Safari**. Safari will scroll the viewport to the focused element even when it's offscreen. The spec even flags this as a top risk. The correct approach is a transparent, zero-opacity textarea positioned *over* the active input field, repositioned on every focus change. Figma does exactly this. Your plan acknowledges it, but the implementation went with the naive approach.

**The IME handling is untested theatre.** You have `compositionstart/update/end` handlers, but `update_composition` in `text.rs:204-216` replaces the composition range text on every update event. On CJK IMEs this fires rapidly. The `replace_range` call does byte-index conversion via linear scan (`byte_index_from_grapheme` at line 494) for every event — O(n) per grapheme per composition update. On a 10,000-character buffer with a slow IME, this will jank.

### 2. Widget identity via `hash_id(label)` is broken

`ui.rs:1044-1048` — widget IDs are derived from the *label text* via `DefaultHasher`. Two widgets with the same label will get the same ID. This will cause:
- Focus confusion (clicking one focuses the other)
- Accessibility tree corruption (duplicate IDs)
- Hit-test collisions

In the demo's nested form, "Label 1" and "Email 1" might survive, but any real application will have repeated labels. Every immediate-mode UI framework (Dear ImGui, egui) solved this with ID stacks or explicit push_id. You need this before anything else.

### 3. Per-frame allocations are massive despite "Task 4.7" comments

- `demo.rs:133` — `let batch = self.ui.batch.clone()` clones the **entire vertex buffer, index buffer, and command list** every frame.
- `demo.rs:134` — `let text_runs = batch.text_runs.clone()` clones again.
- `ui.rs:838` — `let events: Vec<InputEvent> = self.events.clone()` clones all events in `apply_pointer_selection`, which is called per focused text input per frame.
- `ui.rs:1124`, `1165`, `1194` — `self.events.clone()` called in `begin_scroll` and `dropdown` handlers.

On a form with 100 widgets (your success criterion), you're cloning the batch 100+ times per frame in scroll containers. The `events_scratch` optimization in demo.rs saves one Vec allocation while the system clones tens of KB of vertex data.

### 4. The accessibility mirror is a sketch, not an implementation

`app.js:35-69` — `AccessibilityMirror` creates `<div>` elements and sets `role` and `aria-label`. But:
- Elements are positioned at `left:-9999px` with no spatial mapping to canvas coordinates. Screen reader spatial navigation (VO+arrow keys) will not work.
- No `tabindex` management. Screen readers can't tab between elements.
- No `aria-describedby` for error messages on form fields.
- The update loop (line 47-49) does a `find()` per node per frame — O(n^2) diff.
- The a11y tree from Rust (end_frame) is flat — all widgets are direct children of root. Nested forms, groups, and fieldsets have no structural representation.

This will not pass WCAG 2.1 AA. It won't even pass a manual smoke test with NVDA.

### 5. The shaders use GLSL ES 1.0 / WebGL 1 syntax in a WebGL 2 context

`renderer.rs:533-574` — `attribute`, `varying`, `gl_FragColor` are WebGL 1 / GLSL ES 1.0. WebGL 2 contexts accept them for backward compatibility, but:
- You lose access to `layout(location=N)` qualifiers, flat interpolation, and integer attributes.
- `fwidth()` in the fragment shader (line 567) requires `OES_standard_derivatives` in WebGL 1 but is always available in GLSL ES 3.00. By sticking with old syntax you're relying on implicit compatibility that may behave differently across drivers.
- `u_use_texture` is an `int` uniform branched on in the fragment shader. This is a GPU pipeline stall on every material change. Use two separate programs or a single uber-shader with `#ifdef`.

### 6. Atlas overflow silently corrupts rendering

`atlas.rs:262-270` — when the atlas runs out of space, it wraps around to (1,1) and fills the pixel buffer with zeros. But `allocate_inner` can't clear the glyph cache (comment on line 265 acknowledges this). So cached glyphs still point to UV coordinates that now contain *different glyphs' pixels*. You'll get garbled text with no error, no warning, and no recovery path.

---

## Structural Concerns

### 7. The form state model is over-engineered for what it does

`Form` in `form.rs` maintains `History<FormState>`, where `FormState` contains `HashMap<FormPath, FieldState>`. Every `set_value` call (line 237) clones the entire `FormState`, including all field values and errors, then pushes it onto an unbounded `History` stack via `Arc`. For a 50-field form with undo history, this is hundreds of KB of cloned HashMaps per keystroke.

The undo model also doesn't compose with `TextBuffer`'s own undo stack. A user pressing Ctrl+Z in a text field will undo the last character (via TextBuffer), but the form's History doesn't know about it. The two undo stacks will diverge.

### 8. The demo is the product

There's no separation between "framework" and "application." `DemoApp` in `demo.rs` hardcodes four form schemas, manages all TextBuffer state manually, and manually syncs TextBuffer -> Form values on every submit. A real user would have to write the same boilerplate. The API requires:
1. Declare a `FormSchema`
2. Create `TextBuffer` for every text field separately
3. On submit, manually copy every TextBuffer into the Form via `set_value`
4. Manually display errors by iterating `form.state.fields`

Compare this to any declarative form library (React Hook Form, Formik) — the value proposition evaporates when the developer experience is this rough.

### 9. The renderer clones the batch, then mutates the clone

`renderer.rs:110-111` — `render()` takes a `&Batch`, clones it into `merged`, then pushes text quads into the clone. This means text quads are rasterized into the atlas *and* vertex data is generated *on the render path*, not the layout path. This couples atlas mutation to rendering order and means you can't pre-sort draw calls for batching efficiency.

### 10. No VAO usage

WebGL 2 provides Vertex Array Objects. The renderer sets up vertex attribute pointers every frame (`draw_batch` lines 275-290, `get_attrib_location` lookups included). This is exactly what VAOs eliminate. On mobile GPUs this is a measurable overhead.

---

## What's Actually Good

1. **The `TextBuffer` implementation** (`text.rs`) is solid. Grapheme-cluster-aware editing, proper selection normalization, word/line boundary detection using `unicode-segmentation`, undo/redo with invertible ops. This is better than what most canvas editors start with.

2. **The hit-test grid** (`hit_test.rs`) is a smart spatial index. Cell-based bucketing at 48px is well-tuned for touch targets.

3. **The dirty-rect atlas tracking** (`atlas.rs:278-287`) and partial upload path (`renderer.rs:183-222`) are correctly implemented and will save real bandwidth on the GPU bus.

4. **The service worker and PWA infrastructure** (`sw.js`, `app.js`) is production-quality for a first pass. Cache-first for static assets, network-first for API, offline queue with IndexedDB — this is correct.

5. **The spec and plan documents** are genuinely excellent. The risk register is honest. The phase dependencies are correct. The success criteria are measurable. This is better planning than I've seen on most funded startups.

---

## What I'd Do Differently (If This Were My Project)

1. **Fix widget identity immediately.** Add an ID stack (`push_id`/`pop_id`) or require explicit IDs. This is foundational.

2. **Stop cloning everything.** The batch should be borrowed, not cloned. Text runs should be rendered in-place. Events should be iterated by reference, not cloned into every handler.

3. **Move text rasterization out of the render path.** Build the full vertex buffer (including text quads) during layout, then hand an immutable batch to the renderer.

4. **Build a real form binding layer.** The TextBuffer<->Form sync should be automatic. A `FormField` widget should own its TextBuffer and update the Form's state on every edit, not require manual plumbing.

5. **Prove the mobile input story before building anything else.** The hidden textarea positioning, iOS Safari focus behavior, and CJK IME composition need to be tested on real devices. If this doesn't work, nothing else matters — the spec says so.

6. **Ship the BiDi module or delete it.** `bidi.rs` is a stub that returns a single run (line 76). It imports `unicode-bidi`, does work, then throws it away and returns the paragraph's base direction. This is dead code that adds 50KB+ to the WASM binary.

---

## Bottom Line

The **vision is sound**, the **planning is excellent**, and the **text editing core is genuinely good**. But the current implementation is a prototype that has been annotated with "Task X.Y" comments to create the appearance of progress on the roadmap without actually solving the hard problems. The atlas overflows silently, the accessibility layer is non-functional, widget IDs will collide, and every frame allocates enough memory to make a mid-range Android phone stutter.

The honest next step is to declare this Phase 0 — a proof of concept that validated the rendering pipeline — and rebuild the runtime around the problems the spec correctly identified: mobile input, accessibility, and allocation-free rendering. The Rust code quality is high enough that most of `text.rs`, `hit_test.rs`, `batch.rs`, and `validation.rs` survive that refactor. The form layer and demo need to be rethought from scratch.
