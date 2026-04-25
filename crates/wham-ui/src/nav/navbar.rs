//! Navbar — a top-of-page navigation banner.
//!
//! The `Navbar` renders a horizontal strip containing a logo slot, a list of
//! navigation links, and an optional actions slot. It maps to the HTML
//! `<header>` element containing a `<nav>`.
//!
//! # ARIA role: `banner` (outer landmark) + `navigation` (inner nav region)
//!
//! Keyboard navigation:
//! - `Tab` / `Shift+Tab` move focus through links and action buttons.
//! - `Enter` on a focused link follows it; `Enter` / `Space` on a focused
//!   action button activates it.
//! - `Home` / `End` move focus to the first / last interactive item.

use wham_elements::{Button, Link};

/// A single navigation link entry in the navbar.
#[derive(Clone, Debug)]
pub struct NavLink {
    /// The underlying link element (label, href, current state).
    pub link: Link,
}

impl NavLink {
    /// Create a nav link with the given label and href.
    pub fn new(label: impl Into<String>, href: impl Into<String>) -> Self {
        Self { link: Link::new(label, href) }
    }

    /// Mark this link as the currently active page.
    pub fn current(mut self) -> Self {
        self.link = self.link.current();
        self
    }
}

/// The result of a single [`Navbar::handle_key`] call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavbarEvent {
    /// No interactive change.
    None,
    /// The link at `index` was activated (Enter on a focused link).
    LinkFollowed { index: usize },
    /// The action button at `index` was activated.
    ActionActivated { index: usize },
    /// Focus moved to the item at `focused_index`.
    FocusMoved { focused_index: usize },
}

/// Top-of-page navigation bar.
///
/// Contains an optional logo label, a list of nav links, and an optional list
/// of action buttons (e.g. "Sign in", "Get started").
///
/// # ARIA role: `banner` (outer) + `navigation` (nav region)
#[derive(Clone, Debug)]
pub struct Navbar {
    /// Optional logo or site-name text shown at the far left.
    pub logo: Option<String>,
    /// The primary navigation links.
    pub links: Vec<NavLink>,
    /// Optional action buttons in the trailing slot.
    pub actions: Vec<Button>,
    /// Index of the currently focused interactive item.
    ///
    /// `None` means no item is focused. The items are indexed as:
    /// `0..links.len()` for links, then `links.len()..` for actions.
    pub focused_index: Option<usize>,
}

impl Navbar {
    /// Create a new `Navbar` with no items.
    pub fn new() -> Self {
        Self { logo: None, links: Vec::new(), actions: Vec::new(), focused_index: None }
    }

    /// Set the logo / site-name label.
    pub fn logo(mut self, logo: impl Into<String>) -> Self {
        self.logo = Some(logo.into());
        self
    }

    /// Append a navigation link.
    pub fn link(mut self, link: NavLink) -> Self {
        self.links.push(link);
        self
    }

    /// Append an action button.
    pub fn action(mut self, button: Button) -> Self {
        self.actions.push(button);
        self
    }

    /// Total number of interactive items (links + action buttons).
    pub fn item_count(&self) -> usize {
        self.links.len() + self.actions.len()
    }

    /// Move focus to the first interactive item.
    pub fn focus_first(&mut self) {
        if self.item_count() > 0 {
            self.focused_index = Some(0);
        }
    }

    /// Move focus to the last interactive item.
    pub fn focus_last(&mut self) {
        let count = self.item_count();
        if count > 0 {
            self.focused_index = Some(count - 1);
        }
    }

    /// Handle a keyboard event and return what happened.
    ///
    /// Processes `Tab`, `Shift+Tab`, `Home`, `End`, `Enter`, and `Space`.
    pub fn handle_key(
        &mut self,
        event: &wham_core::input::InputEvent,
    ) -> NavbarEvent {
        use wham_core::input::{InputEvent, KeyCode};

        match event {
            InputEvent::KeyDown { code, modifiers } => match code {
                KeyCode::Tab => {
                    let count = self.item_count();
                    if count == 0 {
                        return NavbarEvent::None;
                    }
                    if modifiers.shift {
                        // Shift+Tab: move focus backward.
                        self.focused_index = match self.focused_index {
                            Some(0) | None => None,
                            Some(i) => Some(i - 1),
                        };
                    } else {
                        // Tab: move focus forward.
                        self.focused_index = match self.focused_index {
                            None => Some(0),
                            Some(i) if i + 1 < count => Some(i + 1),
                            _ => None,
                        };
                    }
                    match self.focused_index {
                        Some(idx) => NavbarEvent::FocusMoved { focused_index: idx },
                        None => NavbarEvent::None,
                    }
                }
                KeyCode::Home => {
                    self.focus_first();
                    match self.focused_index {
                        Some(idx) => NavbarEvent::FocusMoved { focused_index: idx },
                        None => NavbarEvent::None,
                    }
                }
                KeyCode::End => {
                    self.focus_last();
                    match self.focused_index {
                        Some(idx) => NavbarEvent::FocusMoved { focused_index: idx },
                        None => NavbarEvent::None,
                    }
                }
                KeyCode::Enter => {
                    let idx = match self.focused_index {
                        Some(i) => i,
                        None => return NavbarEvent::None,
                    };
                    if idx < self.links.len() {
                        NavbarEvent::LinkFollowed { index: idx }
                    } else {
                        let action_idx = idx - self.links.len();
                        if action_idx < self.actions.len() {
                            NavbarEvent::ActionActivated { index: action_idx }
                        } else {
                            NavbarEvent::None
                        }
                    }
                }
                KeyCode::Other(s) if s == " " => {
                    // Space activates action buttons (not links).
                    let idx = match self.focused_index {
                        Some(i) => i,
                        None => return NavbarEvent::None,
                    };
                    if idx >= self.links.len() {
                        let action_idx = idx - self.links.len();
                        if action_idx < self.actions.len() {
                            return NavbarEvent::ActionActivated { index: action_idx };
                        }
                    }
                    NavbarEvent::None
                }
                _ => NavbarEvent::None,
            },
            _ => NavbarEvent::None,
        }
    }
}

impl Default for Navbar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wham_core::input::{InputEvent, KeyCode, Modifiers};

    fn key_down(code: KeyCode) -> InputEvent {
        InputEvent::KeyDown { code, modifiers: Modifiers::default() }
    }

    fn shift_tab() -> InputEvent {
        InputEvent::KeyDown {
            code: KeyCode::Tab,
            modifiers: Modifiers { shift: true, ..Default::default() },
        }
    }

    fn make_navbar() -> Navbar {
        Navbar::new()
            .logo("Acme")
            .link(NavLink::new("Home", "/").current())
            .link(NavLink::new("About", "/about"))
            .link(NavLink::new("Blog", "/blog"))
            .action(Button::new("Sign in"))
    }

    #[test]
    fn item_count_includes_links_and_actions() {
        let nav = make_navbar();
        assert_eq!(nav.item_count(), 4); // 3 links + 1 action
    }

    #[test]
    fn tab_advances_focus() {
        let mut nav = make_navbar();
        let ev = nav.handle_key(&key_down(KeyCode::Tab));
        assert_eq!(ev, NavbarEvent::FocusMoved { focused_index: 0 });
        let ev = nav.handle_key(&key_down(KeyCode::Tab));
        assert_eq!(ev, NavbarEvent::FocusMoved { focused_index: 1 });
    }

    #[test]
    fn shift_tab_moves_focus_back() {
        let mut nav = make_navbar();
        nav.handle_key(&key_down(KeyCode::Tab)); // focus 0
        nav.handle_key(&key_down(KeyCode::Tab)); // focus 1
        let ev = nav.handle_key(&shift_tab());
        assert_eq!(ev, NavbarEvent::FocusMoved { focused_index: 0 });
    }

    #[test]
    fn home_moves_to_first_item() {
        let mut nav = make_navbar();
        nav.focused_index = Some(3);
        let ev = nav.handle_key(&key_down(KeyCode::Home));
        assert_eq!(ev, NavbarEvent::FocusMoved { focused_index: 0 });
    }

    #[test]
    fn end_moves_to_last_item() {
        let mut nav = make_navbar();
        let ev = nav.handle_key(&key_down(KeyCode::End));
        assert_eq!(ev, NavbarEvent::FocusMoved { focused_index: 3 });
    }

    #[test]
    fn enter_on_link_fires_link_followed() {
        let mut nav = make_navbar();
        nav.focused_index = Some(1);
        let ev = nav.handle_key(&key_down(KeyCode::Enter));
        assert_eq!(ev, NavbarEvent::LinkFollowed { index: 1 });
    }

    #[test]
    fn enter_on_action_fires_action_activated() {
        let mut nav = make_navbar();
        nav.focused_index = Some(3); // first action
        let ev = nav.handle_key(&key_down(KeyCode::Enter));
        assert_eq!(ev, NavbarEvent::ActionActivated { index: 0 });
    }

    #[test]
    fn space_on_action_fires_action_activated() {
        let mut nav = make_navbar();
        nav.focused_index = Some(3);
        let ev = nav.handle_key(&key_down(KeyCode::Other(" ".into())));
        assert_eq!(ev, NavbarEvent::ActionActivated { index: 0 });
    }

    #[test]
    fn space_on_link_does_nothing() {
        let mut nav = make_navbar();
        nav.focused_index = Some(0);
        let ev = nav.handle_key(&key_down(KeyCode::Other(" ".into())));
        assert_eq!(ev, NavbarEvent::None);
    }

    #[test]
    fn tab_past_end_clears_focus() {
        let mut nav = make_navbar();
        nav.focused_index = Some(3); // last item
        let ev = nav.handle_key(&key_down(KeyCode::Tab));
        assert_eq!(ev, NavbarEvent::None);
        assert_eq!(nav.focused_index, None);
    }
}
