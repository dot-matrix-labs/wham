//! `ui-core` — platform-agnostic GPU-rendered forms library.
//!
//! This crate provides the immediate-mode widget API, form model, text editing,
//! validation, and batching primitives. It has no dependency on browser APIs and
//! can be compiled and tested with `cargo test` on any host platform.
//!
//! Primitive types (`batch`, `hit_test`, `input`, `state`, `theme`, `types`)
//! are provided by the `wham-core` crate and re-exported here for backward
//! compatibility.
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use ui_core::prelude::*;
//!
//! let schema = FormSchema::new("login")
//!     .field("email", FieldType::Text)
//!     .required("email");
//!
//! let mut form = Form::new(schema);
//! let theme = Theme::default_light();
//! let mut ui = Ui::new(800.0, 600.0, theme);
//!
//! // Each animation frame:
//! ui.begin_frame(events, width, height, scale, time_ms);
//! ui.label("Sign in");
//! // ... emit widgets ...
//! let a11y = ui.end_frame();
//! // Render ui.batch() with the WebGL2 renderer.
//! ```
//!
//! See [`docs/getting-started.md`](https://github.com/your-org/wham/blob/main/docs/getting-started.md)
//! for a full walkthrough.

// Re-export primitive modules from wham-core for backward compatibility.
pub use wham_core::batch;
pub use wham_core::hit_test;
pub use wham_core::input;
pub use wham_core::state;
pub use wham_core::theme;
pub use wham_core::types;

/// Accessibility tree — roles, states, and the hidden DOM mirror interface.
pub mod accessibility;
/// [`FormApp`] trait for building form applications.
pub mod app;
/// Form model: schema, field values, validation, and submission lifecycle.
pub mod form;
/// Icon pack loading and UV coordinate lookup.
pub mod icon;
/// Grapheme-aware text buffer with caret, selection, IME, and undo/redo.
pub mod text;
/// Immediate-mode widget API and layout engine.
pub mod ui;
/// Validation rules and error types for form fields.
pub mod validation;

pub use accessibility::{A11yNode, A11yRole, A11yState, A11yTree};
pub use app::FormApp;
pub use wham_core::batch::{Batch, DrawCmd, Material, Quad, TextRun, Vertex};
pub use wham_core::hit_test::{HitTestEntry, HitTestGrid};
pub use icon::{IconEntry, IconId, IconPack};
pub use form::{
    AutocompleteHint, FieldId, FieldSchema, FieldState, FieldType, FieldValue, Form, FormEvent,
    FormPath, FormSchema, FormState, PendingSubmission,
};
pub use wham_core::input::{InputEvent, KeyCode, Modifiers, PointerButton, PointerEvent, TextInputEvent};
pub use wham_core::state::History;
pub use text::{Caret, Selection, TextBuffer, TextEditOp};
pub use wham_core::theme::{Theme, ThemeColors};
pub use wham_core::types::{Color, Rect, Vec2};
pub use ui::{Layout, Ui, WidgetInfo, WidgetKind};
pub use validation::{ValidationError, ValidationRule};

/// Convenience re-exports for the most commonly used types.
///
/// ```rust,ignore
/// use ui_core::prelude::*;
/// ```
pub mod prelude {
    pub use crate::accessibility::{A11yNode, A11yRole, A11yState, A11yTree};
    pub use crate::app::FormApp;
    pub use wham_core::batch::{Batch, DrawCmd, Quad, TextRun, Vertex};
    pub use crate::icon::{IconId, IconPack};
    pub use crate::form::{
        AutocompleteHint, FieldId, FieldSchema, FieldType, FieldValue, Form, FormEvent, FormPath,
        FormSchema,
    };
    pub use wham_core::input::{InputEvent, KeyCode, Modifiers};
    pub use crate::text::TextBuffer;
    pub use wham_core::theme::Theme;
    pub use wham_core::types::{Color, Rect, Vec2};
    pub use crate::ui::Ui;
    pub use crate::validation::{ValidationError, ValidationRule};
}
