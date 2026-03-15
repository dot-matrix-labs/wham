use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::state::History;
use crate::validation::{validate_value, ValidationError, ValidationRule};

pub type FieldId = String;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FormPath(pub Vec<FieldId>);

impl PartialEq for FormPath {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for FormPath {}

impl Hash for FormPath {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for part in &self.0 {
            part.hash(state);
        }
    }
}

impl FormPath {
    pub fn root() -> Self {
        FormPath(Vec::new())
    }

    pub fn push(&self, id: impl Into<FieldId>) -> Self {
        let mut next = self.0.clone();
        next.push(id.into());
        FormPath(next)
    }

    pub fn as_string(&self) -> String {
        if self.0.is_empty() {
            "root".to_string()
        } else {
            self.0.join(".")
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FieldValue {
    Text(String),
    Number(f64),
    Bool(bool),
    Selection(String),
    Group(HashMap<FieldId, FieldValue>),
    GroupList(Vec<HashMap<FieldId, FieldValue>>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FieldState {
    pub value: FieldValue,
    pub errors: Vec<String>,
    pub touched: bool,
    pub pending: bool,
    pub disabled: bool,
    pub dirty: bool,
}

impl FieldState {
    pub fn new(value: FieldValue) -> Self {
        Self {
            value,
            errors: Vec::new(),
            touched: false,
            pending: false,
            disabled: false,
            dirty: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FieldType {
    Text,
    Number,
    Checkbox,
    Select { options: Vec<String> },
    Group { fields: Vec<FieldSchema>, repeatable: bool },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FieldSchema {
    pub id: FieldId,
    pub label: String,
    pub field_type: FieldType,
    pub rules: Vec<ValidationRule>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FormSchema {
    pub fields: Vec<FieldSchema>,
}

/// Holds the current state of all form fields, keyed by path.
///
/// `FormState` is cloned only for history snapshots (undo/redo) and submission
/// rollback — not on the per-frame rendering hot path. A plain `HashMap` is used
/// instead of a persistent data structure because each mutation already clones
/// the entire state into a new `Arc<FormState>`, so structural sharing from
/// `im::HashMap` provided no benefit while adding overhead.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FormState {
    fields: HashMap<FormPath, FieldState>,
}

impl FormState {
    pub fn empty() -> Self {
        Self {
            fields: HashMap::new(),
        }
    }

    /// Returns a reference to the fields map.
    pub fn fields(&self) -> &HashMap<FormPath, FieldState> {
        &self.fields
    }

    /// Returns a mutable reference to the fields map.
    pub fn fields_mut(&mut self) -> &mut HashMap<FormPath, FieldState> {
        &mut self.fields
    }

    /// Returns a reference to the field state at the given path, if it exists.
    pub fn get_field(&self, path: &FormPath) -> Option<&FieldState> {
        self.fields.get(path)
    }

    /// Returns a mutable reference to the field state at the given path.
    pub fn get_field_mut(&mut self, path: &FormPath) -> Option<&mut FieldState> {
        self.fields.get_mut(path)
    }
}

#[derive(Clone, Debug)]
pub struct PendingSubmission {
    pub id: u64,
    pub snapshot: Arc<FormState>,
    pub payload: serde_json::Value,
    pub retries_left: u8,
}

#[derive(Clone, Debug)]
pub enum FormEvent {
    FieldChanged(FormPath),
    ValidationFailed(Vec<ValidationError>),
    SubmissionStarted(u64),
    SubmissionSuccess(u64),
    SubmissionError(u64, String),
    RolledBack(u64),
}

#[derive(Clone, Debug)]
pub struct Form {
    schema: FormSchema,
    state: Arc<FormState>,
    history: History<FormState>,
    pending: Option<PendingSubmission>,
    last_error: Option<String>,
    submit_counter: u64,
}

impl Form {
    pub fn new(schema: FormSchema) -> Self {
        let state = Self::build_initial_state(&schema);
        Self {
            history: History::new(state.clone()),
            state: Arc::new(state),
            schema,
            pending: None,
            last_error: None,
            submit_counter: 0,
        }
    }

    // -----------------------------------------------------------------
    // Accessor methods
    // -----------------------------------------------------------------

    /// Returns a reference to the form schema.
    pub fn schema(&self) -> &FormSchema {
        &self.schema
    }

    /// Returns a reference to the current form state (wrapped in `Arc`).
    pub fn state(&self) -> &FormState {
        &self.state
    }

    /// Returns the `Arc<FormState>` for cheap cloning (e.g. snapshot comparisons).
    pub fn state_arc(&self) -> Arc<FormState> {
        self.state.clone()
    }

    /// Returns a reference to the history tracker.
    pub fn history(&self) -> &History<FormState> {
        &self.history
    }

    /// Returns a mutable reference to the history tracker.
    pub fn history_mut(&mut self) -> &mut History<FormState> {
        &mut self.history
    }

    /// Returns a reference to the pending submission, if any.
    pub fn pending(&self) -> Option<&PendingSubmission> {
        self.pending.as_ref()
    }

    /// Returns the last error message, if any.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    fn build_initial_state(schema: &FormSchema) -> FormState {
        let mut state = FormState::empty();
        for field in &schema.fields {
            Self::insert_default_field(&mut state, FormPath::root(), field);
        }
        state
    }

    fn insert_default_field(state: &mut FormState, path: FormPath, field: &FieldSchema) {
        let field_path = path.push(field.id.clone());
        match &field.field_type {
            FieldType::Text => {
                state
                    .fields
                    .insert(field_path, FieldState::new(FieldValue::Text(String::new())));
            }
            FieldType::Number => {
                state
                    .fields
                    .insert(field_path, FieldState::new(FieldValue::Number(0.0)));
            }
            FieldType::Checkbox => {
                state
                    .fields
                    .insert(field_path, FieldState::new(FieldValue::Bool(false)));
            }
            FieldType::Select { options } => {
                let initial = options.first().cloned().unwrap_or_default();
                state.fields.insert(
                    field_path,
                    FieldState::new(FieldValue::Selection(initial)),
                );
            }
            FieldType::Group { fields, repeatable } => {
                if *repeatable {
                    state.fields.insert(
                        field_path,
                        FieldState::new(FieldValue::GroupList(Vec::new())),
                    );
                } else {
                    let mut group = HashMap::new();
                    for child in fields {
                        let value = Self::default_value_for_schema(child);
                        group.insert(child.id.clone(), value);
                        Self::insert_default_field(state, field_path.clone(), child);
                    }
                    state.fields.insert(
                        field_path,
                        FieldState::new(FieldValue::Group(group)),
                    );
                }
            }
        }
    }

    fn default_value_for_schema(field: &FieldSchema) -> FieldValue {
        match &field.field_type {
            FieldType::Text => FieldValue::Text(String::new()),
            FieldType::Number => FieldValue::Number(0.0),
            FieldType::Checkbox => FieldValue::Bool(false),
            FieldType::Select { options } => {
                let initial = options.first().cloned().unwrap_or_default();
                FieldValue::Selection(initial)
            }
            FieldType::Group { fields, repeatable } => {
                if *repeatable {
                    FieldValue::GroupList(Vec::new())
                } else {
                    let mut map = HashMap::new();
                    for child in fields {
                        map.insert(child.id.clone(), Self::default_value_for_schema(child));
                    }
                    FieldValue::Group(map)
                }
            }
        }
    }

    pub fn set_value(&mut self, path: &FormPath, value: FieldValue) -> FormEvent {
        let mut next = (*self.state).clone();
        if let Some(field) = next.fields.get_mut(path) {
            field.value = value;
            field.touched = true;
            field.dirty = true;
            field.errors.clear();
        }
        Self::update_parent_groups(&mut next, path);
        self.history.push(next.clone());
        self.state = Arc::new(next);
        FormEvent::FieldChanged(path.clone())
    }

    fn update_parent_groups(state: &mut FormState, path: &FormPath) {
        if path.0.len() < 2 {
            return;
        }
        let last = path.0.last().cloned().unwrap_or_default();
        let parent_path = FormPath(path.0[..path.0.len() - 1].to_vec());
        if let (Some(child), Some(parent)) = (
            state.fields.get(path).cloned(),
            state.fields.get_mut(&parent_path),
        ) {
            if let FieldValue::Group(map) = &mut parent.value {
                map.insert(last.clone(), child.value);
            }
        }
        if path.0.len() >= 3 {
            let idx_str = path.0[path.0.len() - 2].clone();
            if let Ok(idx) = idx_str.parse::<usize>() {
                let list_path = FormPath(path.0[..path.0.len() - 2].to_vec());
                if let (Some(child), Some(parent)) = (
                    state.fields.get(path).cloned(),
                    state.fields.get_mut(&list_path),
                ) {
                    if let FieldValue::GroupList(list) = &mut parent.value {
                        if let Some(group) = list.get_mut(idx) {
                            group.insert(last, child.value);
                        }
                    }
                }
            }
        }
    }

    pub fn add_repeat_group(&mut self, path: &FormPath, fields: Vec<FieldSchema>) -> bool {
        let mut next = (*self.state).clone();
        let mut additions: Vec<(FormPath, FieldState)> = Vec::new();
        let mut added = false;
        if let Some(field) = next.fields.get_mut(path) {
            if let FieldValue::GroupList(list) = &mut field.value {
                let mut group = HashMap::new();
                let idx = list.len();
                for child in fields {
                    match child.field_type {
                        FieldType::Text => {
                            group.insert(child.id, FieldValue::Text(String::new()));
                        }
                        FieldType::Number => {
                            group.insert(child.id, FieldValue::Number(0.0));
                        }
                        FieldType::Checkbox => {
                            group.insert(child.id, FieldValue::Bool(false));
                        }
                        FieldType::Select { options } => {
                            let initial = options.first().cloned().unwrap_or_default();
                            group.insert(child.id, FieldValue::Selection(initial));
                        }
                        FieldType::Group { .. } => {
                            group.insert(child.id, FieldValue::Group(HashMap::new()));
                        }
                    }
                }
                let base = path.push(format!("{}", idx));
                for (key, value) in group.iter() {
                    additions.push((
                        base.push(key.clone()),
                        FieldState::new(value.clone()),
                    ));
                }
                list.push(group);
                added = true;
            }
        }
        if !added {
            return false;
        }
        for (child_path, state) in additions {
            next.fields.insert(child_path, state);
        }
        self.history.push(next.clone());
        self.state = Arc::new(next);
        true
    }

    pub fn validate(&mut self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        for field in &self.schema.fields {
            self.collect_errors(FormPath::root(), field, &mut errors);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            let mut next = (*self.state).clone();
            for err in &errors {
                if let Some(field) = next.fields.get_mut(&err.path) {
                    field.errors.push(err.message.clone());
                }
            }
            self.state = Arc::new(next);
            Err(errors)
        }
    }

    fn collect_errors(
        &self,
        path: FormPath,
        field: &FieldSchema,
        out: &mut Vec<ValidationError>,
    ) {
        let field_path = path.push(field.id.clone());
        match &field.field_type {
            FieldType::Group { fields, repeatable } => {
                if *repeatable {
                    if let Some(state) = self.state.fields.get(&field_path) {
                        if let FieldValue::GroupList(list) = &state.value {
                            for (idx, group) in list.iter().enumerate() {
                                for child in fields {
                                    let child_path = field_path.push(format!("{}", idx)).push(child.id.clone());
                                    if let Some(value) = group.get(&child.id) {
                                        out.extend(validate_value(&child_path, value, &child.rules));
                                    }
                                }
                            }
                        }
                    }
                } else if let Some(state) = self.state.fields.get(&field_path) {
                    if let FieldValue::Group(group) = &state.value {
                        for child in fields {
                            if let Some(value) = group.get(&child.id) {
                                let child_path = field_path.push(child.id.clone());
                                out.extend(validate_value(&child_path, value, &child.rules));
                            }
                        }
                    }
                }
            }
            _ => {
                if let Some(state) = self.state.fields.get(&field_path) {
                    out.extend(validate_value(&field_path, &state.value, &field.rules));
                }
            }
        }
    }

    pub fn start_submit(&mut self, payload: serde_json::Value, retries: u8) -> Result<FormEvent, FormEvent> {
        if let Err(errors) = self.validate() {
            return Err(FormEvent::ValidationFailed(errors));
        }

        let snapshot = self.state.clone();
        self.submit_counter += 1;
        let id = self.submit_counter;
        self.pending = Some(PendingSubmission {
            id,
            snapshot,
            payload,
            retries_left: retries,
        });
        if let Some(pending) = &self.pending {
            let mut next = (*self.state).clone();
            for (_path, field) in next.fields.iter_mut() {
                field.pending = true;
            }
            self.state = Arc::new(next);
            return Ok(FormEvent::SubmissionStarted(pending.id));
        }
        Err(FormEvent::SubmissionError(id, "failed to start".to_string()))
    }

    pub fn apply_success(&mut self, id: u64) -> FormEvent {
        if let Some(pending) = &self.pending {
            if pending.id == id {
                let mut next = (*self.state).clone();
                for (_path, field) in next.fields.iter_mut() {
                    field.pending = false;
                    field.dirty = false;
                }
                self.state = Arc::new(next);
                self.pending = None;
                return FormEvent::SubmissionSuccess(id);
            }
        }
        FormEvent::SubmissionError(id, "unknown submission".to_string())
    }

    pub fn apply_error(&mut self, id: u64, message: &str, rollback: bool) -> FormEvent {
        if let Some(pending) = &self.pending {
            if pending.id == id {
                self.last_error = Some(message.to_string());
                if rollback {
                    self.state = pending.snapshot.clone();
                    self.pending = None;
                    return FormEvent::RolledBack(id);
                }
                let mut next = (*self.state).clone();
                for (_path, field) in next.fields.iter_mut() {
                    field.pending = false;
                }
                self.state = Arc::new(next);
                self.pending = None;
                return FormEvent::SubmissionError(id, message.to_string());
            }
        }
        FormEvent::SubmissionError(id, "unknown submission".to_string())
    }

    pub fn retry_pending(&mut self) -> Option<u64> {
        if let Some(pending) = &mut self.pending {
            if pending.retries_left > 0 {
                pending.retries_left -= 1;
                return Some(pending.id);
            }
        }
        None
    }

    pub fn timeout_pending(&mut self) -> FormEvent {
        if let Some(pending) = &self.pending {
            return self.apply_error(pending.id, "timeout", true);
        }
        FormEvent::SubmissionError(0, "no pending submission".to_string())
    }

    pub fn set_field_error(&mut self, path: &FormPath, message: &str) {
        let mut next = (*self.state).clone();
        if let Some(field) = next.fields.get_mut(path) {
            field.errors.push(message.to_string());
        }
        self.state = Arc::new(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::ValidationRule;

    fn text_field(id: &str) -> FieldSchema {
        FieldSchema {
            id: id.to_string(),
            label: id.to_string(),
            field_type: FieldType::Text,
            rules: vec![],
        }
    }

    fn simple_schema() -> FormSchema {
        FormSchema {
            fields: vec![text_field("name"), text_field("email")],
        }
    }

    #[test]
    fn set_value_creates_history_snapshot() {
        let mut form = Form::new(simple_schema());
        assert!(!form.history.can_undo());

        let path = FormPath::root().push("name");
        form.set_value(&path, FieldValue::Text("Alice".into()));
        assert!(form.history.can_undo());

        // Current state reflects the change
        let field = form.state.fields.get(&path).unwrap();
        if let FieldValue::Text(ref v) = field.value {
            assert_eq!(v, "Alice");
        } else {
            panic!("expected Text variant");
        }
    }

    #[test]
    fn history_undo_restores_previous_state() {
        let mut form = Form::new(simple_schema());
        let path = FormPath::root().push("name");

        form.set_value(&path, FieldValue::Text("Alice".into()));
        form.set_value(&path, FieldValue::Text("Bob".into()));

        // Undo should restore "Alice"
        let prev = form.history.undo().unwrap();
        let field = prev.fields.get(&path).unwrap();
        if let FieldValue::Text(ref v) = field.value {
            assert_eq!(v, "Alice");
        } else {
            panic!("expected Text variant");
        }
    }

    #[test]
    fn clone_independence() {
        // Verify that cloned FormState is fully independent (no shared mutable state).
        let mut form = Form::new(simple_schema());
        let path = FormPath::root().push("name");

        form.set_value(&path, FieldValue::Text("original".into()));
        let snapshot = form.state.clone();

        form.set_value(&path, FieldValue::Text("modified".into()));

        // Snapshot should still have the original value
        let snap_field = snapshot.fields.get(&path).unwrap();
        if let FieldValue::Text(ref v) = snap_field.value {
            assert_eq!(v, "original");
        } else {
            panic!("expected Text variant");
        }

        // Current state should have the modified value
        let cur_field = form.state.fields.get(&path).unwrap();
        if let FieldValue::Text(ref v) = cur_field.value {
            assert_eq!(v, "modified");
        } else {
            panic!("expected Text variant");
        }
    }

    #[test]
    fn submission_rollback_restores_snapshot() {
        let schema = FormSchema {
            fields: vec![FieldSchema {
                id: "name".into(),
                label: "Name".into(),
                field_type: FieldType::Text,
                rules: vec![ValidationRule::Required],
            }],
        };
        let mut form = Form::new(schema);
        let path = FormPath::root().push("name");

        form.set_value(&path, FieldValue::Text("before-submit".into()));
        let _pre_submit = form.state.clone();

        let payload = serde_json::json!({"name": "before-submit"});
        form.start_submit(payload, 1).unwrap();

        // Mutate state after submission started
        form.set_value(&path, FieldValue::Text("after-submit".into()));

        // Rollback should restore to pre-submit snapshot
        if let Some(pending) = &form.pending {
            let snap = pending.snapshot.clone();
            let snap_field = snap.fields.get(&path).unwrap();
            if let FieldValue::Text(ref v) = snap_field.value {
                assert_eq!(v, "before-submit");
            } else {
                panic!("expected Text variant");
            }
        }
    }

    #[test]
    fn set_field_error_does_not_push_to_history() {
        let mut form = Form::new(simple_schema());
        let path = FormPath::root().push("name");

        form.set_value(&path, FieldValue::Text("test".into()));
        assert!(form.history.can_undo());

        // set_field_error updates state but should not add a history entry
        let undo_count_before = {
            let mut count = 0u32;
            let mut h = form.history.clone();
            while h.can_undo() {
                h.undo();
                count += 1;
            }
            count
        };

        form.set_field_error(&path, "some error");

        let undo_count_after = {
            let mut count = 0u32;
            let mut h = form.history.clone();
            while h.can_undo() {
                h.undo();
                count += 1;
            }
            count
        };

        // History depth should be unchanged after set_field_error
        assert_eq!(undo_count_before, undo_count_after);
    }

    // -----------------------------------------------------------------------
    // FormPath
    // -----------------------------------------------------------------------

    #[test]
    fn form_path_root_as_string() {
        assert_eq!(FormPath::root().as_string(), "root");
    }

    #[test]
    fn form_path_push_as_string() {
        let path = FormPath::root().push("a").push("b");
        assert_eq!(path.as_string(), "a.b");
    }

    #[test]
    fn form_path_equality() {
        let a = FormPath::root().push("x").push("y");
        let b = FormPath::root().push("x").push("y");
        assert_eq!(a, b);
    }

    #[test]
    fn form_path_inequality() {
        let a = FormPath::root().push("x");
        let b = FormPath::root().push("y");
        assert_ne!(a, b);
    }

    // -----------------------------------------------------------------------
    // set_value / get_value roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn set_value_roundtrip_text() {
        let mut form = Form::new(simple_schema());
        let path = FormPath::root().push("name");
        form.set_value(&path, FieldValue::Text("Alice".into()));
        let field = form.state.fields.get(&path).unwrap();
        match &field.value {
            FieldValue::Text(v) => assert_eq!(v, "Alice"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn set_value_marks_dirty_and_touched() {
        let mut form = Form::new(simple_schema());
        let path = FormPath::root().push("name");
        form.set_value(&path, FieldValue::Text("test".into()));
        let field = form.state.fields.get(&path).unwrap();
        assert!(field.dirty);
        assert!(field.touched);
    }

    #[test]
    fn set_value_clears_previous_errors() {
        let mut form = Form::new(simple_schema());
        let path = FormPath::root().push("name");
        form.set_field_error(&path, "bad");
        form.set_value(&path, FieldValue::Text("fixed".into()));
        let field = form.state.fields.get(&path).unwrap();
        assert!(field.errors.is_empty());
    }

    #[test]
    fn set_value_returns_field_changed_event() {
        let mut form = Form::new(simple_schema());
        let path = FormPath::root().push("name");
        let event = form.set_value(&path, FieldValue::Text("x".into()));
        match event {
            FormEvent::FieldChanged(p) => assert_eq!(p, path),
            _ => panic!("expected FieldChanged"),
        }
    }

    // -----------------------------------------------------------------------
    // Validation
    // -----------------------------------------------------------------------

    #[test]
    fn validation_required_empty_text_fails() {
        let schema = FormSchema {
            fields: vec![FieldSchema {
                id: "name".into(),
                label: "Name".into(),
                field_type: FieldType::Text,
                rules: vec![ValidationRule::Required],
            }],
        };
        let mut form = Form::new(schema);
        let result = form.validate();
        assert!(result.is_err());
    }

    #[test]
    fn validation_required_nonempty_text_passes() {
        let schema = FormSchema {
            fields: vec![FieldSchema {
                id: "name".into(),
                label: "Name".into(),
                field_type: FieldType::Text,
                rules: vec![ValidationRule::Required],
            }],
        };
        let mut form = Form::new(schema);
        let path = FormPath::root().push("name");
        form.set_value(&path, FieldValue::Text("Alice".into()));
        let result = form.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn validation_errors_stored_on_field() {
        let schema = FormSchema {
            fields: vec![FieldSchema {
                id: "name".into(),
                label: "Name".into(),
                field_type: FieldType::Text,
                rules: vec![ValidationRule::Required],
            }],
        };
        let mut form = Form::new(schema);
        let _ = form.validate();
        let path = FormPath::root().push("name");
        let field = form.state.fields.get(&path).unwrap();
        assert!(!field.errors.is_empty());
    }

    // -----------------------------------------------------------------------
    // Field errors
    // -----------------------------------------------------------------------

    #[test]
    fn set_field_error_adds_error() {
        let mut form = Form::new(simple_schema());
        let path = FormPath::root().push("email");
        form.set_field_error(&path, "invalid email");
        let field = form.state.fields.get(&path).unwrap();
        assert_eq!(field.errors.len(), 1);
        assert_eq!(field.errors[0], "invalid email");
    }

    #[test]
    fn set_field_error_accumulates() {
        let mut form = Form::new(simple_schema());
        let path = FormPath::root().push("email");
        form.set_field_error(&path, "error 1");
        form.set_field_error(&path, "error 2");
        let field = form.state.fields.get(&path).unwrap();
        assert_eq!(field.errors.len(), 2);
    }

    // -----------------------------------------------------------------------
    // Submission
    // -----------------------------------------------------------------------

    #[test]
    fn start_submit_fails_validation() {
        let schema = FormSchema {
            fields: vec![FieldSchema {
                id: "name".into(),
                label: "Name".into(),
                field_type: FieldType::Text,
                rules: vec![ValidationRule::Required],
            }],
        };
        let mut form = Form::new(schema);
        let result = form.start_submit(serde_json::json!({}), 0);
        assert!(result.is_err());
    }

    #[test]
    fn start_submit_sets_pending_flag() {
        let mut form = Form::new(simple_schema());
        let path = FormPath::root().push("name");
        form.set_value(&path, FieldValue::Text("test".into()));
        let result = form.start_submit(serde_json::json!({}), 0);
        assert!(result.is_ok());
        // All fields should be marked pending
        for (_p, field) in form.state.fields.iter() {
            assert!(field.pending);
        }
    }

    #[test]
    fn apply_success_clears_pending_and_dirty() {
        let mut form = Form::new(simple_schema());
        let path = FormPath::root().push("name");
        form.set_value(&path, FieldValue::Text("test".into()));
        let id = match form.start_submit(serde_json::json!({}), 0).unwrap() {
            FormEvent::SubmissionStarted(id) => id,
            _ => panic!("expected SubmissionStarted"),
        };
        form.apply_success(id);
        assert!(form.pending.is_none());
        for (_p, field) in form.state.fields.iter() {
            assert!(!field.pending);
            assert!(!field.dirty);
        }
    }

    #[test]
    fn apply_error_with_rollback_restores_snapshot() {
        let mut form = Form::new(simple_schema());
        let path = FormPath::root().push("name");
        form.set_value(&path, FieldValue::Text("before".into()));
        let id = match form.start_submit(serde_json::json!({}), 0).unwrap() {
            FormEvent::SubmissionStarted(id) => id,
            _ => panic!("expected SubmissionStarted"),
        };
        form.set_value(&path, FieldValue::Text("after".into()));
        let event = form.apply_error(id, "fail", true);
        match event {
            FormEvent::RolledBack(_) => {}
            _ => panic!("expected RolledBack"),
        }
        // State should be rolled back to the pre-submit snapshot
        let field = form.state.fields.get(&path).unwrap();
        match &field.value {
            FieldValue::Text(v) => assert_eq!(v, "before"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn retry_pending_decrements_retries() {
        let mut form = Form::new(simple_schema());
        let path = FormPath::root().push("name");
        form.set_value(&path, FieldValue::Text("test".into()));
        form.start_submit(serde_json::json!({}), 3).unwrap();
        assert!(form.retry_pending().is_some());
        assert!(form.retry_pending().is_some());
        assert!(form.retry_pending().is_some());
        assert!(form.retry_pending().is_none()); // exhausted
    }

    // -----------------------------------------------------------------------
    // Form initial state
    // -----------------------------------------------------------------------

    #[test]
    fn new_form_creates_default_fields() {
        let schema = FormSchema {
            fields: vec![
                FieldSchema {
                    id: "name".into(),
                    label: "Name".into(),
                    field_type: FieldType::Text,
                    rules: vec![],
                },
                FieldSchema {
                    id: "age".into(),
                    label: "Age".into(),
                    field_type: FieldType::Number,
                    rules: vec![],
                },
                FieldSchema {
                    id: "agree".into(),
                    label: "Agree".into(),
                    field_type: FieldType::Checkbox,
                    rules: vec![],
                },
            ],
        };
        let form = Form::new(schema);
        assert_eq!(form.state.fields.len(), 3);
        let name_path = FormPath::root().push("name");
        match &form.state.fields.get(&name_path).unwrap().value {
            FieldValue::Text(v) => assert_eq!(v, ""),
            _ => panic!("expected Text"),
        }
        let age_path = FormPath::root().push("age");
        match &form.state.fields.get(&age_path).unwrap().value {
            FieldValue::Number(v) => assert_eq!(*v, 0.0),
            _ => panic!("expected Number"),
        }
    }

    #[test]
    fn new_form_no_history() {
        let form = Form::new(simple_schema());
        assert!(!form.history.can_undo());
        assert!(!form.history.can_redo());
    }

    // -----------------------------------------------------------------------
    // History integration
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_set_values_create_history() {
        let mut form = Form::new(simple_schema());
        let path = FormPath::root().push("name");
        form.set_value(&path, FieldValue::Text("a".into()));
        form.set_value(&path, FieldValue::Text("b".into()));
        form.set_value(&path, FieldValue::Text("c".into()));
        // Should be able to undo 3 times
        assert!(form.history.can_undo());
        let prev = form.history.undo().unwrap();
        match &prev.fields.get(&path).unwrap().value {
            FieldValue::Text(v) => assert_eq!(v, "b"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn history_redo_after_undo() {
        let mut form = Form::new(simple_schema());
        let path = FormPath::root().push("name");
        form.set_value(&path, FieldValue::Text("first".into()));
        form.set_value(&path, FieldValue::Text("second".into()));
        form.history.undo(); // back to "first"
        assert!(form.history.can_redo());
        let next = form.history.redo().unwrap();
        match &next.fields.get(&path).unwrap().value {
            FieldValue::Text(v) => assert_eq!(v, "second"),
            _ => panic!("expected Text"),
        }
    }
}
