pub mod accessibility;
pub mod app;
pub mod batch;
pub mod form;
pub mod hit_test;
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
pub use form::{
    FieldId, FieldSchema, FieldState, FieldType, FieldValue, Form, FormEvent, FormPath, FormSchema,
    FormState, PendingSubmission,
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
    pub use crate::form::{
        FieldId, FieldSchema, FieldType, FieldValue, Form, FormEvent, FormPath, FormSchema,
    };
    pub use crate::input::{InputEvent, KeyCode, Modifiers};
    pub use crate::text::TextBuffer;
    pub use crate::theme::Theme;
    pub use crate::types::{Color, Rect, Vec2};
    pub use crate::ui::Ui;
    pub use crate::validation::{ValidationError, ValidationRule};
}
