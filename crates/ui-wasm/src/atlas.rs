use std::collections::HashMap;

use fontdue::Font;

use ui_core::types::{Rect, Vec2};

#[derive(Clone, Debug)]
pub struct Glyph {
    pub uv: Rect,
    pub size: Vec2,
    pub bearing: Vec2,
    pub advance: f32,
}

#[derive(Debug)]
pub struct TextAtlas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    cursor: Vec2,
    row_h: f32,
    glyphs: HashMap<char, Glyph>,
    dirty: bool,
    font: Option<Font>,
}

impl TextAtlas {
    pub fn new(width: u32, height: u32) -> Self {
        let mut pixels = vec![0u8; (width * height) as usize];
        pixels[0] = 255;
        Self {
            width,
            height,
            pixels,
            cursor: Vec2::new(1.0, 1.0),
            row_h: 0.0,
            glyphs: HashMap::new(),
            dirty: true,
            font: None,
        }
    }

    pub fn set_font_bytes(&mut self, bytes: Vec<u8>) {
        if let Ok(font) = Font::from_bytes(bytes, fontdue::FontSettings::default()) {
            self.font = Some(font);
            self.glyphs.clear();
            self.cursor = Vec2::new(1.0, 1.0);
            self.row_h = 0.0;
            self.pixels.fill(0);
            self.pixels[0] = 255;
            self.dirty = true;
        }
    }

    pub fn ensure_glyph(&mut self, ch: char, font_size: f32) -> Glyph {
        if let Some(glyph) = self.glyphs.get(&ch) {
            return glyph.clone();
        }

        let glyph = if let Some(font) = &self.font {
            let (metrics, bitmap) = font.rasterize(ch, font_size);
            let w = metrics.width as u32;
            let h = metrics.height as u32;
            let (x, y) = self.allocate(w.max(1), h.max(1));
            for row in 0..h {
                let dst = ((y + row) * self.width + x) as usize;
                let src = (row * w) as usize;
                let len = w as usize;
                self.pixels[dst..dst + len].copy_from_slice(&bitmap[src..src + len]);
            }
            let uv = Rect::new(
                x as f32 / self.width as f32,
                y as f32 / self.height as f32,
                w as f32 / self.width as f32,
                h as f32 / self.height as f32,
            );
            Glyph {
                uv,
                size: Vec2::new(w as f32, h as f32),
                bearing: Vec2::new(metrics.xmin as f32, metrics.ymin as f32),
                advance: metrics.advance_width,
            }
        } else {
            Glyph {
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                size: Vec2::new(8.0, 12.0),
                bearing: Vec2::new(0.0, 0.0),
                advance: 8.0,
            }
        };

        self.glyphs.insert(ch, glyph.clone());
        glyph
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    fn allocate(&mut self, w: u32, h: u32) -> (u32, u32) {
        let padding = 1.0;
        if self.cursor.x + w as f32 + padding > self.width as f32 {
            self.cursor.x = 1.0;
            self.cursor.y += self.row_h + padding;
            self.row_h = 0.0;
        }
        if self.cursor.y + h as f32 + padding > self.height as f32 {
            self.cursor = Vec2::new(1.0, 1.0);
            self.row_h = 0.0;
            self.glyphs.clear();
            self.pixels.fill(0);
        }
        let x = self.cursor.x as u32;
        let y = self.cursor.y as u32;
        self.cursor.x += w as f32 + padding;
        self.row_h = self.row_h.max(h as f32);
        self.dirty = true;
        (x, y)
    }
}

