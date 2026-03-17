//! `wham-elements` — HTML spec element layer for the wham GPU-rendered forms library.
//!
//! This crate provides the form model, text editing, validation rules, and
//! accessibility tree for building accessible GPU-rendered form UIs. It depends
//! only on `wham-core` and has no dependency on browser APIs; it can be compiled
//! and tested with `cargo test` on any host platform.
//!
//! # Modules
//!
//! - [`accessibility`] — Accessibility tree: ARIA roles, states, and the node hierarchy.
//! - [`form`] — Form model: schema, field values, validation, and submission lifecycle.
//! - [`icon`] — Icon pack loading and UV coordinate lookup.
//! - [`text`] — Grapheme-aware text buffer with caret, selection, IME, and undo/redo.
//! - [`validation`] — Field-level and cross-field validation rules.

/// ARIA role: none (structural) — accessibility tree: roles, states, and the hidden DOM mirror interface.
pub mod accessibility;
/// ARIA role: form — form model: schema, field values, validation, and submission lifecycle.
pub mod form;
/// ARIA role: img — icon pack loading and UV coordinate lookup.
pub mod icon;
/// ARIA role: textbox — grapheme-aware text buffer with caret, selection, IME, and undo/redo.
pub mod text;
/// ARIA role: none (structural) — validation rules and error types for form fields.
pub mod validation;

pub use accessibility::{A11yNode, A11yNodeEl, A11yRole, A11yState, A11yTree, A11yTreeEl};
pub use form::{
    AutocompleteHint, FieldId, FieldSchema, FieldState, FieldType, FieldValue, Form, FormEvent,
    FormPath, FormSchema, FormState, PendingSubmission,
};
pub use icon::{IconEntry, IconId, IconPack};
pub use text::{Caret, Selection, TextBuffer, TextEditOp};
pub use validation::{ValidationError, ValidationRule};
