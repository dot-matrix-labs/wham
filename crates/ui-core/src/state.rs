use std::sync::Arc;

/// Maximum number of past states retained in the history stack.
/// Prevents unbounded memory growth (OOM) when users make many edits.
const MAX_HISTORY_ENTRIES: usize = 100;

#[derive(Clone, Debug)]
pub struct History<T> {
    past: Vec<Arc<T>>,
    present: Arc<T>,
    future: Vec<Arc<T>>,
}

impl<T> History<T> {
    pub fn new(initial: T) -> Self {
        Self {
            past: Vec::new(),
            present: Arc::new(initial),
            future: Vec::new(),
        }
    }

    pub fn present(&self) -> Arc<T> {
        self.present.clone()
    }

    pub fn push(&mut self, next: T) {
        let current = self.present.clone();
        self.past.push(current);
        if self.past.len() > MAX_HISTORY_ENTRIES {
            let excess = self.past.len() - MAX_HISTORY_ENTRIES;
            self.past.drain(..excess);
        }
        self.present = Arc::new(next);
        self.future.clear();
    }

    /// Returns the maximum number of past states retained.
    pub fn max_entries() -> usize {
        MAX_HISTORY_ENTRIES
    }

    /// Returns the current number of past states.
    pub fn past_len(&self) -> usize {
        self.past.len()
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    pub fn undo(&mut self) -> Option<Arc<T>> {
        let prev = self.past.pop()?;
        let current = self.present.clone();
        self.future.push(current);
        self.present = prev.clone();
        Some(prev)
    }

    pub fn redo(&mut self) -> Option<Arc<T>> {
        let next = self.future.pop()?;
        let current = self.present.clone();
        self.past.push(current);
        self.present = next.clone();
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_push_and_undo() {
        let mut h = History::new(0u32);
        h.push(1);
        h.push(2);
        assert_eq!(*h.present(), 2);
        assert_eq!(h.past_len(), 2);

        h.undo();
        assert_eq!(*h.present(), 1);
    }

    #[test]
    fn history_cap_limits_past_entries() {
        let mut h = History::new(0u32);
        for i in 1..=(MAX_HISTORY_ENTRIES + 50) {
            h.push(i as u32);
        }
        assert_eq!(h.past_len(), MAX_HISTORY_ENTRIES);
        // The oldest entries should have been drained; the earliest
        // remaining past entry is 51 (we pushed 1..=150, kept last 100
        // past entries which are 51..=150, present is 150).
        assert_eq!(*h.present(), (MAX_HISTORY_ENTRIES + 50) as u32);
    }

    #[test]
    fn history_cap_oldest_entries_are_dropped() {
        let mut h = History::new(0u32);
        for i in 1..=(MAX_HISTORY_ENTRIES + 10) {
            h.push(i as u32);
        }
        // Undo all the way back -- we should only be able to undo
        // MAX_HISTORY_ENTRIES times since older entries were evicted.
        let mut undo_count = 0;
        while h.undo().is_some() {
            undo_count += 1;
        }
        assert_eq!(undo_count, MAX_HISTORY_ENTRIES);
        // After undoing everything, present should be the oldest
        // surviving past entry (value 10, which was the present
        // before pushing value 11).
        assert_eq!(*h.present(), 10);
    }

    #[test]
    fn history_cap_does_not_affect_small_stacks() {
        let mut h = History::new(0u32);
        for i in 1..5 {
            h.push(i);
        }
        assert_eq!(h.past_len(), 4);
        assert_eq!(*h.present(), 4);
    }

    #[test]
    fn history_redo_after_cap() {
        let mut h = History::new(0u32);
        for i in 1..=(MAX_HISTORY_ENTRIES + 5) {
            h.push(i as u32);
        }
        // Undo a few, then redo -- future stack should work normally
        h.undo();
        h.undo();
        assert!(h.can_redo());
        h.redo();
        assert_eq!(*h.present(), (MAX_HISTORY_ENTRIES + 4) as u32);
    }
}

