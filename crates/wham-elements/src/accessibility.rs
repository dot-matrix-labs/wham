use serde::{Deserialize, Serialize};

// Re-export role-independent core primitives so callers can import everything
// from a single module path.
pub use wham_core::accessibility::{A11yNode, A11yState, A11yTree};

/// ARIA roles recognised by the wham element layer.
///
/// The concrete role type used with [`A11yNode<A11yRole>`] and
/// [`A11yTree<A11yRole>`].  `wham-core` itself never references this enum —
/// it is deliberately restricted to the element layer.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum A11yRole {
    Form,
    Group,
    Label,
    TextBox,
    CheckBox,
    RadioButton,
    Button,
    ComboBox,
}

/// Convenience alias: an [`A11yNode`] whose role is [`A11yRole`].
pub type A11yNodeEl = A11yNode<A11yRole>;

/// Convenience alias: an [`A11yTree`] whose role is [`A11yRole`].
pub type A11yTreeEl = A11yTree<A11yRole>;
