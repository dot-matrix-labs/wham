use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Caret {
    pub index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub start: usize,
    pub end: usize,
}

impl Selection {
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

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextEditOp {
    Insert { at: usize, text: String },
    Delete { start: usize, end: usize, text: String },
}

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

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn caret(&self) -> Caret {
        self.caret
    }

    pub fn selection(&self) -> Option<Selection> {
        self.selection
    }

    pub fn composition(&self) -> Option<Selection> {
        self.composition
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        let len = self.grapheme_len();
        self.caret.index = len;
        self.selection = None;
        self.composition = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub fn grapheme_len(&self) -> usize {
        UnicodeSegmentation::graphemes(self.text.as_str(), true).count()
    }

    pub fn select_all(&mut self) {
        let len = self.grapheme_len();
        self.selection = Some(Selection { start: 0, end: len });
        self.caret.index = len;
    }

    pub fn set_caret(&mut self, index: usize) {
        let len = self.grapheme_len();
        self.caret.index = index.min(len);
        self.selection = None;
    }

    pub fn set_selection(&mut self, start: usize, end: usize) {
        let len = self.grapheme_len();
        let s = start.min(len);
        let e = end.min(len);
        self.selection = Some(Selection { start: s, end: e });
        self.caret.index = e;
    }

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

    pub fn move_left(&mut self, extend_selection: bool) {
        if self.caret.index == 0 {
            return;
        }
        let new_index = self.caret.index - 1;
        self.move_to(new_index, extend_selection);
    }

    pub fn move_right(&mut self, extend_selection: bool) {
        let len = self.grapheme_len();
        if self.caret.index >= len {
            return;
        }
        let new_index = self.caret.index + 1;
        self.move_to(new_index, extend_selection);
    }

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

    pub fn begin_composition(&mut self) {
        self.composition = Some(Selection {
            start: self.caret.index,
            end: self.caret.index,
        });
    }

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
