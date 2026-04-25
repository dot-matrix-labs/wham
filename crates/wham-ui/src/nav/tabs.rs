//! Tabs — a tabbed panel switcher.
//!
//! The `Tabs` component displays a list of tab triggers and manages which
//! panel is active. It supports horizontal and vertical orientations.
//!
//! # ARIA roles: `tablist` (container), `tab` (trigger), `tabpanel` (content)
//!
//! Keyboard navigation (per WAI-ARIA Tabs pattern):
//! - `Tab` moves focus into the tab list; `Shift+Tab` moves focus out.
//! - Within the tab list, `ArrowRight` / `ArrowDown` move to the next tab;
//!   `ArrowLeft` / `ArrowUp` move to the previous tab. Focus wraps around.
//! - `Home` focuses the first tab; `End` focuses the last tab.
//! - Focused tabs are activated immediately (follows the "automatic" pattern).

/// Orientation of the tab strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TabOrientation {
    /// Tabs are laid out left-to-right (default).
    #[default]
    Horizontal,
    /// Tabs are stacked top-to-bottom.
    Vertical,
}

/// A single tab trigger + its associated panel content.
#[derive(Clone, Debug)]
pub struct TabItem {
    /// Visible label shown on the tab trigger.
    pub label: String,
    /// Opaque panel identifier; callers use this to decide what to render.
    pub panel_id: String,
    /// Whether this tab is disabled (cannot be activated).
    pub disabled: bool,
}

impl TabItem {
    /// Create an enabled tab item.
    pub fn new(label: impl Into<String>, panel_id: impl Into<String>) -> Self {
        Self { label: label.into(), panel_id: panel_id.into(), disabled: false }
    }

    /// Mark this tab as disabled.
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

/// The outcome of a single [`Tabs::handle_key`] call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TabsEvent {
    /// No state change.
    None,
    /// The active tab changed to `index`.
    Activated { index: usize },
    /// Focus moved to `index` (but the tab was not activated).
    FocusMoved { index: usize },
}

/// Tabbed panel switcher.
///
/// # ARIA roles: `tablist` / `tab` / `tabpanel`
#[derive(Clone, Debug)]
pub struct Tabs {
    /// The tab items.
    pub items: Vec<TabItem>,
    /// Index of the currently active (selected) tab.
    pub active_index: usize,
    /// Index of the tab that currently has keyboard focus inside the tablist.
    pub focused_index: Option<usize>,
    /// Orientation of the tab strip.
    pub orientation: TabOrientation,
}

impl Tabs {
    /// Create a horizontal `Tabs` with no items and the first tab active.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            active_index: 0,
            focused_index: None,
            orientation: TabOrientation::Horizontal,
        }
    }

    /// Set the orientation.
    pub fn orientation(mut self, o: TabOrientation) -> Self {
        self.orientation = o;
        self
    }

    /// Append a tab item.
    pub fn tab(mut self, item: TabItem) -> Self {
        self.items.push(item);
        self
    }

    /// Set the initially active tab index (0-based).
    pub fn active(mut self, index: usize) -> Self {
        self.active_index = index;
        self
    }

    /// Next enabled tab index after `from`, wrapping around.
    fn next_enabled(&self, from: usize) -> Option<usize> {
        let n = self.items.len();
        if n == 0 {
            return None;
        }
        let mut i = (from + 1) % n;
        for _ in 0..n {
            if !self.items[i].disabled {
                return Some(i);
            }
            i = (i + 1) % n;
        }
        None
    }

    /// Previous enabled tab index before `from`, wrapping around.
    fn prev_enabled(&self, from: usize) -> Option<usize> {
        let n = self.items.len();
        if n == 0 {
            return None;
        }
        let mut i = if from == 0 { n - 1 } else { from - 1 };
        for _ in 0..n {
            if !self.items[i].disabled {
                return Some(i);
            }
            i = if i == 0 { n - 1 } else { i - 1 };
        }
        None
    }

    /// First enabled tab index.
    fn first_enabled(&self) -> Option<usize> {
        self.items.iter().position(|t| !t.disabled)
    }

    /// Last enabled tab index.
    fn last_enabled(&self) -> Option<usize> {
        self.items.iter().rposition(|t| !t.disabled)
    }

    /// Handle a keyboard event and return what happened.
    ///
    /// Implements the WAI-ARIA automatic-activation tabs pattern.
    pub fn handle_key(
        &mut self,
        event: &wham_core::input::InputEvent,
    ) -> TabsEvent {
        use wham_core::input::{InputEvent, KeyCode};

        match event {
            InputEvent::KeyDown { code, .. } => {
                let current = self.focused_index.unwrap_or(self.active_index);
                let (next_key, prev_key) = match self.orientation {
                    TabOrientation::Horizontal => (KeyCode::ArrowRight, KeyCode::ArrowLeft),
                    TabOrientation::Vertical => (KeyCode::ArrowDown, KeyCode::ArrowUp),
                };

                if *code == next_key {
                    if let Some(next) = self.next_enabled(current) {
                        self.focused_index = Some(next);
                        self.active_index = next;
                        return TabsEvent::Activated { index: next };
                    }
                } else if *code == prev_key {
                    if let Some(prev) = self.prev_enabled(current) {
                        self.focused_index = Some(prev);
                        self.active_index = prev;
                        return TabsEvent::Activated { index: prev };
                    }
                } else {
                    match code {
                        KeyCode::Home => {
                            if let Some(first) = self.first_enabled() {
                                self.focused_index = Some(first);
                                self.active_index = first;
                                return TabsEvent::Activated { index: first };
                            }
                        }
                        KeyCode::End => {
                            if let Some(last) = self.last_enabled() {
                                self.focused_index = Some(last);
                                self.active_index = last;
                                return TabsEvent::Activated { index: last };
                            }
                        }
                        _ => {}
                    }
                }
                TabsEvent::None
            }
            _ => TabsEvent::None,
        }
    }
}

impl Default for Tabs {
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

    fn make_tabs() -> Tabs {
        Tabs::new()
            .tab(TabItem::new("Overview", "overview"))
            .tab(TabItem::new("Details", "details"))
            .tab(TabItem::new("Reviews", "reviews"))
            .tab(TabItem::new("Disabled", "disabled").disabled())
    }

    #[test]
    fn default_active_is_zero() {
        let t = make_tabs();
        assert_eq!(t.active_index, 0);
    }

    #[test]
    fn arrow_right_activates_next_tab() {
        let mut t = make_tabs();
        t.focused_index = Some(0);
        let ev = t.handle_key(&key_down(KeyCode::ArrowRight));
        assert_eq!(ev, TabsEvent::Activated { index: 1 });
        assert_eq!(t.active_index, 1);
    }

    #[test]
    fn arrow_left_activates_prev_tab() {
        let mut t = make_tabs();
        t.focused_index = Some(2);
        let ev = t.handle_key(&key_down(KeyCode::ArrowLeft));
        assert_eq!(ev, TabsEvent::Activated { index: 1 });
    }

    #[test]
    fn arrow_right_wraps_around() {
        let mut t = make_tabs();
        // Last enabled tab is index 2 (index 3 is disabled).
        t.focused_index = Some(2);
        let ev = t.handle_key(&key_down(KeyCode::ArrowRight));
        // next_enabled from 2: wraps to 0 (skipping disabled 3).
        assert_eq!(ev, TabsEvent::Activated { index: 0 });
    }

    #[test]
    fn arrow_right_skips_disabled_tab() {
        let mut t = make_tabs();
        // Focused on 2; next would be 3 (disabled), then 0.
        t.focused_index = Some(2);
        t.handle_key(&key_down(KeyCode::ArrowRight)); // wraps to 0
        assert_eq!(t.active_index, 0);
    }

    #[test]
    fn home_activates_first_tab() {
        let mut t = make_tabs();
        t.focused_index = Some(2);
        let ev = t.handle_key(&key_down(KeyCode::Home));
        assert_eq!(ev, TabsEvent::Activated { index: 0 });
    }

    #[test]
    fn end_activates_last_enabled_tab() {
        let mut t = make_tabs();
        let ev = t.handle_key(&key_down(KeyCode::End));
        // Last enabled is 2 (3 is disabled).
        assert_eq!(ev, TabsEvent::Activated { index: 2 });
    }

    #[test]
    fn vertical_tabs_use_arrow_up_down() {
        let mut t = Tabs::new()
            .orientation(TabOrientation::Vertical)
            .tab(TabItem::new("A", "a"))
            .tab(TabItem::new("B", "b"))
            .tab(TabItem::new("C", "c"));

        t.focused_index = Some(0);
        let ev = t.handle_key(&key_down(KeyCode::ArrowDown));
        assert_eq!(ev, TabsEvent::Activated { index: 1 });

        let ev = t.handle_key(&key_down(KeyCode::ArrowUp));
        assert_eq!(ev, TabsEvent::Activated { index: 0 });
    }
}
