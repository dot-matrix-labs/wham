//! Breadcrumb — a hierarchical trail of navigation links.
//!
//! The `Breadcrumb` shows the path from the root to the current page. When
//! there are too many items, interior items are truncated to an ellipsis.
//!
//! # ARIA role: `navigation` with `aria-label="Breadcrumb"`
//!
//! The current (last) item has `aria-current="page"`.
//!
//! Keyboard navigation:
//! - `Tab` / `Shift+Tab` move focus through the visible crumbs.
//! - `Enter` on a focused crumb follows the link.
//! - `ArrowLeft` / `ArrowRight` also move focus between crumbs.

use wham_elements::Link;

/// A single step in the breadcrumb trail.
#[derive(Clone, Debug)]
pub struct BreadcrumbItem {
    /// The underlying link element.
    pub link: Link,
}

impl BreadcrumbItem {
    /// Create a crumb with the given label and href.
    pub fn new(label: impl Into<String>, href: impl Into<String>) -> Self {
        Self { link: Link::new(label, href) }
    }
}

/// The outcome of a single [`Breadcrumb::handle_key`] call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BreadcrumbEvent {
    /// No interactive change.
    None,
    /// The crumb at `index` (in the *original* items list, not the truncated
    /// view) was followed.
    Followed { index: usize },
    /// Focus moved to the crumb at `index` (original items list).
    FocusMoved { index: usize },
}

/// Hierarchical navigation breadcrumb.
///
/// Supports overflow truncation: when `items.len() > max_visible`, the middle
/// items are hidden and replaced with an ellipsis placeholder.
///
/// # ARIA role: `navigation` (`aria-label="Breadcrumb"`)
#[derive(Clone, Debug)]
pub struct Breadcrumb {
    /// All breadcrumb items (root → … → current page).
    pub items: Vec<BreadcrumbItem>,
    /// Maximum number of items to show before truncating the middle.
    ///
    /// When `items.len() > max_visible`, the component always shows the first
    /// item, an ellipsis, and the last `max_visible - 2` items.
    /// A value of `0` means "show all".
    pub max_visible: usize,
    /// Index (into `items`) of the currently focused crumb, if any.
    pub focused_index: Option<usize>,
}

impl Breadcrumb {
    /// Create a breadcrumb that shows all items.
    pub fn new() -> Self {
        Self { items: Vec::new(), max_visible: 0, focused_index: None }
    }

    /// Set the maximum number of visible items before truncation.
    pub fn max_visible(mut self, n: usize) -> Self {
        self.max_visible = n;
        self
    }

    /// Append an item.
    pub fn item(mut self, item: BreadcrumbItem) -> Self {
        self.items.push(item);
        self
    }

    /// The last item is the "current page".
    pub fn current_index(&self) -> Option<usize> {
        if self.items.is_empty() { None } else { Some(self.items.len() - 1) }
    }

    /// Returns the indices of the items that are currently visible.
    ///
    /// When truncation is active, the first element is `0` (root), followed
    /// by the last `max_visible - 2` indices. The ellipsis lives between
    /// index 0 and the first visible tail index.
    pub fn visible_indices(&self) -> Vec<usize> {
        let n = self.items.len();
        if self.max_visible == 0 || n <= self.max_visible {
            (0..n).collect()
        } else {
            // Always show the first item and the last (max_visible - 1) items.
            let tail_count = (self.max_visible).saturating_sub(1).max(1);
            let tail_start = n.saturating_sub(tail_count);
            let mut v: Vec<usize> = vec![0];
            v.extend(tail_start..n);
            v
        }
    }

    /// Whether the breadcrumb is currently in truncated mode.
    pub fn is_truncated(&self) -> bool {
        self.max_visible > 0 && self.items.len() > self.max_visible
    }

    /// Handle a keyboard event and return what happened.
    pub fn handle_key(
        &mut self,
        event: &wham_core::input::InputEvent,
    ) -> BreadcrumbEvent {
        use wham_core::input::{InputEvent, KeyCode};

        let visible = self.visible_indices();
        if visible.is_empty() {
            return BreadcrumbEvent::None;
        }

        let current_vis_pos =
            self.focused_index.and_then(|fi| visible.iter().position(|&vi| vi == fi));

        match event {
            InputEvent::KeyDown { code, modifiers } => match code {
                KeyCode::Tab | KeyCode::ArrowRight => {
                    let forward = !matches!(code, KeyCode::Tab) || !modifiers.shift;
                    let next_pos = if forward {
                        match current_vis_pos {
                            None => Some(0),
                            Some(i) if i + 1 < visible.len() => Some(i + 1),
                            _ => return BreadcrumbEvent::None,
                        }
                    } else {
                        // Shift+Tab or ArrowLeft
                        match current_vis_pos {
                            None | Some(0) => return BreadcrumbEvent::None,
                            Some(i) => Some(i - 1),
                        }
                    };
                    if let Some(p) = next_pos {
                        let idx = visible[p];
                        self.focused_index = Some(idx);
                        BreadcrumbEvent::FocusMoved { index: idx }
                    } else {
                        BreadcrumbEvent::None
                    }
                }
                KeyCode::ArrowLeft => {
                    let next_pos = match current_vis_pos {
                        None | Some(0) => return BreadcrumbEvent::None,
                        Some(i) => Some(i - 1),
                    };
                    if let Some(p) = next_pos {
                        let idx = visible[p];
                        self.focused_index = Some(idx);
                        BreadcrumbEvent::FocusMoved { index: idx }
                    } else {
                        BreadcrumbEvent::None
                    }
                }
                KeyCode::Enter => {
                    if let Some(fi) = self.focused_index {
                        BreadcrumbEvent::Followed { index: fi }
                    } else {
                        BreadcrumbEvent::None
                    }
                }
                _ => BreadcrumbEvent::None,
            },
            _ => BreadcrumbEvent::None,
        }
    }
}

impl Default for Breadcrumb {
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

    fn make_crumb() -> Breadcrumb {
        Breadcrumb::new()
            .item(BreadcrumbItem::new("Home", "/"))
            .item(BreadcrumbItem::new("Products", "/products"))
            .item(BreadcrumbItem::new("Widgets", "/products/widgets"))
            .item(BreadcrumbItem::new("Super Widget", "/products/widgets/super"))
    }

    #[test]
    fn visible_indices_all_when_no_limit() {
        let b = make_crumb();
        assert_eq!(b.visible_indices(), vec![0, 1, 2, 3]);
    }

    #[test]
    fn truncation_keeps_first_and_tail() {
        let b = make_crumb().max_visible(3);
        // 4 items, max 3 → show [0] + last 2 → [0, 2, 3]
        assert_eq!(b.visible_indices(), vec![0, 2, 3]);
        assert!(b.is_truncated());
    }

    #[test]
    fn no_truncation_when_items_fit() {
        let b = make_crumb().max_visible(10);
        assert!(!b.is_truncated());
    }

    #[test]
    fn current_index_is_last() {
        let b = make_crumb();
        assert_eq!(b.current_index(), Some(3));
    }

    #[test]
    fn tab_focuses_first_item() {
        let mut b = make_crumb();
        let ev = b.handle_key(&key_down(KeyCode::Tab));
        assert_eq!(ev, BreadcrumbEvent::FocusMoved { index: 0 });
    }

    #[test]
    fn arrow_right_advances_focus() {
        let mut b = make_crumb();
        b.focused_index = Some(1);
        let ev = b.handle_key(&key_down(KeyCode::ArrowRight));
        assert_eq!(ev, BreadcrumbEvent::FocusMoved { index: 2 });
    }

    #[test]
    fn arrow_left_retreats_focus() {
        let mut b = make_crumb();
        b.focused_index = Some(2);
        let ev = b.handle_key(&key_down(KeyCode::ArrowLeft));
        assert_eq!(ev, BreadcrumbEvent::FocusMoved { index: 1 });
    }

    #[test]
    fn enter_follows_focused_crumb() {
        let mut b = make_crumb();
        b.focused_index = Some(1);
        let ev = b.handle_key(&key_down(KeyCode::Enter));
        assert_eq!(ev, BreadcrumbEvent::Followed { index: 1 });
    }

    #[test]
    fn truncated_focus_skips_hidden_items() {
        let mut b = make_crumb().max_visible(3);
        // Visible: [0, 2, 3]. Focus should jump from 0 to 2 (not 1).
        b.focused_index = Some(0);
        let ev = b.handle_key(&key_down(KeyCode::ArrowRight));
        assert_eq!(ev, BreadcrumbEvent::FocusMoved { index: 2 });
    }
}
