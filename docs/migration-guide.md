# Migration Guide — Coming from React / Vue Forms

This guide explains the conceptual differences between wham's immediate-mode GPU-rendered approach and the retained-mode component model used by React Hook Form, Formik, VeeValidate, and similar libraries.

---

## The core mental model shift

**React / Vue**: your form is a tree of stateful components. Each input component owns its value, subscribes to change events, and re-renders itself when state changes.

**wham**: your form is rebuilt from scratch every frame by a plain Rust function. There are no components, no lifecycle methods, and no event subscriptions. Input state lives in your application struct, not in the widget.

```
React                             wham
─────────────────────────────     ─────────────────────────────────────
<TextInput value={email}          ui.text_input_for(form, &email_path,
         onChange={setEmail} />       "Email", "Enter email");
```

---

## Controlled inputs

In React, a controlled input binds `value` to state and updates via `onChange`:

```jsx
// React
const [email, setEmail] = useState('');
<input value={email} onChange={e => setEmail(e.target.value)} />
```

In wham, `text_input_for` is the equivalent — it reads from and writes to `form` automatically:

```rust
// wham — form-integrated (recommended)
let path = FormPath::root().push("email");
ui.text_input_for(form, &path, "Email", "user@example.com");
// Changes are written back via form.set_value internally.
```

If you manage the buffer yourself (equivalent to an uncontrolled input that you sync manually):

```rust
// wham — manual buffer
ui.text_input("Email", &mut self.email_buf, "user@example.com");
if self.email_buf.text() != last_email {
    form.set_value(&email_path, FieldValue::Text(self.email_buf.text().into()));
}
```

---

## Validation

React Hook Form triggers validation on submit, on blur, or on change via `mode`.

In wham, call `form.validate()` explicitly (usually in a submit handler). Validation errors are written into `FieldState::errors` and you render them manually:

```rust
// wham
if ui.button("Submit") {
    match form.validate() {
        Ok(()) => { form.start_submit(payload, 3); }
        Err(errors) => { /* errors already in field state */ }
    }
}

// Render errors below the field:
let path = FormPath::root().push("email");
if let Some(field) = form.state().get_field(&path) {
    for error in &field.errors {
        ui.label_colored(error, Color::rgba(0.9, 0.2, 0.2, 1.0));
    }
}
```

Available validation rules:

| wham | React Hook Form equivalent |
|------|---------------------------|
| `ValidationRule::Required` | `{ required: true }` |
| `ValidationRule::Email` | `{ pattern: /email regex/ }` |
| `ValidationRule::Regex { pattern }` | `{ pattern: /your regex/ }` |
| `ValidationRule::NumberRange { min, max }` | `{ min, max }` |

---

## Form submission

React pattern:

```jsx
const onSubmit = async (data) => {
  const response = await fetch('/api', { body: JSON.stringify(data), ... });
};
<form onSubmit={handleSubmit(onSubmit)}>
```

wham pattern (optimistic submit):

```rust
// Trigger
if ui.button("Submit") {
    let payload = serde_json::json!({ "email": form.state().get_field(&email_path) });
    match form.start_submit(payload, 3) {
        Ok(FormEvent::SubmissionStarted(id)) => {
            // Send to server. When response arrives (JS → Wasm bridge):
            // form.apply_success(id) or form.apply_error(id, msg, rollback)
        }
        Err(FormEvent::ValidationFailed(_)) => {
            // Errors are already in field state; render them next frame.
        }
        _ => {}
    }
}

// Show a spinner while pending
if form.pending().is_some() {
    ui.label("Saving…");
}
```

On success/error the JS side calls back into Wasm:

```js
const response = await fetch('/api', { method: 'POST', body: payload });
if (response.ok) {
    app.apply_success(submissionId);
} else {
    app.apply_error(submissionId, await response.text(), true /* rollback */);
}
```

---

## No component state

React components own state via `useState` / `useReducer`. In wham, all mutable state lives in your application struct and is passed explicitly:

```rust
pub struct MyApp {
    email_buf: TextBuffer,
    accepted_terms: bool,
    selected_country: String,
    countries: Vec<String>,
}
```

Because the entire widget tree is rebuilt from this struct every frame, you never need to "lift state up" — it is already at the top.

---

## No event handlers on widgets

In React you attach `onChange`, `onBlur`, `onFocus`, and `onSubmit` to elements. In wham, widgets return a boolean that you check inline:

```rust
// wham equivalent of onChange={handler}
if ui.checkbox("Accept terms", &mut self.accepted) {
    // toggled this frame — run your handler here
}

// wham equivalent of onBlur — check focus state
if ui.focused_id() != last_focused {
    // focus changed
}
```

---

## Multi-step / wizard forms

React typically uses a `step` state variable and conditional rendering. In wham, use a plain enum:

```rust
enum Step { Details, Payment, Confirm }

fn build(&mut self, ui: &mut Ui, form: &mut Form) {
    match self.step {
        Step::Details  => self.build_details(ui, form),
        Step::Payment  => self.build_payment(ui, form),
        Step::Confirm  => self.build_confirm(ui, form),
    }
}
```

---

## Dynamic / repeating fields

React uses `useFieldArray` from React Hook Form. In wham use `FormSchema::repeatable_group` and `Form::add_repeat_group`:

```rust
// Schema
FormSchema::new("order")
    .repeatable_group("items", |g| {
        g.field("name", FieldType::Text)
         .field("qty",  FieldType::Number)
    })

// Add a row at runtime
if ui.button("Add item") {
    form.add_repeat_group(&items_path, item_fields.clone());
}

// Render rows
if let Some(field) = form.state().get_field(&items_path) {
    if let FieldValue::GroupList(rows) = &field.value {
        for (i, _row) in rows.iter().enumerate() {
            ui.push_id(i);
            let name_path = items_path.push(i.to_string()).push("name");
            ui.text_input_for(form, &name_path, "Name", "");
            ui.pop_id();
        }
    }
}
```

---

## Key differences summary

| Aspect | React / Vue | wham |
|--------|-------------|------|
| Widget model | Stateful components | Pure function calls |
| State ownership | In the component | In your app struct |
| Re-render trigger | State change | Every animation frame |
| Validation trigger | Lifecycle hooks | Explicit `form.validate()` |
| Event handlers | `onChange`, `onBlur` | Return value / inline check |
| DOM | Real DOM inputs | GPU quads on `<canvas>` |
| Bundle size | Varies (React ~40 KB) | WASM ~200 KB |
