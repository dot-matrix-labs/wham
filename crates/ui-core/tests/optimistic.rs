use ui_core::form::{FieldSchema, FieldType, FieldValue, Form, FormEvent, FormPath, FormSchema};
use ui_core::validation::ValidationRule;

#[test]
fn optimistic_submit_and_rollback() {
    let schema = FormSchema {
        fields: vec![FieldSchema {
            id: "name".into(),
            label: "Name".into(),
            field_type: FieldType::Text,
            rules: vec![ValidationRule::Required],
        }],
    };
    let mut form = Form::new(schema);
    let _ = form.set_value(&FormPath(vec!["name".into()]), FieldValue::Text("A".into()));
    let event = form.start_submit(serde_json::json!({ "name": "A" }), 1).unwrap();
    let submit_id = match event {
        FormEvent::SubmissionStarted(id) => id,
        _ => 0,
    };
    let original_field_count = form.state().fields().len();
    let _ = form.set_value(&FormPath(vec!["name".into()]), FieldValue::Text("B".into()));
    let rollback = form.apply_error(submit_id, "error", true);
    assert!(matches!(rollback, FormEvent::RolledBack(_)));
    assert_eq!(form.state().fields().len(), original_field_count);
}

