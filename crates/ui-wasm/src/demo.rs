use ui_core::app::FormApp;
use ui_core::form::{
    FieldSchema, FieldType, FieldValue, Form, FormEvent, FormPath, FormSchema,
};
use ui_core::text::TextBuffer;
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

pub struct DemoApp {
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
    auth_mode: usize,
    register_email: TextBuffer,
    register_password: TextBuffer,
    register_confirm: TextBuffer,
    register_role: String,
}

impl DemoApp {
    pub fn new() -> Self {
        Self {
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
            auth_mode: 0,
            register_email: TextBuffer::new(""),
            register_password: TextBuffer::new(""),
            register_confirm: TextBuffer::new(""),
            register_role: "User".to_string(),
        }
    }

    fn build_login(&mut self, ui: &mut Ui, timestamp_ms: f64) {
        let options = vec!["Login".to_string(), "Register".to_string()];
        ui.radio_group("Auth Mode", &options, &mut self.auth_mode);
        if self.auth_mode == 0 {
            ui.label("Login");
            ui.text_input("Email", &mut self.login_email, "email@example.com");
            ui.text_input_masked("Password", &mut self.login_password, "password");
            if ui.button("Submit Login") {
                let mut form = self.login_form.clone();
                let _ = form.set_value(
                    &FormPath(vec!["email".into()]),
                    FieldValue::Text(self.login_email.text().to_string()),
                );
                let _ = form.set_value(
                    &FormPath(vec!["password".into()]),
                    FieldValue::Text(self.login_password.text().to_string()),
                );
                self.submit_form(FormKind::Login, &mut form, timestamp_ms);
                self.login_form = form;
            }
            ui.tooltip(
                "Submit Login",
                "Sends an optimistic login request with retry/backoff.",
            );
            let errors = Self::collect_errors(&self.login_form);
            let pending = Self::is_pending(&self.login_form);
            self.show_errors(ui, &errors);
            self.show_loading(ui, pending);
        } else {
            ui.label("Register");
            ui.text_input("Email", &mut self.register_email, "email@example.com");
            ui.text_input_masked("Password", &mut self.register_password, "password");
            ui.text_input_masked(
                "Confirm Password",
                &mut self.register_confirm,
                "confirm",
            );
            let roles = vec![
                "User".to_string(),
                "Admin".to_string(),
                "Viewer".to_string(),
            ];
            ui.select("Role", &roles, &mut self.register_role);
            if ui.button("Submit Register") {
                let mut form = self.register_form.clone();
                let _ = form.set_value(
                    &FormPath(vec!["email".into()]),
                    FieldValue::Text(self.register_email.text().to_string()),
                );
                let _ = form.set_value(
                    &FormPath(vec!["password".into()]),
                    FieldValue::Text(self.register_password.text().to_string()),
                );
                let _ = form.set_value(
                    &FormPath(vec!["confirm".into()]),
                    FieldValue::Text(self.register_confirm.text().to_string()),
                );
                let _ = form.set_value(
                    &FormPath(vec!["role".into()]),
                    FieldValue::Selection(self.register_role.clone()),
                );
                if self.register_password.text() != self.register_confirm.text() {
                    form.set_field_error(
                        &FormPath(vec!["confirm".into()]),
                        "Passwords do not match.",
                    );
                } else {
                    self.submit_form(FormKind::Register, &mut form, timestamp_ms);
                }
                self.register_form = form;
            }
            let errors = Self::collect_errors(&self.register_form);
            let pending = Self::is_pending(&self.register_form);
            self.show_errors(ui, &errors);
            self.show_loading(ui, pending);
        }
    }

    fn build_dynamic(&mut self, ui: &mut Ui, timestamp_ms: f64) {
        ui.label("Dynamic Validation");
        ui.text_input("Username", &mut self.dynamic_name, "user");
        ui.text_input("Age", &mut self.dynamic_age, "18");
        ui.text_input_multiline("Bio", &mut self.dynamic_bio, "multi-line bio", 80.0);
        ui.checkbox("Subscribe to updates", &mut self.dynamic_subscribe);
        if ui.button("Submit Profile") {
            let mut form = self.dynamic_form.clone();
            let _ = form.set_value(
                &FormPath(vec!["username".into()]),
                FieldValue::Text(self.dynamic_name.text().to_string()),
            );
            let age = self.dynamic_age.text().parse::<f64>().unwrap_or(0.0);
            let _ = form.set_value(&FormPath(vec!["age".into()]), FieldValue::Number(age));
            let _ = form.set_value(
                &FormPath(vec!["bio".into()]),
                FieldValue::Text(self.dynamic_bio.text().to_string()),
            );
            let _ = form.set_value(
                &FormPath(vec!["subscribe".into()]),
                FieldValue::Bool(self.dynamic_subscribe),
            );
            self.submit_form(FormKind::Dynamic, &mut form, timestamp_ms);
            self.dynamic_form = form;
        }
        let errors = Self::collect_errors(&self.dynamic_form);
        let pending = Self::is_pending(&self.dynamic_form);
        self.show_errors(ui, &errors);
        self.show_loading(ui, pending);
    }

    fn build_nested(&mut self, ui: &mut Ui, timestamp_ms: f64) {
        ui.label("Nested Groups");
        ui.text_input("Full Name", &mut self.nested_name, "Jane Doe");
        ui.text_input(
            "Contact Email",
            &mut self.nested_email,
            "jane@domain.com",
        );
        if ui.button("Add Contact") {
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
                        placeholder: None,
                    },
                    FieldSchema {
                        id: "value".into(),
                        label: "Value".into(),
                        field_type: FieldType::Text,
                        rules: vec![ValidationRule::Email],
                        placeholder: None,
                    },
                ],
            );
        }
        for (idx, (label, value)) in self.nested_contacts.iter_mut().enumerate() {
            ui.push_id(idx);
            ui.label(&format!("Contact {}", idx + 1));
            ui.text_input("Label", label, "Work");
            ui.text_input("Email", value, "name@domain.com");
            let _ = self.nested_form.set_value(
                &FormPath(vec![
                    "contacts".into(),
                    idx.to_string(),
                    "label".into(),
                ]),
                FieldValue::Text(label.text().to_string()),
            );
            let _ = self.nested_form.set_value(
                &FormPath(vec![
                    "contacts".into(),
                    idx.to_string(),
                    "value".into(),
                ]),
                FieldValue::Text(value.text().to_string()),
            );
            ui.pop_id();
        }
        if ui.button("Submit Nested") {
            let mut form = self.nested_form.clone();
            let _ = form.set_value(
                &FormPath(vec!["profile".into(), "name".into()]),
                FieldValue::Text(self.nested_name.text().to_string()),
            );
            let _ = form.set_value(
                &FormPath(vec!["profile".into(), "email".into()]),
                FieldValue::Text(self.nested_email.text().to_string()),
            );
            self.submit_form(FormKind::Nested, &mut form, timestamp_ms);
            self.nested_form = form;
        }
        let errors = Self::collect_errors(&self.nested_form);
        let pending = Self::is_pending(&self.nested_form);
        self.show_errors(ui, &errors);
        self.show_loading(ui, pending);
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

    fn collect_errors(form: &Form) -> Vec<String> {
        form.state()
            .fields()
            .values()
            .flat_map(|field| field.errors.iter().cloned())
            .collect()
    }

    fn is_pending(form: &Form) -> bool {
        form.state().fields().values().any(|field| field.pending)
    }

    fn show_errors(&mut self, ui: &mut Ui, errors: &[String]) {
        for error in errors {
            let color = ui.theme().colors.error;
            ui.label_colored(error, color);
        }
    }

    fn show_loading(&mut self, ui: &mut Ui, pending: bool) {
        if pending {
            let color = ui.theme().colors.primary;
            ui.label_colored("Loading...", color);
        }
    }
}

impl FormApp for DemoApp {
    fn build(&mut self, ui: &mut Ui, _form: &mut Form) {
        let timestamp_ms = ui.time_ms();
        self.resolve_pending(timestamp_ms);

        ui.label("GPU Forms UI");
        if ui.button("Login/Register") {
            self.mode = DemoMode::Login;
        }
        if ui.button("Dynamic Validation") {
            self.mode = DemoMode::Dynamic;
        }
        if ui.button("Nested Groups") {
            self.mode = DemoMode::Nested;
        }

        match self.mode {
            DemoMode::Login => self.build_login(ui, timestamp_ms),
            DemoMode::Dynamic => self.build_dynamic(ui, timestamp_ms),
            DemoMode::Nested => self.build_nested(ui, timestamp_ms),
        }

        if let Some(status) = &self.status {
            ui.label(status);
        }
    }

    fn schema(&self) -> FormSchema {
        // The DemoApp manages multiple forms internally; we return the
        // login schema as the "primary" form for the runtime. The other
        // forms are owned by DemoApp itself.
        login_schema()
    }
}

fn login_schema() -> FormSchema {
    FormSchema::new("login")
        .field("email", FieldType::Text)
        .with_label("email", "Email")
        .required("email")
        .with_validation("email", ValidationRule::Email)
        .field("password", FieldType::Text)
        .with_label("password", "Password")
        .required("password")
}

fn register_schema() -> FormSchema {
    FormSchema::new("register")
        .field("email", FieldType::Text)
        .with_label("email", "Email")
        .required("email")
        .with_validation("email", ValidationRule::Email)
        .field("password", FieldType::Text)
        .with_label("password", "Password")
        .required("password")
        .field("confirm", FieldType::Text)
        .with_label("confirm", "Confirm Password")
        .required("confirm")
        .field("role", FieldType::Select {
            options: vec!["User".into(), "Admin".into(), "Viewer".into()],
        })
        .with_label("role", "Role")
}

fn dynamic_schema() -> FormSchema {
    FormSchema::new("dynamic")
        .field("username", FieldType::Text)
        .with_label("username", "Username")
        .required("username")
        .with_validation("username", ValidationRule::Regex {
            pattern: "^[a-zA-Z0-9_]{3,16}$".into(),
        })
        .field("age", FieldType::Number)
        .with_label("age", "Age")
        .with_validation("age", ValidationRule::NumberRange {
            min: Some(13.0),
            max: Some(120.0),
        })
        .field("bio", FieldType::Text)
        .with_label("bio", "Bio")
        .field("subscribe", FieldType::Checkbox)
        .with_label("subscribe", "Subscribe")
}

fn nested_schema() -> FormSchema {
    FormSchema::new("nested")
        .group("profile", |s| {
            s.field("name", FieldType::Text)
                .with_label("name", "Full Name")
                .required("name")
                .field("email", FieldType::Text)
                .with_label("email", "Contact Email")
                .with_validation("email", ValidationRule::Email)
        })
        .repeatable_group("contacts", |s| {
            s.field("label", FieldType::Text)
                .with_label("label", "Label")
                .required("label")
                .field("value", FieldType::Text)
                .with_label("value", "Value")
                .with_validation("value", ValidationRule::Email)
        })
}
