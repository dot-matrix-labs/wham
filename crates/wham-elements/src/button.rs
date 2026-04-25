//! Button element — an interactive control that triggers an action.
//!
//! ARIA role: `button`

use wham_core::input::{InputEvent, KeyCode};

/// The semantic purpose of a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ButtonKind {
    /// A generic push button (default).
    #[default]
    Button,
    /// A button that submits a form.
    Submit,
    /// A button that resets a form.
    Reset,
}

/// Runtime interaction state of a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ButtonState {
    /// Whether the button currently has keyboard focus.
    pub focused: bool,
    /// Whether the button is pressed (pointer down or Space/Enter held).
    pub pressed: bool,
    /// Whether the button is disabled and cannot be activated.
    pub disabled: bool,
}

/// A labelled interactive button element.
///
/// `Button` is a pure-data element: it holds a label, kind, and state. The
/// caller drives interaction by feeding [`InputEvent`]s through
/// [`Button::handle_event`] and acts on the returned [`ButtonAction`].
///
/// # ARIA role: `button`
#[derive(Clone, Debug)]
pub struct Button {
    /// Accessible label (maps to `aria-label` when there is no visible text).
    pub label: String,
    /// Semantic button type.
    pub kind: ButtonKind,
    /// Current interaction state.
    pub state: ButtonState,
}

/// The outcome of processing one input event against a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonAction {
    /// No state change.
    None,
    /// The button was activated (click, Enter, or Space).
    Activated,
    /// Focus entered the button.
    FocusGained,
    /// Focus left the button.
    FocusLost,
}

impl Button {
    /// Create a new enabled button with the given label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            kind: ButtonKind::default(),
            state: ButtonState::default(),
        }
    }

    /// Create a disabled button.
    pub fn disabled(mut self) -> Self {
        self.state.disabled = true;
        self
    }

    /// Set the button kind.
    pub fn kind(mut self, kind: ButtonKind) -> Self {
        self.kind = kind;
        self
    }

    /// Process a single input event and return the resulting action.
    ///
    /// Only keyboard events that are meaningful for buttons (Enter, Space) are
    /// handled here. Pointer events and focus management are the caller's
    /// responsibility.
    pub fn handle_event(&mut self, event: &InputEvent) -> ButtonAction {
        if self.state.disabled {
            return ButtonAction::None;
        }
        match event {
            InputEvent::KeyDown { code, .. } => match code {
                KeyCode::Enter => {
                    self.state.pressed = true;
                    ButtonAction::None
                }
                KeyCode::Other(s) if s == " " => {
                    self.state.pressed = true;
                    let _ = s;
                    ButtonAction::None
                }
                _ => ButtonAction::None,
            },
            InputEvent::KeyUp { code, .. } => match code {
                KeyCode::Enter => {
                    self.state.pressed = false;
                    ButtonAction::Activated
                }
                KeyCode::Other(s) if s == " " => {
                    self.state.pressed = false;
                    let _ = s;
                    ButtonAction::Activated
                }
                _ => ButtonAction::None,
            },
            _ => ButtonAction::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wham_core::input::Modifiers;

    fn key_down(code: KeyCode) -> InputEvent {
        InputEvent::KeyDown { code, modifiers: Modifiers::default() }
    }

    fn key_up(code: KeyCode) -> InputEvent {
        InputEvent::KeyUp { code, modifiers: Modifiers::default() }
    }

    #[test]
    fn button_new_defaults() {
        let b = Button::new("Click me");
        assert_eq!(b.label, "Click me");
        assert_eq!(b.kind, ButtonKind::Button);
        assert!(!b.state.disabled);
        assert!(!b.state.pressed);
        assert!(!b.state.focused);
    }

    #[test]
    fn enter_activates_button() {
        let mut b = Button::new("OK");
        b.handle_event(&key_down(KeyCode::Enter));
        let action = b.handle_event(&key_up(KeyCode::Enter));
        assert_eq!(action, ButtonAction::Activated);
    }

    #[test]
    fn space_activates_button() {
        let mut b = Button::new("OK");
        b.handle_event(&key_down(KeyCode::Other(" ".into())));
        let action = b.handle_event(&key_up(KeyCode::Other(" ".into())));
        assert_eq!(action, ButtonAction::Activated);
    }

    #[test]
    fn disabled_button_ignores_keys() {
        let mut b = Button::new("OK").disabled();
        b.handle_event(&key_down(KeyCode::Enter));
        let action = b.handle_event(&key_up(KeyCode::Enter));
        assert_eq!(action, ButtonAction::None);
    }
}
