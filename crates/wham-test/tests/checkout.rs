//! Checkout form scenario tests.
//!
//! Models an e-commerce checkout screen with contact info, shipping address,
//! and a payment section — demonstrating multi-column layout, dropdown selects,
//! and form-bound text inputs across several sections.

use ui_core::{
    form::{FieldType, Form, FormPath, FormSchema},
    theme::Theme,
    ui::{Ui, WidgetKind},
};
use wham_test::Size;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Run a single headless frame and return the widgets.
fn run_once(
    size: Size,
    build: impl FnOnce(&mut Ui),
) -> (Vec<ui_core::ui::WidgetInfo>, Vec<ui_core::batch::TextRun>) {
    let w = size.width as f32;
    let h = size.height as f32;
    let mut ui = Ui::new(w, h, Theme::default_light());
    ui.begin_frame(vec![], w, h, 1.0, 0.0);
    build(&mut ui);
    ui.end_frame();
    let widgets = ui.widgets().to_vec();
    let text_runs = ui.batch().text_runs.clone();
    (widgets, text_runs)
}

fn schema() -> FormSchema {
    let countries = vec![
        "United States".to_string(),
        "Canada".to_string(),
        "United Kingdom".to_string(),
        "Australia".to_string(),
        "Germany".to_string(),
    ];
    FormSchema::new("checkout")
        .field("first_name", FieldType::Text)
        .with_label("first_name", "First name")
        .required("first_name")
        .field("last_name", FieldType::Text)
        .with_label("last_name", "Last name")
        .required("last_name")
        .field("email", FieldType::Text)
        .with_label("email", "Email address")
        .required("email")
        .field("address", FieldType::Text)
        .with_label("address", "Street address")
        .field("city", FieldType::Text)
        .with_label("city", "City")
        .field("postal", FieldType::Text)
        .with_label("postal", "Postal code")
        .field("country", FieldType::Select { options: countries })
        .with_label("country", "Country")
        .field("card_number", FieldType::Text)
        .with_label("card_number", "Card number")
        .with_placeholder("card_number", "1234 5678 9012 3456")
        .field("expiry", FieldType::Text)
        .with_label("expiry", "Expiry")
        .with_placeholder("expiry", "MM / YY")
        .field("cvv", FieldType::Text)
        .with_label("cvv", "CVV")
        .with_placeholder("cvv", "•••")
}

fn checkout_view(ui: &mut Ui, form: &mut Form, country: &mut String) {
    // Contact
    ui.label("Contact information");
    ui.begin_row_with(&[1.0, 1.0]);
    ui.text_input_for(form, &FormPath::root().push("first_name"), "First name", "Jane");
    ui.text_input_for(form, &FormPath::root().push("last_name"), "Last name", "Smith");
    ui.end_row();
    ui.text_input_for(form, &FormPath::root().push("email"), "Email address", "jane@example.com");

    // Shipping
    ui.label("Shipping address");
    ui.text_input_for(form, &FormPath::root().push("address"), "Street address", "123 Main St");
    ui.begin_row_with(&[2.0, 1.0]);
    ui.text_input_for(form, &FormPath::root().push("city"), "City", "San Francisco");
    ui.text_input_for(form, &FormPath::root().push("postal"), "Postal code", "94105");
    ui.end_row();

    let countries = vec![
        "United States".to_string(),
        "Canada".to_string(),
        "United Kingdom".to_string(),
        "Australia".to_string(),
        "Germany".to_string(),
    ];
    ui.select("Country", &countries, country);

    // Payment
    ui.label("Payment");
    ui.text_input_for(form, &FormPath::root().push("card_number"), "Card number", "1234 5678 9012 3456");
    ui.begin_row_with(&[2.0, 1.0]);
    ui.text_input_for(form, &FormPath::root().push("expiry"), "Expiry", "MM / YY");
    ui.text_input_for(form, &FormPath::root().push("cvv"), "CVV", "\u{2022}\u{2022}\u{2022}");
    ui.end_row();

    ui.button("Place order \u{2014} $129.00");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn checkout_contact_section_renders_all_fields() {
    let size = Size { width: 640, height: 900 };
    let mut form = Form::new(schema());
    let mut country = "United States".to_string();

    let (widgets, _) = run_once(size, |ui| checkout_view(ui, &mut form, &mut country));

    let text_inputs: Vec<_> = widgets.iter().filter(|w| w.kind == WidgetKind::TextInput).collect();
    // 9 text fields: first name, last name, email, address, city, postal, card, expiry, cvv
    assert!(text_inputs.len() >= 9, "expected at least 9 text input widgets, got {}", text_inputs.len());

    let buttons: Vec<_> = widgets.iter().filter(|w| w.kind == WidgetKind::Button).collect();
    assert_eq!(buttons.len(), 1, "one submit button");

    let selects: Vec<_> = widgets.iter().filter(|w| w.kind == WidgetKind::Select).collect();
    assert_eq!(selects.len(), 1, "one country select");
}

#[test]
fn checkout_two_column_name_row_has_distinct_x_positions() {
    let size = Size { width: 640, height: 900 };
    let mut form = Form::new(schema());
    let mut country = "United States".to_string();

    let (widgets, _) = run_once(size, |ui| checkout_view(ui, &mut form, &mut country));

    let first_name = widgets.iter().find(|w| w.label == "First name")
        .expect("First name widget not found");
    let last_name = widgets.iter().find(|w| w.label == "Last name")
        .expect("Last name widget not found");

    assert_ne!(
        first_name.rect.x as i32,
        last_name.rect.x as i32,
        "first name and last name must be at different x positions (side by side)"
    );
    assert_eq!(
        first_name.rect.y as i32,
        last_name.rect.y as i32,
        "first name and last name must be at the same y position (same row)"
    );
}

#[test]
fn checkout_country_select_renders_with_label() {
    let size = Size { width: 640, height: 900 };
    let mut form = Form::new(schema());
    let mut country = "United States".to_string();

    let (widgets, text_runs) = run_once(size, |ui| checkout_view(ui, &mut form, &mut country));

    let select = widgets.iter().find(|w| w.label == "Country")
        .expect("Country select widget not found");
    assert_eq!(select.kind, WidgetKind::Select);
    assert_eq!(select.value.as_deref(), Some("United States"),
        "initial country value should be first option");

    // The select widget renders a text run showing label + value
    let has_country_text = text_runs.iter().any(|r| r.text.contains("Country"));
    assert!(has_country_text, "select should emit a text run containing its label");
}

#[test]
fn checkout_submit_button_is_last_in_tab_order() {
    let size = Size { width: 640, height: 900 };
    let mut form = Form::new(schema());
    let mut country = "United States".to_string();

    let (widgets, _) = run_once(size, |ui| checkout_view(ui, &mut form, &mut country));

    let last = widgets.last().expect("no widgets emitted");
    assert_eq!(last.kind, WidgetKind::Button, "last widget should be the submit button");
    assert!(last.label.contains("Place order"), "button label should reference the action");
}
