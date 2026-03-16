//! Sign-in form scenario tests.
//!
//! Models a banking or SaaS sign-in screen: a heading, email input, masked
//! password input, and a primary "Sign in" button.  Each test exercises a
//! different aspect of the library that downstream consumers depend on.

use ui_core::{
    form::{FieldType, FieldValue, Form, FormPath, FormSchema},
    ui::WidgetKind,
};
use wham_test::{click_at, type_text, Session, Size};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn schema() -> FormSchema {
    FormSchema::new("sign-in")
        .field("email", FieldType::Text)
        .with_label("email", "Email")
        .with_placeholder("email", "you@example.com")
        .required("email")
        .field("password", FieldType::Text)
        .with_label("password", "Password")
        .required("password")
}

fn render(ui: &mut ui_core::ui::Ui, form: &mut Form) {
    ui.label("Welcome back");
    ui.text_input_for(form, &FormPath::root().push("email"), "Email", "you@example.com");
    ui.text_input_masked_for(form, &FormPath::root().push("password"), "Password", "");
    ui.button("Sign in");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn sign_in_emits_correct_widget_tree() {
    let mut session = Session::new(Size { width: 480, height: 640 });
    let mut form = Form::new(schema());

    let frame = session.next_frame(vec![], 0.0, |ui| render(ui, &mut form));

    assert_eq!(frame.count_kind(WidgetKind::Label), 1, "one heading label");
    assert_eq!(frame.count_kind(WidgetKind::TextInput), 2, "email + password inputs");
    assert_eq!(frame.count_kind(WidgetKind::Button), 1, "one submit button");
}

#[test]
fn sign_in_placeholder_text_is_rendered() {
    let mut session = Session::new(Size { width: 480, height: 640 });
    let mut form = Form::new(schema());

    let frame = session.next_frame(vec![], 0.0, |ui| render(ui, &mut form));

    assert!(frame.has_text("Welcome back"), "heading must appear in text runs");
    assert!(frame.has_text("you@example.com"), "email placeholder must appear");
    assert!(frame.has_text("Sign in"), "button label must appear");
}

#[test]
fn sign_in_button_fires_on_click() {
    let mut session = Session::new(Size { width: 480, height: 640 });
    let mut form = Form::new(schema());

    // Frame 1: render to discover the button's position.
    let layout = session.next_frame(vec![], 0.0, |ui| render(ui, &mut form));
    let btn_center = layout.widget("Sign in").expect("button not found").rect.center();

    // Frame 2: click the button.
    let mut fired = false;
    session.next_frame(click_at(btn_center), 16.0, |ui| {
        ui.label("Welcome back");
        ui.text_input_for(&mut form, &FormPath::root().push("email"), "Email", "you@example.com");
        ui.text_input_masked_for(&mut form, &FormPath::root().push("password"), "Password", "");
        fired = ui.button("Sign in");
    });

    assert!(fired, "button must fire on pointer click");
}

#[test]
fn sign_in_password_field_masks_input() {
    let mut session = Session::new(Size { width: 480, height: 640 });
    let mut form = Form::new(schema());

    // Pre-fill the password via the form model before the first render.
    // `text_input_for` initialises its internal buffer from form state on
    // first use, so the text run will reflect this value immediately.
    form.set_value(
        &FormPath::root().push("password"),
        FieldValue::Text("hunter2".into()),
    );

    let frame = session.next_frame(vec![], 0.0, |ui| render(ui, &mut form));

    assert!(
        !frame.has_text("hunter2"),
        "password plaintext must not appear in rendered text runs"
    );
    assert!(
        frame.has_text("\u{2022}"),
        "masked password must render as bullet characters (U+2022)"
    );
}

#[test]
fn sign_in_typing_email_syncs_to_form() {
    let mut session = Session::new(Size { width: 480, height: 640 });
    let mut form = Form::new(schema());

    // Frame 1: render to get the email input's position.
    let layout = session.next_frame(vec![], 0.0, |ui| render(ui, &mut form));
    let email_center = layout.widget("Email").expect("email widget not found").rect.center();

    // Frame 2: click the email field to focus it.
    session.next_frame(click_at(email_center), 16.0, |ui| render(ui, &mut form));

    // Frame 3: type an email address character by character.
    session.next_frame(type_text("alice@example.com"), 32.0, |ui| render(ui, &mut form));

    // The form model must now reflect the typed value.
    let path = FormPath::root().push("email");
    let field = form.state().get_field(&path).expect("email field missing from form state");
    match &field.value {
        FieldValue::Text(s) => assert_eq!(s, "alice@example.com"),
        other => panic!("expected FieldValue::Text, got {:?}", other),
    }
}
