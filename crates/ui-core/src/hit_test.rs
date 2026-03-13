use crate::types::{Rect, Vec2};

#[derive(Clone, Debug)]
pub struct HitTestEntry {
    pub id: u64,
    pub rect: Rect,
}

#[derive(Clone, Debug)]
pub struct HitTestGrid {
    cell_size: f32,
    width: usize,
    height: usize,
    cells: Vec<Vec<HitTestEntry>>,
}

impl HitTestGrid {
    pub fn new(width: f32, height: f32, cell_size: f32) -> Self {
        let w = (width / cell_size).ceil() as usize;
        let h = (height / cell_size).ceil() as usize;
        let mut cells = Vec::with_capacity(w * h);
        for _ in 0..(w * h) {
            cells.push(Vec::new());
        }
        Self {
            cell_size,
            width: w,
            height: h,
            cells,
        }
    }

    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.clear();
        }
    }

    pub fn insert(&mut self, entry: HitTestEntry) {
        let min_x = (entry.rect.x / self.cell_size).floor() as isize;
        let min_y = (entry.rect.y / self.cell_size).floor() as isize;
        let max_x = ((entry.rect.x + entry.rect.w) / self.cell_size).floor() as isize;
        let max_y = ((entry.rect.y + entry.rect.h) / self.cell_size).floor() as isize;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height
                {
                    let idx = y as usize * self.width + x as usize;
                    self.cells[idx].push(entry.clone());
                }
            }
        }
    }

    pub fn hit_test(&self, pos: Vec2) -> Option<u64> {
        let x = (pos.x / self.cell_size).floor() as isize;
        let y = (pos.y / self.cell_size).floor() as isize;
        if x < 0 || y < 0 || (x as usize) >= self.width || (y as usize) >= self.height {
            return None;
        }
        let idx = y as usize * self.width + x as usize;
        for entry in &self.cells[idx] {
            if entry.rect.contains(pos) {
                return Some(entry.id);
            }
        }
        None
    }
}

