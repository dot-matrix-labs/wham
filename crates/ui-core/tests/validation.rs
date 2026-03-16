use ui_core::form::{FieldSchema, FieldType, FieldValue, Form, FormPath, FormSchema};
use ui_core::validation::ValidationRule;

#[test]
fn required_and_email_validation() {
    let schema = FormSchema {
        name: "test".to_string(),
        fields: vec![FieldSchema {
            id: "email".into(),
            label: "Email".into(),
            field_type: FieldType::Text,
            rules: vec![ValidationRule::Required, ValidationRule::Email],
            placeholder: None,
            autocomplete: None,
        }],
    };
    let mut form = Form::new(schema);
    let _ = form.set_value(&FormPath(vec!["email".into()]), FieldValue::Text("".into()));
    let err = form.validate().unwrap_err();
    assert!(err.len() >= 1);

    let _ = form.set_value(&FormPath(vec!["email".into()]), FieldValue::Text("user@example.com".into()));
    assert!(form.validate().is_ok());
}
