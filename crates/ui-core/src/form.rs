use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::state::History;
use crate::validation::{validate_value, ValidationError, ValidationRule};

/// A field identifier — an owned `String` used as a key in form paths and schemas.
pub type FieldId = String;

/// A dot-separated path identifying a specific field within a (possibly nested) form.
///
/// Use [`FormPath::root`] as a starting point and [`FormPath::push`] to descend
/// into nested groups.
///
/// # Example
///
/// ```
/// use ui_core::form::FormPath;
///
/// let path = FormPath::root().push("address").push("city");
/// assert_eq!(path.as_string(), "address.city");
/// ```
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
    /// Returns the root (empty) path, representing the top level of the form.
    pub fn root() -> Self {
        FormPath(Vec::new())
    }

    /// Returns a new path with `id` appended as the next segment.
    pub fn push(&self, id: impl Into<FieldId>) -> Self {
        let mut next = self.0.clone();
        next.push(id.into());
        FormPath(next)
    }

    /// Returns the path as a human-readable dot-separated string (e.g. `"address.city"`).
    /// Returns `"root"` for the empty path.
    pub fn as_string(&self) -> String {
        if self.0.is_empty() {
            "root".to_string()
        } else {
            self.0.join(".")
        }
    }
}

/// The runtime value stored in a form field.
///
/// Each variant corresponds to a [`FieldType`] in the schema.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FieldValue {
    /// Free-form text value.
    Text(String),
    /// Numeric value.
    Number(f64),
    /// Boolean (checkbox) value.
    Bool(bool),
    /// Currently selected option string for a `Select` field.
    Selection(String),
    /// Values for a non-repeatable nested group, keyed by child field id.
    Group(HashMap<FieldId, FieldValue>),
    /// Values for a repeatable nested group — a list of row maps.
    GroupList(Vec<HashMap<FieldId, FieldValue>>),
}

/// Runtime state of a single form field.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FieldState {
    /// The current value.
    pub value: FieldValue,
    /// Validation error messages (populated by [`Form::validate`]).
    pub errors: Vec<String>,
    /// `true` after the user has interacted with this field at least once.
    pub touched: bool,
    /// `true` while a submission is in-flight (optimistic UI).
    pub pending: bool,
    /// `true` if the field is disabled (read-only in the UI).
    pub disabled: bool,
    /// `true` if the value has changed since the last successful submission.
    pub dirty: bool,
}

impl FieldState {
    /// Creates a new field state with the given initial value and all flags set to `false`.
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

/// Hint for the browser's autofill / password-manager machinery.
///
/// Maps to the HTML `autocomplete` attribute values that password managers and
/// browser autofill use to identify credential fields.  Only fields that carry
/// one of these hints will get a corresponding hidden DOM `<input>` in the
/// `AutofillBridge` on the JS side.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutocompleteHint {
    /// `autocomplete="username"` — login username or user identifier.
    Username,
    /// `autocomplete="email"` — email address used as a login identifier or
    /// contact address.
    Email,
    /// `autocomplete="current-password"` — password for the current account
    /// (login form).
    CurrentPassword,
    /// `autocomplete="new-password"` — new password chosen by the user
    /// (registration / change-password form).
    NewPassword,
    /// `autocomplete="name"` — full name.
    Name,
    /// `autocomplete="given-name"` — given (first) name.
    GivenName,
    /// `autocomplete="family-name"` — family (last) name.
    FamilyName,
    /// An explicit, arbitrary `autocomplete` token.  Use this for tokens not
    /// covered by the enum variants above.
    Custom(String),
}

impl AutocompleteHint {
    /// Returns the `autocomplete` attribute string for this hint.
    pub fn as_str(&self) -> &str {
        match self {
            AutocompleteHint::Username => "username",
            AutocompleteHint::Email => "email",
            AutocompleteHint::CurrentPassword => "current-password",
            AutocompleteHint::NewPassword => "new-password",
            AutocompleteHint::Name => "name",
            AutocompleteHint::GivenName => "given-name",
            AutocompleteHint::FamilyName => "family-name",
            AutocompleteHint::Custom(s) => s.as_str(),
        }
    }

    /// Returns the HTML `<input type>` most appropriate for this hint.
    ///
    /// Password hints → `"password"`, email hint → `"email"`, everything else
    /// → `"text"`.
    pub fn input_type(&self) -> &'static str {
        match self {
            AutocompleteHint::CurrentPassword | AutocompleteHint::NewPassword => "password",
            AutocompleteHint::Email => "email",
            _ => "text",
        }
    }
}

/// Declares the semantic type of a form field.
///
/// Determines which widget is rendered and what [`FieldValue`] variant is stored.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FieldType {
    /// Single-line or multiline text input. Runtime value: [`FieldValue::Text`].
    Text,
    /// Numeric input. Runtime value: [`FieldValue::Number`].
    Number,
    /// Boolean checkbox. Runtime value: [`FieldValue::Bool`].
    Checkbox,
    /// Dropdown / select with a fixed list of options. Runtime value: [`FieldValue::Selection`].
    Select { options: Vec<String> },
    /// Nested group of sub-fields. If `repeatable` is `true`, the field holds a
    /// list of rows (each row is one instance of the sub-schema).
    Group { fields: Vec<FieldSchema>, repeatable: bool },
}

/// Schema declaration for a single form field.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FieldSchema {
    /// Unique identifier for this field within its parent group.
    pub id: FieldId,
    /// Human-readable label shown next to the widget.
    pub label: String,
    /// The semantic type, determining the widget and value kind.
    pub field_type: FieldType,
    /// Validation rules evaluated when [`Form::validate`] is called.
    pub rules: Vec<ValidationRule>,
    /// Optional placeholder text shown when the field is empty.
    pub placeholder: Option<String>,
    /// Autofill / password-manager hint.  When `Some`, a hidden DOM `<input>`
    /// with the corresponding `autocomplete` attribute will be created by the
    /// `AutofillBridge` in `app.js`.
    pub autocomplete: Option<AutocompleteHint>,
}

/// Declares the complete schema for a form.
///
/// Build a schema using the builder methods, then pass it to [`Form::new`].
///
/// # Example
///
/// ```
/// use ui_core::form::{FormSchema, FieldType};
/// use ui_core::validation::ValidationRule;
///
/// let schema = FormSchema::new("contact")
///     .field("name", FieldType::Text)
///     .with_label("name", "Full name")
///     .required("name")
///     .field("email", FieldType::Text)
///     .with_label("email", "Email")
///     .required("email")
///     .with_validation("email", ValidationRule::Email);
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FormSchema {
    /// Unique name for this form (used for logging and serialization).
    pub name: String,
    /// Top-level field declarations.
    pub fields: Vec<FieldSchema>,
}

impl FormSchema {
    /// Create a new empty form schema with the given name.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            fields: Vec::new(),
        }
    }

    /// Add a field with the given name and type, using default settings.
    pub fn field(mut self, name: &str, field_type: FieldType) -> Self {
        self.fields.push(FieldSchema {
            id: name.to_string(),
            label: name.to_string(),
            field_type,
            rules: Vec::new(),
            placeholder: None,
            autocomplete: None,
        });
        self
    }

    /// Set the autofill / password-manager hint for the named field.
    ///
    /// When set, the JS `AutofillBridge` will create a hidden `<input>` with
    /// the matching `autocomplete` attribute so that browser autofill and
    /// third-party password managers can discover and fill the field.
    ///
    /// # Example
    /// ```
    /// use ui_core::form::{FormSchema, FieldType, AutocompleteHint};
    /// let schema = FormSchema::new("login")
    ///     .field("email", FieldType::Text)
    ///     .with_autocomplete("email", AutocompleteHint::Email)
    ///     .field("password", FieldType::Text)
    ///     .with_autocomplete("password", AutocompleteHint::CurrentPassword);
    /// ```
    pub fn with_autocomplete(mut self, name: &str, hint: AutocompleteHint) -> Self {
        if let Some(field) = self.fields.iter_mut().find(|f| f.id == name) {
            field.autocomplete = Some(hint);
        }
        self
    }

    /// Mark the named field as required by adding `ValidationRule::Required`.
    pub fn required(mut self, name: &str) -> Self {
        if let Some(field) = self.fields.iter_mut().find(|f| f.id == name) {
            if !field.rules.iter().any(|r| matches!(r, ValidationRule::Required)) {
                field.rules.push(ValidationRule::Required);
            }
        }
        self
    }

    /// Set the display label for the named field.
    pub fn with_label(mut self, name: &str, label: &str) -> Self {
        if let Some(field) = self.fields.iter_mut().find(|f| f.id == name) {
            field.label = label.to_string();
        }
        self
    }

    /// Set the placeholder text for the named field.
    pub fn with_placeholder(mut self, name: &str, placeholder: &str) -> Self {
        if let Some(field) = self.fields.iter_mut().find(|f| f.id == name) {
            field.placeholder = Some(placeholder.to_string());
        }
        self
    }

    /// Add a validation rule to the named field.
    pub fn with_validation(mut self, name: &str, rule: ValidationRule) -> Self {
        if let Some(field) = self.fields.iter_mut().find(|f| f.id == name) {
            field.rules.push(rule);
        }
        self
    }

    /// Add a nested field group built by the provided closure.
    pub fn group(mut self, name: &str, builder: impl FnOnce(FormSchema) -> FormSchema) -> Self {
        let inner = builder(FormSchema::new(name));
        self.fields.push(FieldSchema {
            id: name.to_string(),
            label: name.to_string(),
            field_type: FieldType::Group {
                fields: inner.fields,
                repeatable: false,
            },
            rules: Vec::new(),
            placeholder: None,
            autocomplete: None,
        });
        self
    }

    /// Add a repeatable (list) field group built by the provided closure.
    pub fn repeatable_group(mut self, name: &str, builder: impl FnOnce(FormSchema) -> FormSchema) -> Self {
        let inner = builder(FormSchema::new(name));
        self.fields.push(FieldSchema {
            id: name.to_string(),
            label: name.to_string(),
            field_type: FieldType::Group {
                fields: inner.fields,
                repeatable: true,
            },
            rules: Vec::new(),
            placeholder: None,
            autocomplete: None,
        });
        self
    }
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
    /// Creates an empty form state with no fields.
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

/// An in-flight form submission awaiting a server response.
#[derive(Clone, Debug)]
pub struct PendingSubmission {
    /// Monotonically increasing ID for this submission.
    pub id: u64,
    /// Snapshot of the form state at submission time, used for optimistic rollback.
    pub snapshot: Arc<FormState>,
    /// The serialized payload that was sent to the server.
    pub payload: serde_json::Value,
    /// How many automatic retries remain before the submission is abandoned.
    pub retries_left: u8,
}

/// Events emitted by the form model.
///
/// Call [`Form::set_value`], [`Form::validate`], [`Form::start_submit`], etc. and
/// inspect the returned `FormEvent` to drive your application's side-effects (e.g.
/// showing a spinner, displaying errors, navigating on success).
#[derive(Clone, Debug)]
pub enum FormEvent {
    /// A field value changed at the given path.
    FieldChanged(FormPath),
    /// Validation failed; the vec contains one error per failing field.
    ValidationFailed(Vec<ValidationError>),
    /// A submission started successfully with the given id.
    SubmissionStarted(u64),
    /// The server confirmed the submission with the given id succeeded.
    SubmissionSuccess(u64),
    /// The server reported an error for the submission with the given id.
    SubmissionError(u64, String),
    /// The submission was rolled back (state restored to the pre-submit snapshot).
    RolledBack(u64),
}

/// The live form model: schema, current field state, submission lifecycle, and history.
///
/// Create a `Form` with [`Form::new`], update it via [`Form::set_value`], and drive
/// submissions with [`Form::start_submit`] / [`Form::apply_success`] /
/// [`Form::apply_error`].
///
/// # Example
///
/// ```
/// use ui_core::form::{Form, FormSchema, FieldType, FieldValue, FormPath};
///
/// let schema = FormSchema::new("example").field("name", FieldType::Text).required("name");
/// let mut form = Form::new(schema);
///
/// let path = FormPath::root().push("name");
/// form.set_value(&path, FieldValue::Text("Alice".into()));
///
/// assert!(form.validate().is_ok());
/// ```
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
    /// Creates a new form from the given schema, initializing all fields to their default values.
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

    /// Sets the value of the field at `path`, marks it as touched and dirty, and
    /// clears any existing validation errors for that field.
    ///
    /// Returns [`FormEvent::FieldChanged`] with the path that changed.
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

    /// Appends a new empty row to the repeatable group at `path`.
    ///
    /// `fields` must match the sub-schema declared for the group. Returns `true`
    /// if the row was added, `false` if `path` does not point to a `GroupList`.
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

    /// Runs all validation rules and returns `Ok(())` if the form is valid.
    ///
    /// On failure, returns `Err(errors)` and writes the error messages into the
    /// affected [`FieldState::errors`] fields so widgets can display them.
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

    /// Validates the form and, if valid, starts an optimistic submission.
    ///
    /// All fields are marked `pending = true` immediately. Returns
    /// `Ok(FormEvent::SubmissionStarted(id))` on success, or
    /// `Err(FormEvent::ValidationFailed(errors))` if validation fails.
    ///
    /// After the server responds, call [`apply_success`](Self::apply_success) or
    /// [`apply_error`](Self::apply_error) with the returned `id`.
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

    /// Marks the pending submission with `id` as succeeded.
    ///
    /// Clears all `pending` and `dirty` flags, and removes the pending submission.
    /// Returns [`FormEvent::SubmissionSuccess`].
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

    /// Marks the pending submission with `id` as failed.
    ///
    /// If `rollback` is `true`, the form state is restored to the pre-submit snapshot
    /// and [`FormEvent::RolledBack`] is returned. Otherwise the current state is kept
    /// with `pending` cleared and [`FormEvent::SubmissionError`] is returned.
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

    /// Attempts to retry the pending submission, decrementing `retries_left`.
    ///
    /// Returns `Some(id)` if a retry is possible, or `None` if there are no
    /// retries remaining or no submission is pending.
    pub fn retry_pending(&mut self) -> Option<u64> {
        if let Some(pending) = &mut self.pending {
            if pending.retries_left > 0 {
                pending.retries_left -= 1;
                return Some(pending.id);
            }
        }
        None
    }

    /// Times out the current pending submission, rolling back state.
    ///
    /// Equivalent to `apply_error(id, "timeout", true)`.
    pub fn timeout_pending(&mut self) -> FormEvent {
        if let Some(pending) = &self.pending {
            return self.apply_error(pending.id, "timeout", true);
        }
        FormEvent::SubmissionError(0, "no pending submission".to_string())
    }

    /// Appends a server-side error message to the field at `path`.
    ///
    /// Use this to display errors returned by the server after a failed submission.
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
            placeholder: None,
            autocomplete: None,
        }
    }

    fn simple_schema() -> FormSchema {
        FormSchema {
            name: "test".to_string(),
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
            name: "test".to_string(),
            fields: vec![FieldSchema {
                id: "name".into(),
                label: "Name".into(),
                field_type: FieldType::Text,
                rules: vec![ValidationRule::Required],
                placeholder: None,
                autocomplete: None,
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
            name: "test".to_string(),
            fields: vec![FieldSchema {
                id: "name".into(),
                label: "Name".into(),
                field_type: FieldType::Text,
                rules: vec![ValidationRule::Required],
                placeholder: None,
                autocomplete: None,
            }],
        };
        let mut form = Form::new(schema);
        let result = form.validate();
        assert!(result.is_err());
    }

    #[test]
    fn validation_required_nonempty_text_passes() {
        let schema = FormSchema {
            name: "test".to_string(),
            fields: vec![FieldSchema {
                id: "name".into(),
                label: "Name".into(),
                field_type: FieldType::Text,
                rules: vec![ValidationRule::Required],
                placeholder: None,
                autocomplete: None,
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
            name: "test".to_string(),
            fields: vec![FieldSchema {
                id: "name".into(),
                label: "Name".into(),
                field_type: FieldType::Text,
                rules: vec![ValidationRule::Required],
                placeholder: None,
                autocomplete: None,
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
            name: "test".to_string(),
            fields: vec![FieldSchema {
                id: "name".into(),
                label: "Name".into(),
                field_type: FieldType::Text,
                rules: vec![ValidationRule::Required],
                placeholder: None,
                autocomplete: None,
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
            name: "test".to_string(),
            fields: vec![
                FieldSchema {
                    id: "name".into(),
                    label: "Name".into(),
                    field_type: FieldType::Text,
                    rules: vec![],
                    placeholder: None,
                    autocomplete: None,
                },
                FieldSchema {
                    id: "age".into(),
                    label: "Age".into(),
                    field_type: FieldType::Number,
                    rules: vec![],
                    placeholder: None,
                    autocomplete: None,
                },
                FieldSchema {
                    id: "agree".into(),
                    label: "Agree".into(),
                    field_type: FieldType::Checkbox,
                    rules: vec![],
                    placeholder: None,
                    autocomplete: None,
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

    // -----------------------------------------------------------------------
    // Builder API
    // -----------------------------------------------------------------------

    #[test]
    fn builder_produces_correct_schema() {
        let schema = FormSchema::new("login")
            .field("email", FieldType::Text)
            .field("password", FieldType::Text);

        assert_eq!(schema.name, "login");
        assert_eq!(schema.fields.len(), 2);
        assert_eq!(schema.fields[0].id, "email");
        assert_eq!(schema.fields[1].id, "password");
    }

    #[test]
    fn builder_required_adds_validation_rule() {
        let schema = FormSchema::new("test")
            .field("email", FieldType::Text)
            .required("email");

        assert_eq!(schema.fields[0].rules.len(), 1);
        assert!(matches!(schema.fields[0].rules[0], ValidationRule::Required));
    }

    #[test]
    fn builder_required_is_idempotent() {
        let schema = FormSchema::new("test")
            .field("email", FieldType::Text)
            .required("email")
            .required("email");

        assert_eq!(schema.fields[0].rules.len(), 1);
    }

    #[test]
    fn builder_required_fields_are_validated() {
        let schema = FormSchema::new("test")
            .field("email", FieldType::Text)
            .required("email");

        let mut form = Form::new(schema);
        let result = form.validate();
        assert!(result.is_err());

        let path = FormPath::root().push("email");
        form.set_value(&path, FieldValue::Text("test@example.com".into()));
        let result = form.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn builder_with_label_sets_label() {
        let schema = FormSchema::new("test")
            .field("email", FieldType::Text)
            .with_label("email", "Email Address");

        assert_eq!(schema.fields[0].label, "Email Address");
    }

    #[test]
    fn builder_with_placeholder_sets_placeholder() {
        let schema = FormSchema::new("test")
            .field("email", FieldType::Text)
            .with_placeholder("email", "you@example.com");

        assert_eq!(
            schema.fields[0].placeholder.as_deref(),
            Some("you@example.com")
        );
    }

    #[test]
    fn builder_with_validation_adds_rule() {
        let schema = FormSchema::new("test")
            .field("email", FieldType::Text)
            .with_validation("email", ValidationRule::Required)
            .with_validation("email", ValidationRule::Email);

        assert_eq!(schema.fields[0].rules.len(), 2);
        assert!(matches!(schema.fields[0].rules[0], ValidationRule::Required));
        assert!(matches!(schema.fields[0].rules[1], ValidationRule::Email));
    }

    #[test]
    fn builder_multiple_validation_rules_on_one_field() {
        let schema = FormSchema::new("test")
            .field("username", FieldType::Text)
            .required("username")
            .with_validation(
                "username",
                ValidationRule::Regex {
                    pattern: "^[a-z]+$".into(),
                },
            );

        assert_eq!(schema.fields[0].rules.len(), 2);
    }

    #[test]
    fn builder_nested_group() {
        let schema = FormSchema::new("test")
            .group("profile", |s| {
                s.field("name", FieldType::Text)
                    .required("name")
                    .field("email", FieldType::Text)
            });

        assert_eq!(schema.fields.len(), 1);
        assert_eq!(schema.fields[0].id, "profile");
        match &schema.fields[0].field_type {
            FieldType::Group { fields, repeatable } => {
                assert!(!repeatable);
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].id, "name");
                assert!(matches!(fields[0].rules[0], ValidationRule::Required));
                assert_eq!(fields[1].id, "email");
            }
            _ => panic!("expected Group"),
        }
    }

    #[test]
    fn builder_repeatable_group() {
        let schema = FormSchema::new("test")
            .repeatable_group("contacts", |s| {
                s.field("label", FieldType::Text)
                    .required("label")
                    .field("value", FieldType::Text)
            });

        assert_eq!(schema.fields.len(), 1);
        match &schema.fields[0].field_type {
            FieldType::Group { fields, repeatable } => {
                assert!(repeatable);
                assert_eq!(fields.len(), 2);
            }
            _ => panic!("expected Group"),
        }
    }

    #[test]
    fn builder_default_label_matches_id() {
        let schema = FormSchema::new("test")
            .field("username", FieldType::Text);

        assert_eq!(schema.fields[0].label, "username");
    }

    #[test]
    fn builder_default_placeholder_is_none() {
        let schema = FormSchema::new("test")
            .field("username", FieldType::Text);

        assert!(schema.fields[0].placeholder.is_none());
    }

    #[test]
    fn builder_form_roundtrip() {
        let schema = FormSchema::new("login")
            .field("email", FieldType::Text)
            .required("email")
            .with_placeholder("email", "you@example.com")
            .field("password", FieldType::Text)
            .required("password");

        let form = Form::new(schema);
        assert_eq!(form.schema().fields.len(), 2);
        assert_eq!(form.schema().name, "login");

        let email_path = FormPath::root().push("email");
        let pwd_path = FormPath::root().push("password");
        assert!(form.state().get_field(&email_path).is_some());
        assert!(form.state().get_field(&pwd_path).is_some());
    }

    // -----------------------------------------------------------------------
    // AutocompleteHint
    // -----------------------------------------------------------------------

    #[test]
    fn autocomplete_hint_as_str() {
        assert_eq!(AutocompleteHint::Email.as_str(), "email");
        assert_eq!(AutocompleteHint::CurrentPassword.as_str(), "current-password");
        assert_eq!(AutocompleteHint::NewPassword.as_str(), "new-password");
        assert_eq!(AutocompleteHint::Username.as_str(), "username");
        assert_eq!(AutocompleteHint::Name.as_str(), "name");
        assert_eq!(AutocompleteHint::GivenName.as_str(), "given-name");
        assert_eq!(AutocompleteHint::FamilyName.as_str(), "family-name");
        assert_eq!(AutocompleteHint::Custom("cc-number".into()).as_str(), "cc-number");
    }

    #[test]
    fn autocomplete_hint_input_type() {
        assert_eq!(AutocompleteHint::CurrentPassword.input_type(), "password");
        assert_eq!(AutocompleteHint::NewPassword.input_type(), "password");
        assert_eq!(AutocompleteHint::Email.input_type(), "email");
        assert_eq!(AutocompleteHint::Username.input_type(), "text");
        assert_eq!(AutocompleteHint::Name.input_type(), "text");
    }

    #[test]
    fn builder_with_autocomplete_sets_hint() {
        let schema = FormSchema::new("login")
            .field("email", FieldType::Text)
            .with_autocomplete("email", AutocompleteHint::Email)
            .field("password", FieldType::Text)
            .with_autocomplete("password", AutocompleteHint::CurrentPassword);

        let email = schema.fields.iter().find(|f| f.id == "email").unwrap();
        let password = schema.fields.iter().find(|f| f.id == "password").unwrap();

        assert_eq!(email.autocomplete, Some(AutocompleteHint::Email));
        assert_eq!(password.autocomplete, Some(AutocompleteHint::CurrentPassword));
    }

    #[test]
    fn builder_with_autocomplete_nonexistent_field_is_noop() {
        let schema = FormSchema::new("test")
            .field("email", FieldType::Text)
            .with_autocomplete("nonexistent", AutocompleteHint::Email);

        // "email" should have no autocomplete set since we referenced a non-existent field
        let email = schema.fields.iter().find(|f| f.id == "email").unwrap();
        assert_eq!(email.autocomplete, None);
    }

    #[test]
    fn field_without_autocomplete_hint_defaults_to_none() {
        let schema = FormSchema::new("test")
            .field("username", FieldType::Text);
        let field = schema.fields.iter().find(|f| f.id == "username").unwrap();
        assert_eq!(field.autocomplete, None);
    }
}
