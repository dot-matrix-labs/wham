//! Link element — a navigational hyperlink.
//!
//! ARIA role: `link`

use wham_core::input::{InputEvent, KeyCode};

/// Runtime interaction state of a link.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct LinkState {
    /// Whether the link currently has keyboard focus.
    pub focused: bool,
    /// Whether the link is "current" (i.e. represents the active page/section).
    pub current: bool,
}

/// A navigational link element.
///
/// Carries a human-readable label and an opaque `href` string. The
/// `href` is intentionally `String` rather than a URL type so this crate
/// stays browser-agnostic. The caller interprets the href value.
///
/// # ARIA role: `link`
#[derive(Clone, Debug)]
pub struct Link {
    /// Visible label text.
    pub label: String,
    /// Destination (URL path, anchor id, or any opaque string).
    pub href: String,
    /// Current interaction state.
    pub state: LinkState,
}

/// The outcome of processing one input event against a link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkAction {
    /// No state change.
    None,
    /// The link was followed (Enter key or pointer click).
    Followed,
}

impl Link {
    /// Create a new link with the given label and href.
    pub fn new(label: impl Into<String>, href: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: href.into(),
            state: LinkState::default(),
        }
    }

    /// Mark this link as the current page/section.
    pub fn current(mut self) -> Self {
        self.state.current = true;
        self
    }

    /// Process a single input event and return the resulting action.
    pub fn handle_event(&mut self, event: &InputEvent) -> LinkAction {
        match event {
            InputEvent::KeyDown { code, .. } => match code {
                KeyCode::Enter => LinkAction::Followed,
                _ => LinkAction::None,
            },
            _ => LinkAction::None,
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

    #[test]
    fn link_new_defaults() {
        let l = Link::new("Home", "/");
        assert_eq!(l.label, "Home");
        assert_eq!(l.href, "/");
        assert!(!l.state.current);
        assert!(!l.state.focused);
    }

    #[test]
    fn enter_follows_link() {
        let mut l = Link::new("Home", "/");
        let action = l.handle_event(&key_down(KeyCode::Enter));
        assert_eq!(action, LinkAction::Followed);
    }

    #[test]
    fn other_keys_do_nothing() {
        let mut l = Link::new("Home", "/");
        let action = l.handle_event(&key_down(KeyCode::Tab));
        assert_eq!(action, LinkAction::None);
    }

    #[test]
    fn current_link_flag() {
        let l = Link::new("Home", "/").current();
        assert!(l.state.current);
    }
}
