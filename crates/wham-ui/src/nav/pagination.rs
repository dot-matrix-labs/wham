//! Pagination — previous / next page navigation control.
//!
//! The `Pagination` component displays a series of page buttons and prev/next
//! controls. It handles keyboard navigation and page range calculations.
//!
//! # ARIA role: `navigation` with `aria-label="Pagination"`
//!
//! Keyboard navigation:
//! - `Tab` / `Shift+Tab` move focus through the visible controls.
//! - `ArrowLeft` / `ArrowRight` move focus between controls.
//! - `Enter` or `Space` activates the focused control.
//! - `Home` jumps to page 1; `End` jumps to the last page.

use wham_elements::Button;

/// The result of a single [`Pagination::handle_key`] call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaginationEvent {
    /// No state change.
    None,
    /// The page changed to `page` (1-indexed).
    PageChanged { page: usize },
}

/// Which control is focused in the pagination widget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaginationFocus {
    /// The "previous page" button.
    Prev,
    /// A specific page number button (1-indexed).
    Page(usize),
    /// The "next page" button.
    Next,
}

/// Page navigation control.
///
/// # ARIA role: `navigation` (`aria-label="Pagination"`)
#[derive(Clone, Debug)]
pub struct Pagination {
    /// Currently displayed page (1-indexed).
    pub current_page: usize,
    /// Total number of pages.
    pub total_pages: usize,
    /// Currently focused control, if any.
    pub focus: Option<PaginationFocus>,
    /// Maximum number of page buttons to show. When 0, show all.
    pub max_page_buttons: usize,
    /// Previous-page button.
    pub prev_button: Button,
    /// Next-page button.
    pub next_button: Button,
}

impl Pagination {
    /// Create a pagination control at page 1 of `total_pages`.
    pub fn new(total_pages: usize) -> Self {
        let total_pages = total_pages.max(1);
        Self {
            current_page: 1,
            total_pages,
            focus: None,
            max_page_buttons: 0,
            prev_button: Button::new("Previous page"),
            next_button: Button::new("Next page"),
        }
    }

    /// Start at a specific page.
    pub fn current(mut self, page: usize) -> Self {
        self.current_page = page.clamp(1, self.total_pages);
        self
    }

    /// Limit the number of visible page buttons.
    pub fn max_page_buttons(mut self, n: usize) -> Self {
        self.max_page_buttons = n;
        self
    }

    /// Whether the "previous" button should be enabled.
    pub fn can_go_prev(&self) -> bool {
        self.current_page > 1
    }

    /// Whether the "next" button should be enabled.
    pub fn can_go_next(&self) -> bool {
        self.current_page < self.total_pages
    }

    /// The page numbers that should be rendered as buttons.
    ///
    /// When `max_page_buttons` is 0, returns all pages. Otherwise returns a
    /// window of `max_page_buttons` pages centred around `current_page`.
    pub fn visible_pages(&self) -> Vec<usize> {
        let n = self.total_pages;
        if self.max_page_buttons == 0 || n <= self.max_page_buttons {
            return (1..=n).collect();
        }
        let half = self.max_page_buttons / 2;
        let start = self.current_page.saturating_sub(half).max(1);
        let end = (start + self.max_page_buttons - 1).min(n);
        let start = end.saturating_sub(self.max_page_buttons - 1).max(1);
        (start..=end).collect()
    }

    /// Build an ordered list of all focusable controls.
    fn focusable_controls(&self) -> Vec<PaginationFocus> {
        let mut controls = Vec::new();
        if self.can_go_prev() {
            controls.push(PaginationFocus::Prev);
        }
        for p in self.visible_pages() {
            controls.push(PaginationFocus::Page(p));
        }
        if self.can_go_next() {
            controls.push(PaginationFocus::Next);
        }
        controls
    }

    /// Move focus to a specific page button.
    pub fn focus_page(&mut self, page: usize) {
        self.focus = Some(PaginationFocus::Page(page));
    }

    /// Handle a keyboard event.
    pub fn handle_key(
        &mut self,
        event: &wham_core::input::InputEvent,
    ) -> PaginationEvent {
        use wham_core::input::{InputEvent, KeyCode};

        match event {
            InputEvent::KeyDown { code, modifiers } => {
                let controls = self.focusable_controls();
                let current_pos =
                    self.focus.and_then(|f| controls.iter().position(|c| *c == f));

                match code {
                    KeyCode::Tab | KeyCode::ArrowRight => {
                        let forward =
                            !matches!(code, KeyCode::Tab) || !modifiers.shift;
                        if forward {
                            let next_pos = match current_pos {
                                None => Some(0),
                                Some(i) if i + 1 < controls.len() => Some(i + 1),
                                _ => return PaginationEvent::None,
                            };
                            if let Some(p) = next_pos {
                                self.focus = Some(controls[p]);
                            }
                        } else {
                            let next_pos = match current_pos {
                                None | Some(0) => return PaginationEvent::None,
                                Some(i) => Some(i - 1),
                            };
                            if let Some(p) = next_pos {
                                self.focus = Some(controls[p]);
                            }
                        }
                        PaginationEvent::None
                    }
                    KeyCode::ArrowLeft => {
                        let next_pos = match current_pos {
                            None | Some(0) => return PaginationEvent::None,
                            Some(i) => Some(i - 1),
                        };
                        if let Some(p) = next_pos {
                            self.focus = Some(controls[p]);
                        }
                        PaginationEvent::None
                    }
                    KeyCode::Home => {
                        self.current_page = 1;
                        self.focus = Some(PaginationFocus::Page(1));
                        PaginationEvent::PageChanged { page: 1 }
                    }
                    KeyCode::End => {
                        self.current_page = self.total_pages;
                        self.focus = Some(PaginationFocus::Page(self.total_pages));
                        PaginationEvent::PageChanged { page: self.total_pages }
                    }
                    KeyCode::Enter | KeyCode::Other(_) => {
                        let is_space = matches!(code, KeyCode::Other(s) if s == " ");
                        if !matches!(code, KeyCode::Enter) && !is_space {
                            return PaginationEvent::None;
                        }
                        match self.focus {
                            Some(PaginationFocus::Prev) => {
                                if self.can_go_prev() {
                                    self.current_page -= 1;
                                    PaginationEvent::PageChanged { page: self.current_page }
                                } else {
                                    PaginationEvent::None
                                }
                            }
                            Some(PaginationFocus::Next) => {
                                if self.can_go_next() {
                                    self.current_page += 1;
                                    PaginationEvent::PageChanged { page: self.current_page }
                                } else {
                                    PaginationEvent::None
                                }
                            }
                            Some(PaginationFocus::Page(p)) => {
                                self.current_page = p;
                                PaginationEvent::PageChanged { page: p }
                            }
                            None => PaginationEvent::None,
                        }
                    }
                    _ => PaginationEvent::None,
                }
            }
            _ => PaginationEvent::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wham_core::input::{InputEvent, KeyCode, Modifiers};

    fn key_down(code: KeyCode) -> InputEvent {
        InputEvent::KeyDown { code, modifiers: Modifiers::default() }
    }

    fn make_pagination() -> Pagination {
        Pagination::new(10).current(3)
    }

    #[test]
    fn visible_pages_all_when_no_limit() {
        let p = Pagination::new(5);
        assert_eq!(p.visible_pages(), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn visible_pages_windowed() {
        let p = Pagination::new(10).current(5).max_page_buttons(5);
        let vp = p.visible_pages();
        assert_eq!(vp.len(), 5);
        assert!(vp.contains(&5));
    }

    #[test]
    fn can_go_prev_and_next() {
        let p = make_pagination();
        assert!(p.can_go_prev());
        assert!(p.can_go_next());
    }

    #[test]
    fn cannot_go_prev_on_first_page() {
        let p = Pagination::new(5).current(1);
        assert!(!p.can_go_prev());
    }

    #[test]
    fn cannot_go_next_on_last_page() {
        let p = Pagination::new(5).current(5);
        assert!(!p.can_go_next());
    }

    #[test]
    fn enter_on_prev_decrements_page() {
        let mut p = make_pagination(); // page 3
        p.focus = Some(PaginationFocus::Prev);
        let ev = p.handle_key(&key_down(KeyCode::Enter));
        assert_eq!(ev, PaginationEvent::PageChanged { page: 2 });
        assert_eq!(p.current_page, 2);
    }

    #[test]
    fn enter_on_next_increments_page() {
        let mut p = make_pagination();
        p.focus = Some(PaginationFocus::Next);
        let ev = p.handle_key(&key_down(KeyCode::Enter));
        assert_eq!(ev, PaginationEvent::PageChanged { page: 4 });
    }

    #[test]
    fn enter_on_page_button_jumps_to_page() {
        let mut p = make_pagination();
        p.focus = Some(PaginationFocus::Page(7));
        let ev = p.handle_key(&key_down(KeyCode::Enter));
        assert_eq!(ev, PaginationEvent::PageChanged { page: 7 });
        assert_eq!(p.current_page, 7);
    }

    #[test]
    fn home_jumps_to_page_one() {
        let mut p = make_pagination();
        let ev = p.handle_key(&key_down(KeyCode::Home));
        assert_eq!(ev, PaginationEvent::PageChanged { page: 1 });
        assert_eq!(p.current_page, 1);
    }

    #[test]
    fn end_jumps_to_last_page() {
        let mut p = make_pagination();
        let ev = p.handle_key(&key_down(KeyCode::End));
        assert_eq!(ev, PaginationEvent::PageChanged { page: 10 });
    }

    #[test]
    fn arrow_right_moves_focus_forward() {
        let mut p = Pagination::new(5).current(3);
        p.focus = Some(PaginationFocus::Prev);
        p.handle_key(&key_down(KeyCode::ArrowRight));
        assert_eq!(p.focus, Some(PaginationFocus::Page(1)));
    }
}
