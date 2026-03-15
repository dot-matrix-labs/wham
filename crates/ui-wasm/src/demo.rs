use serde::Serialize;
use wasm_bindgen::JsValue;

use ui_core::batch::Batch;
use ui_core::form::{FieldSchema, FieldType, FieldValue, Form, FormEvent, FormPath, FormSchema};
use ui_core::input::{InputEvent, KeyCode, Modifiers, PointerButton, PointerEvent, TextInputEvent};
use ui_core::text::TextBuffer;
use ui_core::theme::Theme;
use ui_core::ui::Ui;
use ui_core::validation::ValidationRule;

#[derive(Clone, Copy, Debug)]
enum DemoMode {
    Login,
    Dynamic,
    Nested,
}

#[derive(Clone, Copy, Debug)]
enum FormKind {
    Login,
    Register,
    Dynamic,
    Nested,
}

#[derive(Clone, Debug)]
struct PendingMock {
    id: u64,
    complete_at: f64,
    form: FormKind,
    fail: bool,
}

pub struct FrameOutput {
    pub batch: Batch,
    pub a11y_json: JsValue,
}

pub struct DemoApp {
    ui: Ui,
    events: Vec<InputEvent>,
    mode: DemoMode,
    login_email: TextBuffer,
    login_password: TextBuffer,
    dynamic_name: TextBuffer,
    dynamic_age: TextBuffer,
    dynamic_bio: TextBuffer,
    dynamic_subscribe: bool,
    nested_name: TextBuffer,
    nested_email: TextBuffer,
    nested_contacts: Vec<(TextBuffer, TextBuffer)>,
    login_form: Form,
    register_form: Form,
    dynamic_form: Form,
    nested_form: Form,
    pending: Vec<PendingMock>,
    status: Option<String>,
    clipboard_request: Option<String>,
    auth_mode: usize,
    register_email: TextBuffer,
    register_password: TextBuffer,
    register_confirm: TextBuffer,
    register_role: String,
}

impl DemoApp {
    pub fn new(width: f32, height: f32) -> Self {
        let theme = Theme::default_light();
        Self {
            ui: Ui::new(width, height, theme),
            events: Vec::new(),
            mode: DemoMode::Login,
            login_email: TextBuffer::new(""),
            login_password: TextBuffer::new(""),
            dynamic_name: TextBuffer::new(""),
            dynamic_age: TextBuffer::new(""),
            dynamic_bio: TextBuffer::new(""),
            dynamic_subscribe: false,
            nested_name: TextBuffer::new(""),
            nested_email: TextBuffer::new(""),
            nested_contacts: Vec::new(),
            login_form: Form::new(login_schema()),
            register_form: Form::new(register_schema()),
            dynamic_form: Form::new(dynamic_schema()),
            nested_form: Form::new(nested_schema()),
            pending: Vec::new(),
            status: None,
            clipboard_request: None,
            auth_mode: 0,
            register_email: TextBuffer::new(""),
            register_password: TextBuffer::new(""),
            register_confirm: TextBuffer::new(""),
            register_role: "User".to_string(),
        }
    }

    pub fn frame(&mut self, width: f32, height: f32, scale: f32, timestamp_ms: f64) -> FrameOutput {
        self.resolve_pending(timestamp_ms);
        let events = std::mem::take(&mut self.events);
        self.ui.begin_frame(events, width, height, scale, timestamp_ms);

        self.ui.label("GPU Forms UI");
        if self.ui.button("Login/Register") {
            self.mode = DemoMode::Login;
        }
        if self.ui.button("Dynamic Validation") {
            self.mode = DemoMode::Dynamic;
        }
        if self.ui.button("Nested Groups") {
            self.mode = DemoMode::Nested;
        }

        match self.mode {
            DemoMode::Login => self.build_login(timestamp_ms),
            DemoMode::Dynamic => self.build_dynamic(timestamp_ms),
            DemoMode::Nested => self.build_nested(timestamp_ms),
        }

        if let Some(status) = &self.status {
            self.ui.label(status);
        }

        let a11y = self.ui.end_frame();
        self.clipboard_request = self.ui.clipboard_request.clone();
        let batch = std::mem::take(&mut self.ui.batch);
        let serializer =
            serde_wasm_bindgen::Serializer::new().serialize_large_number_types_as_bigints(true);
        let a11y_json = a11y.serialize(&serializer).unwrap_or(JsValue::NULL);
        FrameOutput {
            batch,
            a11y_json,
        }
    }

    fn build_login(&mut self, timestamp_ms: f64) {
        let options = vec!["Login".to_string(), "Register".to_string()];
        self.ui.radio_group("Auth Mode", &options, &mut self.auth_mode);
        if self.auth_mode == 0 {
            self.ui.label("Login");
            self.ui.text_input("Email", &mut self.login_email, "email@example.com");
            self.ui.text_input_masked("Password", &mut self.login_password, "password");
            if self.ui.button("Submit Login") {
                let mut form = self.login_form.clone();
                let _ = form.set_value(&FormPath(vec!["email".into()]), FieldValue::Text(self.login_email.text().to_string()));
                let _ = form.set_value(&FormPath(vec!["password".into()]), FieldValue::Text(self.login_password.text().to_string()));
                self.submit_form(FormKind::Login, &mut form, timestamp_ms);
                self.login_form = form;
            }
            self.ui.tooltip("Submit Login", "Sends an optimistic login request with retry/backoff.");
            let errors = Self::collect_errors(&self.login_form);
            let pending = Self::is_pending(&self.login_form);
            self.show_errors(&errors);
            self.show_loading(pending);
        } else {
            self.ui.label("Register");
            self.ui.text_input("Email", &mut self.register_email, "email@example.com");
            self.ui.text_input_masked("Password", &mut self.register_password, "password");
            self.ui.text_input_masked("Confirm Password", &mut self.register_confirm, "confirm");
            let roles = vec!["User".to_string(), "Admin".to_string(), "Viewer".to_string()];
            self.ui.select("Role", &roles, &mut self.register_role);
            if self.ui.button("Submit Register") {
                let mut form = self.register_form.clone();
                let _ = form.set_value(&FormPath(vec!["email".into()]), FieldValue::Text(self.register_email.text().to_string()));
                let _ = form.set_value(&FormPath(vec!["password".into()]), FieldValue::Text(self.register_password.text().to_string()));
                let _ = form.set_value(&FormPath(vec!["confirm".into()]), FieldValue::Text(self.register_confirm.text().to_string()));
                let _ = form.set_value(&FormPath(vec!["role".into()]), FieldValue::Selection(self.register_role.clone()));
                if self.register_password.text() != self.register_confirm.text() {
                    form.set_field_error(&FormPath(vec!["confirm".into()]), "Passwords do not match.");
                } else {
                    self.submit_form(FormKind::Register, &mut form, timestamp_ms);
                }
                self.register_form = form;
            }
            let errors = Self::collect_errors(&self.register_form);
            let pending = Self::is_pending(&self.register_form);
            self.show_errors(&errors);
            self.show_loading(pending);
        }
    }

    fn build_dynamic(&mut self, timestamp_ms: f64) {
        self.ui.label("Dynamic Validation");
        self.ui.text_input("Username", &mut self.dynamic_name, "user");
        self.ui.text_input("Age", &mut self.dynamic_age, "18");
        self.ui
            .text_input_multiline("Bio", &mut self.dynamic_bio, "multi-line bio", 80.0);
        self.ui.checkbox("Subscribe to updates", &mut self.dynamic_subscribe);
        if self.ui.button("Submit Profile") {
            let mut form = self.dynamic_form.clone();
            let _ = form.set_value(&FormPath(vec!["username".into()]), FieldValue::Text(self.dynamic_name.text().to_string()));
            let age = self.dynamic_age.text().parse::<f64>().unwrap_or(0.0);
            let _ = form.set_value(&FormPath(vec!["age".into()]), FieldValue::Number(age));
            let _ = form.set_value(&FormPath(vec!["bio".into()]), FieldValue::Text(self.dynamic_bio.text().to_string()));
            let _ = form.set_value(&FormPath(vec!["subscribe".into()]), FieldValue::Bool(self.dynamic_subscribe));
            self.submit_form(FormKind::Dynamic, &mut form, timestamp_ms);
            self.dynamic_form = form;
        }
        let errors = Self::collect_errors(&self.dynamic_form);
        let pending = Self::is_pending(&self.dynamic_form);
        self.show_errors(&errors);
        self.show_loading(pending);
    }

    fn build_nested(&mut self, timestamp_ms: f64) {
        self.ui.label("Nested Groups");
        self.ui.text_input("Full Name", &mut self.nested_name, "Jane Doe");
        self.ui.text_input("Contact Email", &mut self.nested_email, "jane@domain.com");
        if self.ui.button("Add Contact") {
            self.nested_contacts
                .push((TextBuffer::new(""), TextBuffer::new("")));
            let _ = self.nested_form.add_repeat_group(
                &FormPath(vec!["contacts".into()]),
                vec![
                    FieldSchema {
                        id: "label".into(),
                        label: "Label".into(),
                        field_type: FieldType::Text,
                        rules: vec![ValidationRule::Required],
                    },
                    FieldSchema {
                        id: "value".into(),
                        label: "Value".into(),
                        field_type: FieldType::Text,
                        rules: vec![ValidationRule::Email],
                    },
                ],
            );
        }
        for (idx, (label, value)) in self.nested_contacts.iter_mut().enumerate() {
            self.ui.push_id(idx);
            self.ui.label(&format!("Contact {}", idx + 1));
            self.ui.text_input("Label", label, "Work");
            self.ui.text_input("Email", value, "name@domain.com");
            let _ = self.nested_form.set_value(
                &FormPath(vec!["contacts".into(), idx.to_string(), "label".into()]),
                FieldValue::Text(label.text().to_string()),
            );
            let _ = self.nested_form.set_value(
                &FormPath(vec!["contacts".into(), idx.to_string(), "value".into()]),
                FieldValue::Text(value.text().to_string()),
            );
            self.ui.pop_id();
        }
        if self.ui.button("Submit Nested") {
            let mut form = self.nested_form.clone();
            let _ = form.set_value(&FormPath(vec!["profile".into(), "name".into()]), FieldValue::Text(self.nested_name.text().to_string()));
            let _ = form.set_value(&FormPath(vec!["profile".into(), "email".into()]), FieldValue::Text(self.nested_email.text().to_string()));
            self.submit_form(FormKind::Nested, &mut form, timestamp_ms);
            self.nested_form = form;
        }
        let errors = Self::collect_errors(&self.nested_form);
        let pending = Self::is_pending(&self.nested_form);
        self.show_errors(&errors);
        self.show_loading(pending);
    }

    fn submit_form(&mut self, kind: FormKind, form: &mut Form, timestamp_ms: f64) {
        let payload = serde_json::json!({ "timestamp": timestamp_ms });
        match form.start_submit(payload, 2) {
            Ok(FormEvent::SubmissionStarted(id)) => {
                self.pending.push(PendingMock {
                    id,
                    complete_at: timestamp_ms + 900.0,
                    form: kind,
                    fail: id % 2 == 0,
                });
                self.status = Some("Submitting...".to_string());
            }
            Err(FormEvent::ValidationFailed(errors)) => {
                self.status = Some(format!("Validation failed: {}", errors.len()));
            }
            _ => {}
        }
    }

    fn resolve_pending(&mut self, now: f64) {
        let mut remaining = Vec::new();
        for pending in &self.pending {
            if now >= pending.complete_at {
                let form = match pending.form {
                    FormKind::Login => &mut self.login_form,
                    FormKind::Register => &mut self.register_form,
                    FormKind::Dynamic => &mut self.dynamic_form,
                    FormKind::Nested => &mut self.nested_form,
                };
                if pending.fail {
                    let _ = form.apply_error(pending.id, "Server error", true);
                    self.status = Some("Server error, rolled back.".to_string());
                } else {
                    let _ = form.apply_success(pending.id);
                    self.status = Some("Saved successfully.".to_string());
                }
            } else {
                remaining.push(pending.clone());
            }
        }
        self.pending = remaining;
    }

    pub fn take_clipboard_request(&mut self) -> Option<String> {
        self.clipboard_request.take()
    }

    /// Returns the bounding rect (x, y, w, h) of the currently focused widget,
    /// or `None` if nothing is focused.
    pub fn focused_widget_rect(&self) -> Option<[f32; 4]> {
        self.ui.focused_widget_rect().map(|r| [r.x, r.y, r.w, r.h])
    }

    /// Returns `true` if any widget currently has focus.
    pub fn has_focused_widget(&self) -> bool {
        self.ui.focused.is_some()
    }

    /// Returns the kind of the focused widget as a string, or `None`.
    pub fn focused_widget_kind_str(&self) -> Option<&'static str> {
        use ui_core::ui::WidgetKind;
        self.ui.focused_widget_kind().map(|k| match k {
            WidgetKind::Label => "label",
            WidgetKind::Button => "button",
            WidgetKind::Checkbox => "checkbox",
            WidgetKind::Radio => "radio",
            WidgetKind::TextInput => "textinput",
            WidgetKind::Select => "select",
            WidgetKind::Group => "group",
        })
    }

    pub fn handle_pointer_down(&mut self, x: f32, y: f32, button: u16, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        let event = InputEvent::PointerDown(PointerEvent {
            pos: ui_core::types::Vec2::new(x, y),
            button: Some(map_button(button)),
            modifiers: Modifiers { ctrl, alt, shift, meta },
        });
        self.events.push(event);
    }

    pub fn handle_pointer_up(&mut self, x: f32, y: f32, button: u16, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        let event = InputEvent::PointerUp(PointerEvent {
            pos: ui_core::types::Vec2::new(x, y),
            button: Some(map_button(button)),
            modifiers: Modifiers { ctrl, alt, shift, meta },
        });
        self.events.push(event);
    }

    pub fn handle_pointer_move(&mut self, x: f32, y: f32, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        let event = InputEvent::PointerMove(PointerEvent {
            pos: ui_core::types::Vec2::new(x, y),
            button: None,
            modifiers: Modifiers { ctrl, alt, shift, meta },
        });
        self.events.push(event);
    }

    pub fn handle_wheel(&mut self, x: f32, y: f32, dx: f32, dy: f32, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        let event = InputEvent::PointerWheel {
            pos: ui_core::types::Vec2::new(x, y),
            delta: ui_core::types::Vec2::new(dx, dy),
            modifiers: Modifiers { ctrl, alt, shift, meta },
        };
        self.events.push(event);
    }

    pub fn handle_key_down(&mut self, code: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        let event = InputEvent::KeyDown {
            code: KeyCode::from_code_str(code),
            modifiers: Modifiers { ctrl, alt, shift, meta },
        };
        self.events.push(event);
    }

    pub fn handle_key_up(&mut self, code: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        let event = InputEvent::KeyUp {
            code: KeyCode::from_code_str(code),
            modifiers: Modifiers { ctrl, alt, shift, meta },
        };
        self.events.push(event);
    }

    pub fn handle_text_input(&mut self, text: String) {
        self.events.push(InputEvent::TextInput(TextInputEvent { text }));
    }

    pub fn handle_composition_start(&mut self) {
        self.events.push(InputEvent::CompositionStart);
    }

    pub fn handle_composition_update(&mut self, text: String) {
        self.events.push(InputEvent::CompositionUpdate(text));
    }

    pub fn handle_composition_end(&mut self, text: String) {
        self.events.push(InputEvent::CompositionEnd(text));
    }

    pub fn handle_paste(&mut self, text: String) {
        self.events.push(InputEvent::Paste(text));
    }

    fn collect_errors(form: &Form) -> Vec<String> {
        form.state
            .fields
            .values()
            .flat_map(|field| field.errors.iter().cloned())
            .collect()
    }

    fn is_pending(form: &Form) -> bool {
        form.state.fields.values().any(|field| field.pending)
    }

    fn show_errors(&mut self, errors: &[String]) {
        for error in errors {
            self.ui.label_colored(error, self.ui.theme.colors.error);
        }
    }

    fn show_loading(&mut self, pending: bool) {
        if pending {
            self.ui.label_colored("Loading...", self.ui.theme.colors.primary);
        }
    }
}

fn map_button(button: u16) -> PointerButton {
    match button {
        0 => PointerButton::Left,
        1 => PointerButton::Middle,
        2 => PointerButton::Right,
        other => PointerButton::Other(other),
    }
}


fn login_schema() -> FormSchema {
    FormSchema {
        fields: vec![
            FieldSchema {
                id: "email".into(),
                label: "Email".into(),
                field_type: FieldType::Text,
                rules: vec![ValidationRule::Required, ValidationRule::Email],
            },
            FieldSchema {
                id: "password".into(),
                label: "Password".into(),
                field_type: FieldType::Text,
                rules: vec![ValidationRule::Required],
            },
        ],
    }
}

fn register_schema() -> FormSchema {
    FormSchema {
        fields: vec![
            FieldSchema {
                id: "email".into(),
                label: "Email".into(),
                field_type: FieldType::Text,
                rules: vec![ValidationRule::Required, ValidationRule::Email],
            },
            FieldSchema {
                id: "password".into(),
                label: "Password".into(),
                field_type: FieldType::Text,
                rules: vec![ValidationRule::Required],
            },
            FieldSchema {
                id: "confirm".into(),
                label: "Confirm Password".into(),
                field_type: FieldType::Text,
                rules: vec![ValidationRule::Required],
            },
            FieldSchema {
                id: "role".into(),
                label: "Role".into(),
                field_type: FieldType::Select {
                    options: vec!["User".into(), "Admin".into(), "Viewer".into()],
                },
                rules: vec![],
            },
        ],
    }
}

fn dynamic_schema() -> FormSchema {
    FormSchema {
        fields: vec![
            FieldSchema {
                id: "username".into(),
                label: "Username".into(),
                field_type: FieldType::Text,
                rules: vec![
                    ValidationRule::Required,
                    ValidationRule::Regex {
                        pattern: "^[a-zA-Z0-9_]{3,16}$".into(),
                    },
                ],
            },
            FieldSchema {
                id: "age".into(),
                label: "Age".into(),
                field_type: FieldType::Number,
                rules: vec![ValidationRule::NumberRange {
                    min: Some(13.0),
                    max: Some(120.0),
                }],
            },
            FieldSchema {
                id: "bio".into(),
                label: "Bio".into(),
                field_type: FieldType::Text,
                rules: vec![],
            },
            FieldSchema {
                id: "subscribe".into(),
                label: "Subscribe".into(),
                field_type: FieldType::Checkbox,
                rules: vec![],
            },
        ],
    }
}

fn nested_schema() -> FormSchema {
    FormSchema {
        fields: vec![FieldSchema {
            id: "profile".into(),
            label: "Profile".into(),
            field_type: FieldType::Group {
                repeatable: false,
                fields: vec![
                    FieldSchema {
                        id: "name".into(),
                        label: "Full Name".into(),
                        field_type: FieldType::Text,
                        rules: vec![ValidationRule::Required],
                    },
                    FieldSchema {
                        id: "email".into(),
                        label: "Contact Email".into(),
                        field_type: FieldType::Text,
                        rules: vec![ValidationRule::Email],
                    },
                ],
            },
            rules: vec![],
        },
        FieldSchema {
            id: "contacts".into(),
            label: "Contacts".into(),
            field_type: FieldType::Group {
                repeatable: true,
                fields: vec![
                    FieldSchema {
                        id: "label".into(),
                        label: "Label".into(),
                        field_type: FieldType::Text,
                        rules: vec![ValidationRule::Required],
                    },
                    FieldSchema {
                        id: "value".into(),
                        label: "Value".into(),
                        field_type: FieldType::Text,
                        rules: vec![ValidationRule::Email],
                    },
                ],
            },
            rules: vec![],
        }],
    }
}
