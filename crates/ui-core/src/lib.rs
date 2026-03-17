//! `ui-core` — platform-agnostic GPU-rendered forms library.
//!
//! This crate re-exports the full public API of `wham-core` and `wham-elements`,
//! and provides the immediate-mode widget API (`ui`) and `FormApp` trait (`app`).
//! It has no dependency on browser APIs and can be compiled and tested with
//! `cargo test` on any host platform.
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

/// [`FormApp`] trait for building form applications.
pub mod app;
/// Immediate-mode widget API and layout engine.
pub mod ui;

// Re-export wham-core primitives
pub use wham_core::batch::{Batch, DrawCmd, Material, Quad, TextRun, Vertex};
pub use wham_core::hit_test::{HitTestEntry, HitTestGrid};
pub use wham_core::input::{InputEvent, KeyCode, Modifiers, PointerButton, PointerEvent, TextInputEvent};
pub use wham_core::state::History;
pub use wham_core::theme::{Theme, ThemeColors};
pub use wham_core::types::{Color, Rect, Vec2};

// Re-export wham-elements (form model, text editing, validation, accessibility)
pub use wham_elements::accessibility::{A11yNode, A11yNodeEl, A11yRole, A11yState, A11yTree, A11yTreeEl};
pub use wham_elements::form::{
    AutocompleteHint, FieldId, FieldSchema, FieldState, FieldType, FieldValue, Form, FormEvent,
    FormPath, FormSchema, FormState, PendingSubmission,
};
pub use wham_elements::icon::{IconEntry, IconId, IconPack};
pub use wham_elements::text::{Caret, Selection, TextBuffer, TextEditOp};
pub use wham_elements::validation::{ValidationError, ValidationRule};

// Re-export ui
pub use app::FormApp;
pub use ui::{Layout, Ui, WidgetInfo, WidgetKind};

// Shim module aliases for backward compatibility
/// Accessibility tree — re-exported from `wham-elements`.
pub mod accessibility {
    pub use wham_elements::accessibility::*;
}
/// Vertex/index buffer builder — re-exported from `wham-core`.
pub mod batch {
    pub use wham_core::batch::*;
}
/// Form model — re-exported from `wham-elements`.
pub mod form {
    pub use wham_elements::form::*;
}
/// Spatial hash grid — re-exported from `wham-core`.
pub mod hit_test {
    pub use wham_core::hit_test::*;
}
/// Icon pack — re-exported from `wham-elements`.
pub mod icon {
    pub use wham_elements::icon::*;
}
/// Input event types — re-exported from `wham-core`.
pub mod input {
    pub use wham_core::input::*;
}
/// History stack — re-exported from `wham-core`.
pub mod state {
    pub use wham_core::state::*;
}
/// Text buffer — re-exported from `wham-elements`.
pub mod text {
    pub use wham_elements::text::*;
}
/// Theme tokens — re-exported from `wham-core`.
pub mod theme {
    pub use wham_core::theme::*;
}
/// Geometry types — re-exported from `wham-core`.
pub mod types {
    pub use wham_core::types::*;
}
/// Validation rules — re-exported from `wham-elements`.
pub mod validation {
    pub use wham_elements::validation::*;
}

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
