//! Sidebar — a collapsible vertical navigation panel.
//!
//! The `Sidebar` renders a vertical list of named sections, each containing
//! navigation links. Sections can be individually collapsed. The sidebar
//! itself can also be collapsed (e.g. on narrow viewports).
//!
//! # ARIA role: `navigation` (landmark)
//!
//! Keyboard navigation:
//! - `Tab` / `Shift+Tab` move focus through visible items.
//! - `Enter` or `Space` on a section header toggles the section.
//! - `Enter` on a link follows it.
//! - `ArrowDown` / `ArrowUp` move focus between items without activating them.
//! - `Home` / `End` jump to the first / last visible item.

use wham_elements::Link;

/// A single link item inside a sidebar section.
#[derive(Clone, Debug)]
pub struct SidebarItem {
    /// The underlying link element.
    pub link: Link,
}

impl SidebarItem {
    /// Create a sidebar item with the given label and href.
    pub fn new(label: impl Into<String>, href: impl Into<String>) -> Self {
        Self { link: Link::new(label, href) }
    }

    /// Mark this item as the currently active page.
    pub fn current(mut self) -> Self {
        self.link = self.link.current();
        self
    }
}

/// A collapsible group of links.
///
/// # ARIA role: within the `navigation` landmark; section header has role
/// `button` with `aria-expanded`.
#[derive(Clone, Debug)]
pub struct SidebarSection {
    /// Visible section heading.
    pub title: String,
    /// Links within this section.
    pub items: Vec<SidebarItem>,
    /// Whether the section is expanded and its items are visible.
    pub expanded: bool,
}

impl SidebarSection {
    /// Create an expanded section with the given title.
    pub fn new(title: impl Into<String>) -> Self {
        Self { title: title.into(), items: Vec::new(), expanded: true }
    }

    /// Start the section in a collapsed state.
    pub fn collapsed(mut self) -> Self {
        self.expanded = false;
        self
    }

    /// Append a link item to this section.
    pub fn item(mut self, item: SidebarItem) -> Self {
        self.items.push(item);
        self
    }

    /// Toggle the expanded/collapsed state.
    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    /// Number of visible items (0 if collapsed).
    pub fn visible_item_count(&self) -> usize {
        if self.expanded { self.items.len() } else { 0 }
    }
}

/// The result of a single [`Sidebar::handle_key`] call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarEvent {
    /// No interactive change.
    None,
    /// A section at `section_index` was toggled.
    SectionToggled { section_index: usize },
    /// The link at `(section_index, item_index)` was followed.
    LinkFollowed { section_index: usize, item_index: usize },
    /// Focus moved to a new item.
    FocusMoved,
}

/// Identifies the currently focused element in the sidebar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarFocus {
    /// A section header at the given index.
    Section(usize),
    /// An item within a section: (section_index, item_index).
    Item(usize, usize),
}

/// Vertical navigation sidebar with collapsible sections.
///
/// # ARIA role: `navigation`
#[derive(Clone, Debug)]
pub struct Sidebar {
    /// Whether the entire sidebar is collapsed.
    pub collapsed: bool,
    /// The sections and their link items.
    pub sections: Vec<SidebarSection>,
    /// Currently focused element, if any.
    pub focus: Option<SidebarFocus>,
}

impl Sidebar {
    /// Create an expanded sidebar with no sections.
    pub fn new() -> Self {
        Self { collapsed: false, sections: Vec::new(), focus: None }
    }

    /// Start the sidebar in a collapsed state.
    pub fn collapsed(mut self) -> Self {
        self.collapsed = true;
        self
    }

    /// Append a section.
    pub fn section(mut self, section: SidebarSection) -> Self {
        self.sections.push(section);
        self
    }

    /// Build a flat ordered list of all focusable elements for keyboard nav.
    fn focusable_items(&self) -> Vec<SidebarFocus> {
        let mut items = Vec::new();
        for (si, section) in self.sections.iter().enumerate() {
            items.push(SidebarFocus::Section(si));
            if section.expanded {
                for (ii, _) in section.items.iter().enumerate() {
                    items.push(SidebarFocus::Item(si, ii));
                }
            }
        }
        items
    }

    /// Handle a keyboard event and return what happened.
    pub fn handle_key(
        &mut self,
        event: &wham_core::input::InputEvent,
    ) -> SidebarEvent {
        use wham_core::input::{InputEvent, KeyCode};

        if self.collapsed {
            return SidebarEvent::None;
        }

        match event {
            InputEvent::KeyDown { code, modifiers } => {
                let focusable = self.focusable_items();
                let current_pos = self.focus.and_then(|f| focusable.iter().position(|x| *x == f));

                match code {
                    KeyCode::Tab => {
                        let shift = modifiers.shift;
                        let next_pos = match current_pos {
                            None if !shift => Some(0),
                            None => None,
                            Some(0) if shift => None,
                            Some(i) if shift => Some(i - 1),
                            Some(i) if i + 1 < focusable.len() => Some(i + 1),
                            _ => None,
                        };
                        self.focus = next_pos.and_then(|p| focusable.get(p).copied());
                        SidebarEvent::FocusMoved
                    }
                    KeyCode::ArrowDown => {
                        let next_pos = match current_pos {
                            None => Some(0),
                            Some(i) if i + 1 < focusable.len() => Some(i + 1),
                            Some(i) => Some(i),
                        };
                        self.focus = next_pos.and_then(|p| focusable.get(p).copied());
                        SidebarEvent::FocusMoved
                    }
                    KeyCode::ArrowUp => {
                        let next_pos = match current_pos {
                            None | Some(0) => Some(0),
                            Some(i) => Some(i - 1),
                        };
                        self.focus = next_pos.and_then(|p| focusable.get(p).copied());
                        SidebarEvent::FocusMoved
                    }
                    KeyCode::Home => {
                        self.focus = focusable.first().copied();
                        SidebarEvent::FocusMoved
                    }
                    KeyCode::End => {
                        self.focus = focusable.last().copied();
                        SidebarEvent::FocusMoved
                    }
                    KeyCode::Enter | KeyCode::Other(_) => {
                        let is_space = matches!(code, KeyCode::Other(s) if s == " ");
                        if !matches!(code, KeyCode::Enter) && !is_space {
                            return SidebarEvent::None;
                        }
                        match self.focus {
                            Some(SidebarFocus::Section(si)) => {
                                if si < self.sections.len() {
                                    self.sections[si].toggle();
                                    SidebarEvent::SectionToggled { section_index: si }
                                } else {
                                    SidebarEvent::None
                                }
                            }
                            Some(SidebarFocus::Item(si, ii)) => {
                                SidebarEvent::LinkFollowed {
                                    section_index: si,
                                    item_index: ii,
                                }
                            }
                            None => SidebarEvent::None,
                        }
                    }
                    _ => SidebarEvent::None,
                }
            }
            _ => SidebarEvent::None,
        }
    }
}

impl Default for Sidebar {
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

    fn make_sidebar() -> Sidebar {
        Sidebar::new()
            .section(
                SidebarSection::new("Main")
                    .item(SidebarItem::new("Dashboard", "/dashboard").current())
                    .item(SidebarItem::new("Reports", "/reports")),
            )
            .section(
                SidebarSection::new("Settings")
                    .item(SidebarItem::new("Profile", "/profile"))
                    .item(SidebarItem::new("Billing", "/billing")),
            )
    }

    #[test]
    fn sidebar_has_two_sections() {
        let s = make_sidebar();
        assert_eq!(s.sections.len(), 2);
    }

    #[test]
    fn arrow_down_advances_focus() {
        let mut s = make_sidebar();
        s.handle_key(&key_down(KeyCode::ArrowDown)); // Section(0)
        s.handle_key(&key_down(KeyCode::ArrowDown)); // Item(0,0)
        assert_eq!(s.focus, Some(SidebarFocus::Item(0, 0)));
    }

    #[test]
    fn enter_on_section_toggles_it() {
        let mut s = make_sidebar();
        s.focus = Some(SidebarFocus::Section(0));
        assert!(s.sections[0].expanded);
        let ev = s.handle_key(&key_down(KeyCode::Enter));
        assert_eq!(ev, SidebarEvent::SectionToggled { section_index: 0 });
        assert!(!s.sections[0].expanded);
    }

    #[test]
    fn enter_on_item_fires_link_followed() {
        let mut s = make_sidebar();
        s.focus = Some(SidebarFocus::Item(0, 1));
        let ev = s.handle_key(&key_down(KeyCode::Enter));
        assert_eq!(ev, SidebarEvent::LinkFollowed { section_index: 0, item_index: 1 });
    }

    #[test]
    fn collapsed_sidebar_ignores_keys() {
        let mut s = make_sidebar().collapsed();
        let ev = s.handle_key(&key_down(KeyCode::ArrowDown));
        assert_eq!(ev, SidebarEvent::None);
    }

    #[test]
    fn collapsed_section_hides_items_from_focus_order() {
        let mut s = Sidebar::new().section(
            SidebarSection::new("Settings")
                .collapsed()
                .item(SidebarItem::new("Profile", "/profile")),
        );
        // Only the section header is focusable, not the hidden item.
        let focusable = s.focusable_items();
        assert_eq!(focusable.len(), 1);
        assert_eq!(focusable[0], SidebarFocus::Section(0));
        // Expanding it makes the item focusable.
        s.sections[0].toggle();
        let focusable2 = s.focusable_items();
        assert_eq!(focusable2.len(), 2);
    }

    #[test]
    fn home_end_navigation() {
        let mut s = make_sidebar();
        s.handle_key(&key_down(KeyCode::End));
        assert_eq!(s.focus, Some(SidebarFocus::Item(1, 1)));
        s.handle_key(&key_down(KeyCode::Home));
        assert_eq!(s.focus, Some(SidebarFocus::Section(0)));
    }
}
