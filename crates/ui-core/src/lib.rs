//! `ui-core` — platform-agnostic GPU-rendered forms library.
//!
//! This crate provides the immediate-mode widget API, form model, text editing,
//! validation, and batching primitives. It has no dependency on browser APIs and
//! can be compiled and tested with `cargo test` on any host platform.
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

#![warn(missing_docs)]

pub mod accessibility;
pub mod app;
pub mod batch;
pub mod form;
pub mod hit_test;
pub mod icon;
pub mod input;
pub mod state;
pub mod text;
pub mod theme;
pub mod types;
pub mod ui;
pub mod validation;

pub use accessibility::{A11yNode, A11yRole, A11yState, A11yTree};
pub use app::FormApp;
pub use batch::{Batch, DrawCmd, Material, Quad, TextRun, Vertex};
pub use icon::{IconEntry, IconId, IconPack};
pub use form::{
    AutocompleteHint, FieldId, FieldSchema, FieldState, FieldType, FieldValue, Form, FormEvent,
    FormPath, FormSchema, FormState, PendingSubmission,
};
pub use input::{InputEvent, KeyCode, Modifiers, PointerButton, PointerEvent, TextInputEvent};
pub use state::History;
pub use text::{Caret, Selection, TextBuffer, TextEditOp};
pub use theme::{Theme, ThemeColors};
pub use types::{Color, Rect, Vec2};
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
    pub use crate::batch::{Batch, DrawCmd, Quad, TextRun, Vertex};
    pub use crate::icon::{IconId, IconPack};
    pub use crate::form::{
        AutocompleteHint, FieldId, FieldSchema, FieldType, FieldValue, Form, FormEvent, FormPath,
        FormSchema,
    };
    pub use crate::input::{InputEvent, KeyCode, Modifiers};
    pub use crate::text::TextBuffer;
    pub use crate::theme::Theme;
    pub use crate::types::{Color, Rect, Vec2};
    pub use crate::ui::Ui;
    pub use crate::validation::{ValidationError, ValidationRule};
}
