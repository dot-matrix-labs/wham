use crate::form::{Form, FormSchema};
use crate::ui::Ui;

/// Trait for building form-based applications on top of the GPU-rendered UI.
///
/// Implement this trait to define a form application without forking the demo.
/// The runtime calls [`build`] each frame to construct the immediate-mode UI,
/// [`schema`] once at startup to create the [`Form`], and [`on_submit`] when
/// the user submits.
///
/// # Example
///
/// ```ignore
/// struct MyApp;
///
/// impl FormApp for MyApp {
///     fn schema(&self) -> FormSchema {
///         FormSchema::new("my_form")
///     }
///
///     fn build(&mut self, ui: &mut Ui, form: &mut Form) {
///         ui.label("Hello, forms!");
///     }
/// }
/// ```
pub trait FormApp {
    /// Called once to define the form schema.
    ///
    /// The returned [`FormSchema`] is used to create the [`Form`] that gets
    /// passed to [`build`](FormApp::build) each frame.
    fn schema(&self) -> FormSchema;

    /// Called each frame to build the form UI using immediate-mode widgets.
    ///
    /// Use `ui` to emit widgets and `form` to read/write field state.
    fn build(&mut self, ui: &mut Ui, form: &mut Form);

    /// Called when the form is submitted.
    ///
    /// Return `Ok(())` to indicate success, or `Err(message)` to display an
    /// error and keep the form state. The default implementation succeeds
    /// immediately.
    fn on_submit(&mut self, form: &Form) -> Result<(), String> {
        let _ = form;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::form::{FieldSchema, FieldType, FieldValue, FormPath, FormSchema};
    use crate::theme::Theme;
    use crate::validation::ValidationRule;

    /// A minimal FormApp implementation for testing.
    struct TestApp {
        build_called: bool,
        submit_result: Result<(), String>,
    }

    impl TestApp {
        fn new() -> Self {
            Self {
                build_called: false,
                submit_result: Ok(()),
            }
        }

        fn failing() -> Self {
            Self {
                build_called: false,
                submit_result: Err("test error".to_string()),
            }
        }
    }

    impl FormApp for TestApp {
        fn schema(&self) -> FormSchema {
            FormSchema::new("test")
                .field("name", FieldType::Text)
                .with_label("name", "Name")
                .required("name")
        }

        fn build(&mut self, ui: &mut Ui, form: &mut Form) {
            self.build_called = true;
            ui.label("Test Form");
            let path = FormPath::root().push("name");
            if let Some(field) = form.state().fields().get(&path) {
                if let FieldValue::Text(ref v) = field.value {
                    ui.label(v);
                }
            }
        }

        fn on_submit(&mut self, _form: &Form) -> Result<(), String> {
            self.submit_result.clone()
        }
    }

    #[test]
    fn schema_returns_valid_form_schema() {
        let app = TestApp::new();
        let schema = app.schema();
        assert_eq!(schema.fields.len(), 1);
        assert_eq!(schema.fields[0].id, "name");
    }

    #[test]
    fn build_receives_ui_and_form() {
        let mut app = TestApp::new();
        let schema = app.schema();
        let mut form = Form::new(schema);
        let theme = Theme::default_light();
        let mut ui = Ui::new(800.0, 600.0, theme);
        let events = Vec::new();
        ui.begin_frame(events, 800.0, 600.0, 1.0, 0.0);

        app.build(&mut ui, &mut form);
        assert!(app.build_called);
    }

    #[test]
    fn on_submit_default_succeeds() {
        struct MinimalApp;
        impl FormApp for MinimalApp {
            fn schema(&self) -> FormSchema {
                FormSchema::new("minimal")
            }
            fn build(&mut self, _ui: &mut Ui, _form: &mut Form) {}
        }

        let mut app = MinimalApp;
        let form = Form::new(app.schema());
        assert!(app.on_submit(&form).is_ok());
    }

    #[test]
    fn on_submit_can_return_error() {
        let mut app = TestApp::failing();
        let form = Form::new(app.schema());
        let result = app.on_submit(&form);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "test error");
    }

    #[test]
    fn trait_object_is_usable() {
        let mut app = TestApp::new();
        let schema = app.schema();
        let mut form = Form::new(schema);
        let theme = Theme::default_light();
        let mut ui = Ui::new(800.0, 600.0, theme);
        ui.begin_frame(Vec::new(), 800.0, 600.0, 1.0, 0.0);

        // Verify it works through a trait object reference
        let app_ref: &mut dyn FormApp = &mut app;
        app_ref.build(&mut ui, &mut form);
        assert!(app.build_called);
    }
}
