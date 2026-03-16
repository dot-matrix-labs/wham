# API Reference

This document is a quick-reference for the `ui-core` public API, organised by widget type. For an introduction see `docs/getting-started.md`.

All positions are in **logical (CSS) pixels**. All text positions are in **grapheme cluster** indices, not byte offsets.

---

## Frame lifecycle

### `Ui::new(width, height, theme) -> Ui`

Creates a new UI context. Pass the initial canvas size and a `Theme`.

```rust
let theme = Theme::default_light();
let mut ui = Ui::new(800.0, 600.0, theme);
```

---

### `Ui::begin_frame(events, width, height, scale, time_ms)`

Must be called once at the start of every animation frame. Clears the batch, resets the layout cursor, and processes all input events from this frame.

| Parameter | Type | Description |
|-----------|------|-------------|
| `events` | `Vec<InputEvent>` | All input events since the previous frame |
| `width` | `f32` | Canvas width in logical pixels |
| `height` | `f32` | Canvas height in logical pixels |
| `scale` | `f32` | Device-pixel ratio (1.0 on regular, 2.0 on HiDPI) |
| `time_ms` | `f64` | Monotonic timestamp in milliseconds |

---

### `Ui::end_frame() -> A11yTree`

Must be called after all widgets have been emitted. Handles Tab navigation, draws the focus ring, and returns the accessibility tree for the screen-reader mirror.

---

## Text inputs

### `Ui::text_input(label, buffer, placeholder) -> bool`

Single-line text input. Returns `true` on the frame the widget is clicked.

```rust
let mut buf = TextBuffer::new("");
if ui.text_input("Username", &mut buf, "Enter username") {
    // field just gained focus
}
```

---

### `Ui::text_input_masked(label, buffer, placeholder) -> bool`

Identical to `text_input` but characters are displayed as bullet characters. Use for password fields.

```rust
let mut pw_buf = TextBuffer::new("");
ui.text_input_masked("Password", &mut pw_buf, "••••••••");
```

---

### `Ui::text_input_multiline(label, buffer, placeholder, height) -> bool`

Multiline text input with a fixed `height` in logical pixels. Enter inserts a newline.

```rust
let mut notes = TextBuffer::new("");
ui.text_input_multiline("Notes", &mut notes, "Add a note…", 120.0);
```

---

### `Ui::text_input_for(form, path, label, placeholder) -> bool`

Form-integrated single-line text input. Creates and manages the `TextBuffer` internally; syncs changes back to `form` automatically. No manual buffer needed.

```rust
let path = FormPath::root().push("email");
ui.text_input_for(form, &path, "Email", "user@example.com");
```

---

### `Ui::text_input_masked_for(form, path, label, placeholder) -> bool`

Masked variant of `text_input_for`. Use for password fields inside a `Form`.

```rust
let path = FormPath::root().push("password");
ui.text_input_masked_for(form, &path, "Password", "");
```

---

## Buttons

### `Ui::button(label) -> bool`

A clickable button. Returns `true` on the frame it is clicked.

```rust
if ui.button("Submit") {
    form.start_submit(serde_json::json!({}), 3);
}
```

---

## Checkboxes

### `Ui::checkbox(label, value) -> bool`

A toggleable checkbox. Writes the new state into `*value` and returns `true` on the frame it changes.

```rust
let mut accept = false;
if ui.checkbox("I accept the terms", &mut accept) {
    // toggled this frame
}
```

---

## Select / Dropdown

### `Ui::select(label, options, value) -> bool`

A dropdown select widget. Opens a panel of `options` when clicked; writes the selected option into `*value`. Returns `true` when the selection changes.

Keyboard: ArrowUp/Down to navigate, Enter to confirm, Escape to close, type a character to jump to the first matching option.

```rust
let options = vec!["Red".to_string(), "Green".to_string(), "Blue".to_string()];
let mut colour = "Red".to_string();
if ui.select("Colour", &options, &mut colour) {
    println!("New colour: {}", colour);
}
```

---

### `Ui::radio_group(label, options, selected) -> bool`

A group of mutually-exclusive radio buttons. `*selected` is the index of the currently selected option. Returns `true` when the selection changes.

```rust
let sizes = vec!["S".to_string(), "M".to_string(), "L".to_string()];
let mut size = 1usize; // "M"
ui.radio_group("Size", &sizes, &mut size);
```

---

## Icons

### `Ui::set_icon_pack(pack: IconPack)`

Loads an icon pack. Call once during initialisation.

---

### `Ui::icon(name, size) -> Option<Rect>`

Draws a named icon at `size` logical pixels. Returns the bounding rect, or `None` if the icon is not in the loaded pack.

```rust
ui.icon("check-circle", 24.0);
```

---

### `Ui::icon_by_id(id: IconId, size) -> Option<Rect>`

Draws an icon by pre-looked-up `IconId`. Prefer this in hot paths to avoid per-frame string hashing.

---

## Layout

### `Ui::begin_row()` / `Ui::end_row()`

Switches the layout to horizontal for all widgets between these two calls. Widgets share the available width equally.

```rust
ui.begin_row();
ui.label("First name");
ui.label("Last name");
ui.end_row();
```

---

### `Ui::begin_row_with(weights: &[f32])` / `Ui::end_row()`

Like `begin_row`, but children get proportional widths. Weights are summed and each child gets `weight / total` of the available width.

```rust
// First child gets 1/3, second gets 2/3:
ui.begin_row_with(&[1.0, 2.0]);
ui.text_input("Code", &mut code_buf, "");
ui.text_input("Description", &mut desc_buf, "");
ui.end_row();
```

---

### `Ui::begin_scroll(label, height) -> u64` / `Ui::end_scroll()`

Creates a scrollable container of the given `height`. All widgets emitted between these two calls are clipped and offset by the scroll position. Returns the scroll container ID.

```rust
ui.begin_scroll("items", 300.0);
for item in &items {
    ui.label(item);
}
ui.end_scroll();
```

---

## Labels and annotations

### `Ui::label(text)`

A non-interactive text label.

---

### `Ui::label_colored(text, color)`

A label with an explicit foreground colour.

```rust
ui.label_colored("Error: field is required", Color::rgba(0.9, 0.2, 0.2, 1.0));
```

---

### `Ui::tooltip(target_label, text)`

Shows a small tooltip near the top-right corner when the widget identified by `target_label` is hovered. Call this immediately after the widget it annotates.

```rust
ui.button("Info");
ui.tooltip("Info", "This feature is experimental.");
```

---

## ID stack

### `Ui::push_id(id)` / `Ui::pop_id()`

Push and pop values onto the ID stack. Every widget call hashes the current ID stack path to produce a unique widget ID. Always call `push_id` before a loop and `pop_id` after to avoid ID collisions when the same label appears multiple times.

```rust
for (i, item) in items.iter().enumerate() {
    ui.push_id(i);
    ui.label(&item.name);
    if ui.button("Delete") { /* ... */ }
    ui.pop_id();
}
```

---

## Form model

### `FormSchema::new(name) -> FormSchema`

Creates an empty form schema builder.

---

### `FormSchema::field(name, field_type) -> Self`

Adds a field with the given `FieldType`.

---

### `FormSchema::required(name) -> Self`

Marks a field as required (adds `ValidationRule::Required`).

---

### `FormSchema::with_label(name, label) -> Self`

Sets the human-readable label for a field.

---

### `FormSchema::with_placeholder(name, placeholder) -> Self`

Sets placeholder text shown when the field is empty.

---

### `FormSchema::with_validation(name, rule) -> Self`

Appends a `ValidationRule` to a field.

Available rules: `ValidationRule::Required`, `ValidationRule::Email`,
`ValidationRule::Regex { pattern }`, `ValidationRule::NumberRange { min, max }`.

---

### `Form::new(schema) -> Form`

Creates a live form from a schema, initialising all fields to default values.

---

### `Form::set_value(path, value) -> FormEvent`

Updates the field at `path`. Marks it as touched and dirty, clears validation errors.

```rust
form.set_value(&FormPath::root().push("email"), FieldValue::Text("a@b.com".into()));
```

---

### `Form::validate() -> Result<(), Vec<ValidationError>>`

Runs all validation rules. On failure, writes error messages into `FieldState::errors`.

---

### `Form::start_submit(payload, retries) -> Result<FormEvent, FormEvent>`

Validates the form, then marks all fields as `pending` and returns `Ok(SubmissionStarted(id))`.
Returns `Err(ValidationFailed(errors))` if validation fails.

After the server responds:
- `form.apply_success(id)` — clears pending/dirty flags.
- `form.apply_error(id, message, rollback)` — optionally rolls back to the pre-submit snapshot.

---

### `Form::state() -> &FormState`

Returns the current form state (all field values and flags).

```rust
let field = form.state().get_field(&FormPath::root().push("email")).unwrap();
if let FieldValue::Text(ref email) = field.value {
    println!("Email: {}", email);
}
```

---

## TextBuffer

Used by `text_input` and `text_input_multiline`. All positions are grapheme cluster indices.

| Method | Description |
|--------|-------------|
| `TextBuffer::new(text)` | Creates a buffer with initial content; caret at end |
| `text()` | Returns the current string |
| `grapheme_len()` | Number of grapheme clusters |
| `caret()` | Current caret position |
| `selection()` | Current selection (or `None`) |
| `insert_text(text)` | Insert at caret, replacing selection |
| `delete_backward()` | Backspace |
| `delete_forward()` | Delete key |
| `select_all()` | Select all text |
| `set_text(text)` | Replace all content, reset undo history |
| `undo() -> bool` | Undo last edit |
| `redo() -> bool` | Redo last undone edit |
| `selected_text()` | Returns the selected string slice |
| `cut_selection()` | Returns and removes selected text |

---

## Theme

```rust
let theme = Theme::default_light();
let theme = Theme::default_dark();

// Customise:
ui.theme_mut().font_scale = 1.2;       // larger text
ui.theme_mut().high_contrast = true;    // accessibility mode
ui.theme_mut().reduced_motion = true;   // disable animations
```

See `docs/theming.md` for the full colour token reference.
