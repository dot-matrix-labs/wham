use unicode_segmentation::UnicodeSegmentation;

/// A text cursor position expressed as a grapheme cluster index (not a byte offset).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Caret {
    /// Grapheme cluster index (0 = before the first character).
    pub index: usize,
}

/// A half-open text selection expressed in grapheme cluster indices.
///
/// `start` and `end` may be in either order (a reversed selection is common when the
/// user drags leftward). Use [`Selection::normalized`] before doing range arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    /// The anchor end of the selection (where the drag or Shift key-press began).
    pub start: usize,
    /// The active (focus) end of the selection (where the caret currently sits).
    pub end: usize,
}

impl Selection {
    /// Returns a new `Selection` with `start <= end`, swapping the endpoints if needed.
    pub fn normalized(&self) -> Selection {
        if self.start <= self.end {
            *self
        } else {
            Selection {
                start: self.end,
                end: self.start,
            }
        }
    }

    /// Returns `true` if `start == end` (zero-length selection).
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// A reversible text-editing operation, used by [`TextBuffer`] for undo/redo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextEditOp {
    /// Characters were inserted at `at` (grapheme index).
    Insert { at: usize, text: String },
    /// Characters in `[start, end)` (grapheme indices) were deleted.
    Delete { start: usize, end: usize, text: String },
}

/// A grapheme-aware text buffer with caret, selection, IME composition, and undo/redo.
///
/// All positions are expressed in **grapheme cluster** indices, not byte offsets.
/// This means a single emoji family sequence counts as one position, matching
/// what users see on screen.
///
/// # Example
///
/// ```
/// use ui_core::text::TextBuffer;
///
/// let mut buf = TextBuffer::new("hello");
/// buf.move_to_line_end(false);
/// buf.insert_text(" world");
/// assert_eq!(buf.text(), "hello world");
/// buf.undo();
/// assert_eq!(buf.text(), "hello");
/// ```
#[derive(Clone, Debug)]
pub struct TextBuffer {
    text: String,
    caret: Caret,
    selection: Option<Selection>,
    composition: Option<Selection>,
    undo_stack: Vec<TextEditOp>,
    redo_stack: Vec<TextEditOp>,
}

impl TextBuffer {
    /// Creates a new `TextBuffer` pre-populated with `text`.
    /// The caret is placed at the end of the text.
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let caret = Caret {
            index: UnicodeSegmentation::graphemes(text.as_str(), true).count(),
        };
        Self {
            text,
            caret,
            selection: None,
            composition: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Returns the current text content.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the current caret position.
    pub fn caret(&self) -> Caret {
        self.caret
    }

    /// Returns the current selection, or `None` if there is no active selection.
    pub fn selection(&self) -> Option<Selection> {
        self.selection
    }

    /// Returns the active IME composition range, or `None` if no composition is in progress.
    pub fn composition(&self) -> Option<Selection> {
        self.composition
    }

    /// Replaces all text content and resets caret, selection, composition, and undo history.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        let len = self.grapheme_len();
        self.caret.index = len;
        self.selection = None;
        self.composition = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Returns the number of grapheme clusters in the buffer.
    pub fn grapheme_len(&self) -> usize {
        UnicodeSegmentation::graphemes(self.text.as_str(), true).count()
    }

    /// Selects all text and moves the caret to the end.
    pub fn select_all(&mut self) {
        let len = self.grapheme_len();
        self.selection = Some(Selection { start: 0, end: len });
        self.caret.index = len;
    }

    /// Moves the caret to `index` (clamped to the buffer length) and clears the selection.
    pub fn set_caret(&mut self, index: usize) {
        let len = self.grapheme_len();
        self.caret.index = index.min(len);
        self.selection = None;
    }

    /// Sets the selection to `[start, end]` (both clamped) and places the caret at `end`.
    pub fn set_selection(&mut self, start: usize, end: usize) {
        let len = self.grapheme_len();
        let s = start.min(len);
        let e = end.min(len);
        self.selection = Some(Selection { start: s, end: e });
        self.caret.index = e;
    }

    /// Inserts `text` at the caret (replacing any current selection).
    ///
    /// Returns the [`TextEditOp`] that was pushed onto the undo stack.
    pub fn insert_text(&mut self, text: &str) -> Option<TextEditOp> {
        let _ = self.delete_selection();
        let at = self.caret.index;
        let byte_index = self.byte_index_from_grapheme(at);
        self.text.insert_str(byte_index, text);
        let graphemes_inserted = UnicodeSegmentation::graphemes(text, true).count();
        self.caret.index += graphemes_inserted;
        let op = TextEditOp::Insert {
            at,
            text: text.to_string(),
        };
        self.push_undo(op.clone());
        Some(op)
    }

    /// Deletes the grapheme immediately before the caret (Backspace).
    ///
    /// If there is an active selection, deletes the selection instead. Returns `None`
    /// if there is nothing to delete.
    pub fn delete_backward(&mut self) -> Option<TextEditOp> {
        if self.delete_selection().is_some() {
            return None;
        }
        if self.caret.index == 0 {
            return None;
        }
        let start = self.caret.index - 1;
        let end = self.caret.index;
        self.delete_range(start, end)
    }

    /// Deletes the grapheme immediately after the caret (Delete key).
    ///
    /// If there is an active selection, deletes the selection instead. Returns `None`
    /// if there is nothing to delete.
    pub fn delete_forward(&mut self) -> Option<TextEditOp> {
        if self.delete_selection().is_some() {
            return None;
        }
        let len = self.grapheme_len();
        if self.caret.index >= len {
            return None;
        }
        let start = self.caret.index;
        let end = self.caret.index + 1;
        self.delete_range(start, end)
    }

    /// Moves the caret one grapheme to the left.
    /// If `extend_selection` is `true` the selection is extended rather than collapsed.
    pub fn move_left(&mut self, extend_selection: bool) {
        if self.caret.index == 0 {
            return;
        }
        let new_index = self.caret.index - 1;
        self.move_to(new_index, extend_selection);
    }

    /// Moves the caret one grapheme to the right.
    /// If `extend_selection` is `true` the selection is extended rather than collapsed.
    pub fn move_right(&mut self, extend_selection: bool) {
        let len = self.grapheme_len();
        if self.caret.index >= len {
            return;
        }
        let new_index = self.caret.index + 1;
        self.move_to(new_index, extend_selection);
    }

    /// Moves the caret to `index` (clamped).
    /// If `extend_selection` is `true` the selection is extended rather than collapsed.
    pub fn move_to(&mut self, index: usize, extend_selection: bool) {
        let len = self.grapheme_len();
        let clamped = index.min(len);
        if extend_selection {
            let start = self.selection.map(|s| s.start).unwrap_or(self.caret.index);
            self.selection = Some(Selection {
                start,
                end: clamped,
            });
        } else {
            self.selection = None;
        }
        self.caret.index = clamped;
    }

    /// Move the caret to the start of its current logical line (Home key).
    pub fn move_to_line_start(&mut self, extend_selection: bool) {
        let target = self.line_start_before(self.caret.index);
        self.move_to(target, extend_selection);
    }

    /// Move the caret to the end of its current logical line (End key).
    pub fn move_to_line_end(&mut self, extend_selection: bool) {
        let target = self.line_end_after(self.caret.index);
        self.move_to(target, extend_selection);
    }

    /// Begins an IME composition sequence at the current caret position.
    /// Call this in response to a `compositionstart` event.
    pub fn begin_composition(&mut self) {
        self.composition = Some(Selection {
            start: self.caret.index,
            end: self.caret.index,
        });
    }

    /// Updates the in-progress composition with the current candidate `text`.
    /// Replaces the previous composition range in-place.
    pub fn update_composition(&mut self, text: &str) {
        let range = self.composition.unwrap_or(Selection {
            start: self.caret.index,
            end: self.caret.index,
        });
        self.replace_range(range.start, range.end, text);
        let new_end = range.start + UnicodeSegmentation::graphemes(text, true).count();
        self.composition = Some(Selection {
            start: range.start,
            end: new_end,
        });
        self.caret.index = new_end;
    }

    /// Finalises the IME composition, committing `text` as the confirmed input.
    pub fn end_composition(&mut self, text: &str) {
        if let Some(range) = self.composition {
            self.replace_range(range.start, range.end, text);
        } else {
            self.insert_text(text);
        }
        let new_end = self.caret.index;
        self.composition = None;
        self.selection = None;
        self.caret.index = new_end;
    }

    /// Undoes the last edit operation. Returns `true` if an operation was undone.
    pub fn undo(&mut self) -> bool {
        let op = match self.undo_stack.pop() {
            Some(op) => op,
            None => return false,
        };
        let inverse = op.invert();
        self.apply_op(op.clone());
        self.redo_stack.push(inverse);
        true
    }

    /// Redoes the last undone operation. Returns `true` if an operation was redone.
    pub fn redo(&mut self) -> bool {
        let op = match self.redo_stack.pop() {
            Some(op) => op,
            None => return false,
        };
        let inverse = op.invert();
        self.apply_op(op.clone());
        self.undo_stack.push(inverse);
        true
    }

    /// Returns the currently selected text, or `None` if there is no active selection.
    pub fn selected_text(&self) -> Option<String> {
        let sel = self.selection?;
        if sel.is_empty() {
            return None;
        }
        let sel = sel.normalized();
        let start = self.byte_index_from_grapheme(sel.start);
        let end = self.byte_index_from_grapheme(sel.end);
        Some(self.text[start..end].to_string())
    }

    /// Removes the selected text and returns it, suitable for cut operations.
    /// Returns `None` if there is no non-empty selection.
    pub fn cut_selection(&mut self) -> Option<String> {
        let text = self.selected_text()?;
        self.delete_selection();
        Some(text)
    }

    // -----------------------------------------------------------------------
    // Word navigation
    // -----------------------------------------------------------------------

    /// Move the caret one word to the left (Ctrl+Left / Option+Left).
    /// If `extend_selection` is true the selection is extended rather than
    /// collapsed.
    pub fn move_word_left(&mut self, extend_selection: bool) {
        let target = self.word_start_before(self.caret.index);
        self.move_to(target, extend_selection);
    }

    /// Move the caret one word to the right (Ctrl+Right / Option+Right).
    pub fn move_word_right(&mut self, extend_selection: bool) {
        let target = self.word_end_after(self.caret.index);
        self.move_to(target, extend_selection);
    }

    /// Delete the word (and any whitespace) immediately to the left of the
    /// caret (Ctrl+Backspace / Alt+Backspace).
    pub fn delete_word_backward(&mut self) -> Option<TextEditOp> {
        if self.delete_selection().is_some() {
            return None;
        }
        if self.caret.index == 0 {
            return None;
        }
        let start = self.word_start_before(self.caret.index);
        self.delete_range(start, self.caret.index)
    }

    /// Delete the word (and any whitespace) immediately to the right of the
    /// caret (Ctrl+Delete / Alt+Delete).
    pub fn delete_word_forward(&mut self) -> Option<TextEditOp> {
        if self.delete_selection().is_some() {
            return None;
        }
        let len = self.grapheme_len();
        if self.caret.index >= len {
            return None;
        }
        let end = self.word_end_after(self.caret.index);
        self.delete_range(self.caret.index, end)
    }

    // -----------------------------------------------------------------------
    // Click helpers
    // -----------------------------------------------------------------------

    /// Select the word that contains (or is adjacent to) `grapheme_index`.
    /// Used for double-click word selection.
    pub fn select_word_at(&mut self, grapheme_index: usize) {
        let len = self.grapheme_len();
        let idx = grapheme_index.min(len);
        let start = self.word_start_before(idx);
        let end   = self.word_end_after(idx);
        self.selection = Some(Selection { start, end });
        self.caret.index = end;
    }

    /// Select the entire logical line that contains `grapheme_index`.
    /// Used for triple-click line selection.
    pub fn select_line_at(&mut self, grapheme_index: usize) {
        let len = self.grapheme_len();
        let idx = grapheme_index.min(len);
        // Walk backward to the previous newline (or BOF)
        let start = self.line_start_before(idx);
        // Walk forward to the next newline (exclusive) or EOF
        let end   = self.line_end_after(idx);
        self.selection = Some(Selection { start, end });
        self.caret.index = end;
    }

    // -----------------------------------------------------------------------
    // Word / line boundary internals
    // -----------------------------------------------------------------------

    /// Returns the grapheme index of the beginning of the word to the left of
    /// `from`.  Skips leading whitespace first (matches platform behaviour on
    /// macOS / Windows).
    fn word_start_before(&self, from: usize) -> usize {
        if from == 0 {
            return 0;
        }
        // Collect graphemes with their byte offsets so we can locate the
        // Unicode word boundaries.
        let graphemes: Vec<(usize, &str)> =
            self.text.grapheme_indices(true).collect();
        // Convert grapheme index → byte offset
        let byte_at = |gi: usize| -> usize {
            graphemes.get(gi).map(|(b, _)| *b).unwrap_or(self.text.len())
        };
        // We scan leftward through word-boundary tokens produced by
        // `unicode_word_indices`.  Each token is (byte_start, word_str).
        let target_byte = byte_at(from);
        let mut result_byte = 0usize;
        let mut prev_end = 0usize;
        for (wb_start, wb_str) in self.text.split_word_bound_indices() {
            let wb_end = wb_start + wb_str.len();
            if wb_end <= target_byte {
                // This whole token lies before the caret – remember it as a
                // candidate (we want the last one that is a word, not space).
                if !wb_str.chars().all(|c| c.is_whitespace()) {
                    result_byte = wb_start;
                } else if prev_end == wb_start {
                    // Leading whitespace immediately before us – skip it by
                    // not updating result_byte.
                }
            }
            prev_end = wb_end;
        }
        // Convert result_byte back to a grapheme index
        graphemes
            .iter()
            .position(|(b, _)| *b == result_byte)
            .unwrap_or(0)
    }

    /// Returns the grapheme index just past the end of the word to the right
    /// of `from`.  Skips leading whitespace first.
    fn word_end_after(&self, from: usize) -> usize {
        let len = self.grapheme_len();
        if from >= len {
            return len;
        }
        let graphemes: Vec<(usize, &str)> =
            self.text.grapheme_indices(true).collect();
        let byte_at = |gi: usize| -> usize {
            graphemes.get(gi).map(|(b, _)| *b).unwrap_or(self.text.len())
        };
        let target_byte = byte_at(from);
        let mut found = false;
        for (wb_start, wb_str) in self.text.split_word_bound_indices() {
            if wb_start < target_byte {
                continue;
            }
            // First token that starts at or after the caret.
            // Skip a purely-whitespace token (moves to the next word).
            if !found && wb_str.chars().all(|c| c.is_whitespace()) {
                found = true; // skip this one
                continue;
            }
            let wb_end_byte = wb_start + wb_str.len();
            // Convert wb_end_byte to a grapheme index
            return graphemes
                .iter()
                .position(|(b, _)| *b >= wb_end_byte)
                .unwrap_or(len);
        }
        len
    }

    /// Grapheme index of the start of the line containing `from`.
    fn line_start_before(&self, from: usize) -> usize {
        if from == 0 {
            return 0;
        }
        let graphemes: Vec<(usize, &str)> =
            self.text.grapheme_indices(true).collect();
        let mut result = 0usize;
        for (gi, (_byte, g)) in graphemes.iter().enumerate() {
            if gi >= from {
                break;
            }
            if *g == "\n" {
                result = gi + 1;
            }
        }
        result
    }

    /// Grapheme index just past the end of the line containing `from` (before
    /// the newline or at EOF).
    fn line_end_after(&self, from: usize) -> usize {
        let graphemes: Vec<(usize, &str)> =
            self.text.grapheme_indices(true).collect();
        let len = graphemes.len();
        for (gi, (_byte, g)) in graphemes.iter().enumerate() {
            if gi < from {
                continue;
            }
            if *g == "\n" {
                return gi; // stop before the newline itself
            }
        }
        len
    }

    fn delete_selection(&mut self) -> Option<TextEditOp> {
        let sel = self.selection?;
        if sel.is_empty() {
            self.selection = None;
            return None;
        }
        let sel = sel.normalized();
        self.delete_range(sel.start, sel.end)
    }

    fn delete_range(&mut self, start: usize, end: usize) -> Option<TextEditOp> {
        let start_b = self.byte_index_from_grapheme(start);
        let end_b = self.byte_index_from_grapheme(end);
        let removed = self.text[start_b..end_b].to_string();
        self.text.replace_range(start_b..end_b, "");
        self.caret.index = start;
        self.selection = None;
        let op = TextEditOp::Delete {
            start,
            end,
            text: removed,
        };
        self.push_undo(op.clone());
        Some(op)
    }

    fn replace_range(&mut self, start: usize, end: usize, text: &str) {
        let start_b = self.byte_index_from_grapheme(start);
        let end_b = self.byte_index_from_grapheme(end);
        self.text.replace_range(start_b..end_b, text);
        let graphemes_inserted = UnicodeSegmentation::graphemes(text, true).count();
        self.caret.index = start + graphemes_inserted;
        self.selection = None;
    }

    fn byte_index_from_grapheme(&self, grapheme_index: usize) -> usize {
        if grapheme_index == 0 {
            return 0;
        }
        for (count, (byte_index, _)) in self.text.grapheme_indices(true).enumerate() {
            if count == grapheme_index {
                return byte_index;
            }
        }
        self.text.len()
    }

    fn push_undo(&mut self, op: TextEditOp) {
        self.undo_stack.push(op.invert());
        self.redo_stack.clear();
    }

    fn apply_op(&mut self, op: TextEditOp) {
        match op {
            TextEditOp::Insert { at, text } => {
                let byte_index = self.byte_index_from_grapheme(at);
                self.text.insert_str(byte_index, &text);
                let graphemes_inserted = UnicodeSegmentation::graphemes(text.as_str(), true).count();
                self.caret.index = at + graphemes_inserted;
            }
            TextEditOp::Delete { start, end, .. } => {
                let start_b = self.byte_index_from_grapheme(start);
                let end_b = self.byte_index_from_grapheme(end);
                self.text.replace_range(start_b..end_b, "");
                self.caret.index = start;
            }
        }
        self.selection = None;
    }
}

impl TextEditOp {
    pub fn invert(&self) -> TextEditOp {
        match self {
            TextEditOp::Insert { at, text } => {
                let end = at + UnicodeSegmentation::graphemes(text.as_str(), true).count();
                TextEditOp::Delete {
                    start: *at,
                    end,
                    text: text.clone(),
                }
            }
            TextEditOp::Delete { start, end: _end, text } => TextEditOp::Insert {
                at: *start,
                text: text.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Basic construction
    // -----------------------------------------------------------------------

    #[test]
    fn new_empty_buffer() {
        let buf = TextBuffer::new("");
        assert_eq!(buf.text(), "");
        assert_eq!(buf.caret().index, 0);
        assert!(buf.selection().is_none());
        assert!(buf.composition().is_none());
    }

    #[test]
    fn new_buffer_caret_at_end() {
        let buf = TextBuffer::new("hello");
        assert_eq!(buf.caret().index, 5);
    }

    #[test]
    fn new_buffer_with_multibyte() {
        // e-acute is 2 bytes but 1 grapheme
        let buf = TextBuffer::new("caf\u{00e9}");
        assert_eq!(buf.grapheme_len(), 4);
        assert_eq!(buf.caret().index, 4);
    }

    #[test]
    fn new_buffer_with_emoji() {
        // Family emoji is a single grapheme cluster
        let buf = TextBuffer::new("a\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}b");
        assert_eq!(buf.grapheme_len(), 3);
    }

    // -----------------------------------------------------------------------
    // Insert
    // -----------------------------------------------------------------------

    #[test]
    fn insert_at_end() {
        let mut buf = TextBuffer::new("hello");
        buf.insert_text(" world");
        assert_eq!(buf.text(), "hello world");
        assert_eq!(buf.caret().index, 11);
    }

    #[test]
    fn insert_at_beginning() {
        let mut buf = TextBuffer::new("world");
        buf.set_caret(0);
        buf.insert_text("hello ");
        assert_eq!(buf.text(), "hello world");
        assert_eq!(buf.caret().index, 6);
    }

    #[test]
    fn insert_in_middle() {
        let mut buf = TextBuffer::new("helo");
        buf.set_caret(2);
        buf.insert_text("l");
        assert_eq!(buf.text(), "hello");
        assert_eq!(buf.caret().index, 3);
    }

    #[test]
    fn insert_empty_string() {
        let mut buf = TextBuffer::new("abc");
        buf.insert_text("");
        assert_eq!(buf.text(), "abc");
    }

    #[test]
    fn insert_replaces_selection() {
        let mut buf = TextBuffer::new("hello world");
        buf.set_selection(0, 5);
        buf.insert_text("goodbye");
        assert_eq!(buf.text(), "goodbye world");
    }

    // -----------------------------------------------------------------------
    // Delete backward
    // -----------------------------------------------------------------------

    #[test]
    fn delete_backward_basic() {
        let mut buf = TextBuffer::new("abc");
        buf.delete_backward();
        assert_eq!(buf.text(), "ab");
        assert_eq!(buf.caret().index, 2);
    }

    #[test]
    fn delete_backward_at_start() {
        let mut buf = TextBuffer::new("abc");
        buf.set_caret(0);
        let op = buf.delete_backward();
        assert!(op.is_none());
        assert_eq!(buf.text(), "abc");
    }

    #[test]
    fn delete_backward_empty_buffer() {
        let mut buf = TextBuffer::new("");
        let op = buf.delete_backward();
        assert!(op.is_none());
        assert_eq!(buf.text(), "");
    }

    #[test]
    fn delete_backward_with_selection_deletes_selection() {
        let mut buf = TextBuffer::new("hello world");
        buf.set_selection(5, 11);
        buf.delete_backward();
        assert_eq!(buf.text(), "hello");
    }

    // -----------------------------------------------------------------------
    // Delete forward
    // -----------------------------------------------------------------------

    #[test]
    fn delete_forward_basic() {
        let mut buf = TextBuffer::new("abc");
        buf.set_caret(0);
        buf.delete_forward();
        assert_eq!(buf.text(), "bc");
        assert_eq!(buf.caret().index, 0);
    }

    #[test]
    fn delete_forward_at_end() {
        let mut buf = TextBuffer::new("abc");
        let op = buf.delete_forward();
        assert!(op.is_none());
        assert_eq!(buf.text(), "abc");
    }

    // -----------------------------------------------------------------------
    // Cursor movement
    // -----------------------------------------------------------------------

    #[test]
    fn move_left() {
        let mut buf = TextBuffer::new("abc");
        buf.move_left(false);
        assert_eq!(buf.caret().index, 2);
        assert!(buf.selection().is_none());
    }

    #[test]
    fn move_left_at_start() {
        let mut buf = TextBuffer::new("abc");
        buf.set_caret(0);
        buf.move_left(false);
        assert_eq!(buf.caret().index, 0);
    }

    #[test]
    fn move_right() {
        let mut buf = TextBuffer::new("abc");
        buf.set_caret(0);
        buf.move_right(false);
        assert_eq!(buf.caret().index, 1);
    }

    #[test]
    fn move_right_at_end() {
        let mut buf = TextBuffer::new("abc");
        buf.move_right(false);
        assert_eq!(buf.caret().index, 3);
    }

    #[test]
    fn move_left_with_selection_extends() {
        let mut buf = TextBuffer::new("abcdef");
        buf.set_caret(3);
        buf.move_left(true);
        let sel = buf.selection().unwrap();
        assert_eq!(sel.start, 3);
        assert_eq!(sel.end, 2);
    }

    #[test]
    fn move_right_with_selection_extends() {
        let mut buf = TextBuffer::new("abcdef");
        buf.set_caret(3);
        buf.move_right(true);
        let sel = buf.selection().unwrap();
        assert_eq!(sel.start, 3);
        assert_eq!(sel.end, 4);
    }

    // -----------------------------------------------------------------------
    // Home / End (line start/end)
    // -----------------------------------------------------------------------

    #[test]
    fn move_to_line_start_single_line() {
        let mut buf = TextBuffer::new("hello");
        buf.move_to_line_start(false);
        assert_eq!(buf.caret().index, 0);
    }

    #[test]
    fn move_to_line_end_single_line() {
        let mut buf = TextBuffer::new("hello");
        buf.set_caret(0);
        buf.move_to_line_end(false);
        assert_eq!(buf.caret().index, 5);
    }

    #[test]
    fn move_to_line_start_multiline() {
        let mut buf = TextBuffer::new("abc\ndef\nghi");
        // Caret is at end = grapheme 11. Line start of "ghi" is grapheme 8.
        buf.move_to_line_start(false);
        assert_eq!(buf.caret().index, 8);
    }

    #[test]
    fn move_to_line_end_multiline() {
        let mut buf = TextBuffer::new("abc\ndef\nghi");
        buf.set_caret(4); // 'd' in "def"
        buf.move_to_line_end(false);
        assert_eq!(buf.caret().index, 7); // just before '\n' after "def"
    }

    // -----------------------------------------------------------------------
    // Word movement
    // -----------------------------------------------------------------------

    #[test]
    fn move_word_right_basic() {
        let mut buf = TextBuffer::new("hello world");
        buf.set_caret(0);
        buf.move_word_right(false);
        // Should jump past "hello" and the space
        assert!(buf.caret().index >= 5);
    }

    #[test]
    fn move_word_left_basic() {
        let mut buf = TextBuffer::new("hello world");
        // caret at end (11)
        buf.move_word_left(false);
        // Should jump to start of "world" (6)
        assert!(buf.caret().index <= 6);
    }

    // -----------------------------------------------------------------------
    // Select all
    // -----------------------------------------------------------------------

    #[test]
    fn select_all() {
        let mut buf = TextBuffer::new("hello");
        buf.select_all();
        let sel = buf.selection().unwrap();
        assert_eq!(sel.start, 0);
        assert_eq!(sel.end, 5);
        assert_eq!(buf.caret().index, 5);
    }

    #[test]
    fn select_all_empty() {
        let mut buf = TextBuffer::new("");
        buf.select_all();
        let sel = buf.selection().unwrap();
        assert_eq!(sel.start, 0);
        assert_eq!(sel.end, 0);
    }

    // -----------------------------------------------------------------------
    // Selected text / cut
    // -----------------------------------------------------------------------

    #[test]
    fn selected_text_returns_substring() {
        let mut buf = TextBuffer::new("hello world");
        buf.set_selection(6, 11);
        assert_eq!(buf.selected_text().unwrap(), "world");
    }

    #[test]
    fn selected_text_none_without_selection() {
        let buf = TextBuffer::new("hello");
        assert!(buf.selected_text().is_none());
    }

    #[test]
    fn cut_selection_removes_text() {
        let mut buf = TextBuffer::new("hello world");
        buf.set_selection(5, 11);
        let cut = buf.cut_selection().unwrap();
        assert_eq!(cut, " world");
        assert_eq!(buf.text(), "hello");
    }

    // -----------------------------------------------------------------------
    // set_text resets everything
    // -----------------------------------------------------------------------

    #[test]
    fn set_text_resets_state() {
        let mut buf = TextBuffer::new("old");
        buf.insert_text("x"); // creates undo entry
        buf.set_text("brand new");
        assert_eq!(buf.text(), "brand new");
        assert_eq!(buf.caret().index, 9);
        assert!(buf.selection().is_none());
        // undo stack should be cleared
        assert!(!buf.undo());
    }

    // -----------------------------------------------------------------------
    // Undo / Redo
    // -----------------------------------------------------------------------

    #[test]
    fn undo_reverses_insert() {
        let mut buf = TextBuffer::new("");
        buf.insert_text("hello");
        assert_eq!(buf.text(), "hello");
        buf.undo();
        assert_eq!(buf.text(), "");
    }

    #[test]
    fn redo_restores_insert() {
        let mut buf = TextBuffer::new("");
        buf.insert_text("hello");
        buf.undo();
        buf.redo();
        assert_eq!(buf.text(), "hello");
    }

    #[test]
    fn undo_reverses_delete() {
        let mut buf = TextBuffer::new("hello");
        buf.delete_backward();
        assert_eq!(buf.text(), "hell");
        buf.undo();
        assert_eq!(buf.text(), "hello");
    }

    #[test]
    fn new_edit_clears_redo_stack() {
        let mut buf = TextBuffer::new("");
        buf.insert_text("a");
        buf.insert_text("b");
        buf.undo(); // undo "b"
        assert!(buf.redo()); // can redo
        buf.undo(); // undo "b" again
        buf.insert_text("c"); // new edit should clear redo
        assert!(!buf.redo()); // redo no longer available
        assert_eq!(buf.text(), "ac");
    }

    #[test]
    fn multiple_undo_redo() {
        let mut buf = TextBuffer::new("");
        buf.insert_text("a");
        buf.insert_text("b");
        buf.insert_text("c");
        assert_eq!(buf.text(), "abc");

        buf.undo();
        assert_eq!(buf.text(), "ab");
        buf.undo();
        assert_eq!(buf.text(), "a");
        buf.undo();
        assert_eq!(buf.text(), "");
        // Can't undo further
        assert!(!buf.undo());

        buf.redo();
        assert_eq!(buf.text(), "a");
        buf.redo();
        assert_eq!(buf.text(), "ab");
        buf.redo();
        assert_eq!(buf.text(), "abc");
        assert!(!buf.redo());
    }

    #[test]
    fn undo_on_empty_returns_false() {
        let mut buf = TextBuffer::new("hello");
        assert!(!buf.undo());
    }

    #[test]
    fn redo_on_empty_returns_false() {
        let mut buf = TextBuffer::new("hello");
        assert!(!buf.redo());
    }

    // -----------------------------------------------------------------------
    // IME composition
    // -----------------------------------------------------------------------

    #[test]
    fn composition_basic_flow() {
        let mut buf = TextBuffer::new("hello ");
        buf.begin_composition();
        assert!(buf.composition().is_some());

        buf.update_composition("ni");
        assert_eq!(buf.text(), "hello ni");

        buf.update_composition("nihao");
        assert_eq!(buf.text(), "hello nihao");

        buf.end_composition("\u{4f60}\u{597d}");
        assert_eq!(buf.text(), "hello \u{4f60}\u{597d}");
        assert!(buf.composition().is_none());
    }

    #[test]
    fn composition_replaces_previous_update() {
        let mut buf = TextBuffer::new("");
        buf.begin_composition();
        buf.update_composition("ab");
        assert_eq!(buf.text(), "ab");
        buf.update_composition("abc");
        assert_eq!(buf.text(), "abc");
        buf.end_composition("final");
        assert_eq!(buf.text(), "final");
    }

    // -----------------------------------------------------------------------
    // Word / line selection (double/triple click helpers)
    // -----------------------------------------------------------------------

    #[test]
    fn select_word_at_middle() {
        let mut buf = TextBuffer::new("hello world");
        buf.select_word_at(7); // inside "world"
        let sel = buf.selection().unwrap().normalized();
        assert!(sel.start <= 6);
        assert!(sel.end >= 11);
    }

    #[test]
    fn select_line_at_first_line() {
        let mut buf = TextBuffer::new("first\nsecond");
        buf.select_line_at(2); // inside "first"
        let sel = buf.selection().unwrap().normalized();
        assert_eq!(sel.start, 0);
        assert_eq!(sel.end, 5); // before the '\n'
    }

    #[test]
    fn select_line_at_second_line() {
        let mut buf = TextBuffer::new("first\nsecond");
        buf.select_line_at(8); // inside "second"
        let sel = buf.selection().unwrap().normalized();
        assert_eq!(sel.start, 6);
        assert_eq!(sel.end, 12);
    }

    // -----------------------------------------------------------------------
    // Selection normalization
    // -----------------------------------------------------------------------

    #[test]
    fn selection_normalized_already_ordered() {
        let sel = Selection { start: 2, end: 5 };
        let n = sel.normalized();
        assert_eq!(n.start, 2);
        assert_eq!(n.end, 5);
    }

    #[test]
    fn selection_normalized_reversed() {
        let sel = Selection { start: 5, end: 2 };
        let n = sel.normalized();
        assert_eq!(n.start, 2);
        assert_eq!(n.end, 5);
    }

    #[test]
    fn selection_is_empty() {
        assert!(Selection { start: 3, end: 3 }.is_empty());
        assert!(!Selection { start: 3, end: 5 }.is_empty());
    }

    // -----------------------------------------------------------------------
    // TextEditOp::invert
    // -----------------------------------------------------------------------

    #[test]
    fn invert_insert_produces_delete() {
        let op = TextEditOp::Insert { at: 0, text: "hi".into() };
        match op.invert() {
            TextEditOp::Delete { start, end, text } => {
                assert_eq!(start, 0);
                assert_eq!(end, 2);
                assert_eq!(text, "hi");
            }
            _ => panic!("expected Delete"),
        }
    }

    #[test]
    fn invert_delete_produces_insert() {
        let op = TextEditOp::Delete { start: 1, end: 3, text: "ab".into() };
        match op.invert() {
            TextEditOp::Insert { at, text } => {
                assert_eq!(at, 1);
                assert_eq!(text, "ab");
            }
            _ => panic!("expected Insert"),
        }
    }

    // -----------------------------------------------------------------------
    // Edge cases: caret clamping
    // -----------------------------------------------------------------------

    #[test]
    fn set_caret_clamps_to_len() {
        let mut buf = TextBuffer::new("abc");
        buf.set_caret(100);
        assert_eq!(buf.caret().index, 3);
    }

    #[test]
    fn set_selection_clamps() {
        let mut buf = TextBuffer::new("abc");
        buf.set_selection(50, 100);
        let sel = buf.selection().unwrap();
        assert_eq!(sel.start, 3);
        assert_eq!(sel.end, 3);
    }

    // -----------------------------------------------------------------------
    // Delete word backward / forward
    // -----------------------------------------------------------------------

    #[test]
    fn delete_word_backward_basic() {
        let mut buf = TextBuffer::new("hello world");
        buf.delete_word_backward();
        // Should delete "world" (and possibly trailing space)
        assert!(buf.text().len() < 11);
        assert!(buf.text().starts_with("hello"));
    }

    #[test]
    fn delete_word_forward_basic() {
        let mut buf = TextBuffer::new("hello world");
        buf.set_caret(0);
        buf.delete_word_forward();
        // Should delete "hello" (and possibly leading space)
        assert!(buf.text().len() < 11);
        assert!(buf.text().contains("world"));
    }

    #[test]
    fn delete_word_backward_at_start() {
        let mut buf = TextBuffer::new("hello");
        buf.set_caret(0);
        let op = buf.delete_word_backward();
        assert!(op.is_none());
    }

    #[test]
    fn delete_word_forward_at_end() {
        let mut buf = TextBuffer::new("hello");
        let op = buf.delete_word_forward();
        assert!(op.is_none());
    }

    // -----------------------------------------------------------------------
    // Property-based tests
    // -----------------------------------------------------------------------

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn insert_delete_roundtrip(s in "[a-z]{0,20}") {
                let mut buf = TextBuffer::new("");
                buf.insert_text(&s);
                assert_eq!(buf.text(), s);
                // Delete all characters one by one
                for _ in 0..buf.grapheme_len() {
                    buf.delete_backward();
                }
                assert_eq!(buf.text(), "");
            }

            #[test]
            fn arbitrary_ops_never_panic(
                initial in "[a-z]{0,10}",
                ops in proptest::collection::vec(
                    prop_oneof![
                        Just(0u8), // insert "x"
                        Just(1),   // delete_backward
                        Just(2),   // delete_forward
                        Just(3),   // move_left
                        Just(4),   // move_right
                        Just(5),   // undo
                        Just(6),   // redo
                        Just(7),   // select_all
                    ],
                    0..50
                )
            ) {
                let mut buf = TextBuffer::new(&initial);
                for op in ops {
                    match op {
                        0 => { buf.insert_text("x"); }
                        1 => { buf.delete_backward(); }
                        2 => { buf.delete_forward(); }
                        3 => { buf.move_left(false); }
                        4 => { buf.move_right(false); }
                        5 => { buf.undo(); }
                        6 => { buf.redo(); }
                        7 => { buf.select_all(); }
                        _ => unreachable!(),
                    }
                    // Invariant: caret is always valid
                    assert!(buf.caret().index <= buf.grapheme_len(),
                        "caret {} > len {} after op {}",
                        buf.caret().index, buf.grapheme_len(), op);
                }
            }

            #[test]
            fn undo_reverses_last_insert(s in "[a-z]{1,10}") {
                let mut buf = TextBuffer::new("");
                buf.insert_text(&s);
                assert_eq!(buf.text(), s);
                buf.undo();
                assert_eq!(buf.text(), "");
            }

            #[test]
            fn caret_always_valid_after_movements(
                text in "[a-z ]{0,20}",
                moves in proptest::collection::vec(0..4u8, 0..30)
            ) {
                let mut buf = TextBuffer::new(&text);
                for m in moves {
                    match m {
                        0 => buf.move_left(false),
                        1 => buf.move_right(false),
                        2 => buf.move_to_line_start(false),
                        3 => buf.move_to_line_end(false),
                        _ => unreachable!(),
                    }
                    assert!(buf.caret().index <= buf.grapheme_len());
                }
            }
        }
    }
}
