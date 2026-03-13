use std::sync::Arc;

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
        self.present = Arc::new(next);
        self.future.clear();
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

