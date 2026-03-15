use std::collections::HashMap;

use fontdue::Font;

use ui_core::types::{Rect, Vec2};

/// Quantize a font size to 2px buckets to limit glyph cache explosion.
/// For example, 11px and 12px map to the same bucket (12), while 13px maps
/// to a different bucket (14).
pub fn quantize_font_size(font_size: f32) -> u16 {
    ((font_size / 2.0).round() as u16) * 2
}

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
    glyphs: HashMap<(char, u16), Glyph>,
    dirty: bool,
    font: Option<Font>,
    generation: u64,
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
            generation: 0,
        }
    }

    #[allow(dead_code)]
    pub fn generation(&self) -> u64 {
        self.generation
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
            self.generation += 1;
        }
    }

    /// Pre-rasterize all glyphs in `text` at the given `font_size`, ensuring
    /// they are cached in the atlas before the render pass begins.
    pub fn ensure_glyphs_cached(&mut self, text: &str, font_size: f32) {
        for ch in text.chars() {
            if ch == '\n' {
                continue;
            }
            self.ensure_glyph(ch, font_size);
        }
    }

    /// Look up a previously cached glyph at a specific font size. Returns
    /// `None` if the glyph has not been rasterized yet at that quantized size
    /// (callers on the render path should treat this as a bug — layout should
    /// have pre-populated the atlas).
    pub fn get_cached_glyph(&self, ch: char, font_size: f32) -> Option<&Glyph> {
        let key = (ch, quantize_font_size(font_size));
        self.glyphs.get(&key)
    }

    pub fn ensure_glyph(&mut self, ch: char, font_size: f32) -> Glyph {
        let quantized = quantize_font_size(font_size);
        let key = (ch, quantized);
        if let Some(glyph) = self.glyphs.get(&key) {
            return glyph.clone();
        }

        // Rasterize at the quantized size for consistent cache behavior.
        let raster_size = quantized as f32;
        let glyph = if let Some(font) = &self.font {
            let (metrics, bitmap) = font.rasterize(ch, raster_size);
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

        self.glyphs.insert(key, glyph.clone());
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

    /// Invalidate all cached glyphs so they are re-rasterized on next use.
    ///
    /// Called after WebGL context loss because the GPU texture backing the
    /// atlas has been destroyed. The CPU-side pixel buffer is preserved so
    /// that re-uploading is possible immediately after a new texture is
    /// created.
    pub fn invalidate_gpu_cache(&mut self) {
        self.dirty = true;
    }

    fn allocate(&mut self, w: u32, h: u32) -> (u32, u32) {
        let padding = 1.0;
        if self.cursor.x + w as f32 + padding > self.width as f32 {
            self.cursor.x = 1.0;
            self.cursor.y += self.row_h + padding;
            self.row_h = 0.0;
        }
        if self.cursor.y + h as f32 + padding > self.height as f32 {
            #[cfg(target_arch = "wasm32")]
            web_sys::console::warn_1(
                &format!(
                    "TextAtlas overflow: atlas {}x{} full, clearing glyph cache (generation {})",
                    self.width,
                    self.height,
                    self.generation + 1,
                )
                .into(),
            );
            #[cfg(not(target_arch = "wasm32"))]
            eprintln!(
                "TextAtlas overflow: atlas {}x{} full, clearing glyph cache (generation {})",
                self.width,
                self.height,
                self.generation + 1,
            );
            self.cursor = Vec2::new(1.0, 1.0);
            self.row_h = 0.0;
            self.glyphs.clear();
            self.pixels.fill(0);
            self.pixels[0] = 255;
            self.generation += 1;
        }
        let x = self.cursor.x as u32;
        let y = self.cursor.y as u32;
        self.cursor.x += w as f32 + padding;
        self.row_h = self.row_h.max(h as f32);
        self.dirty = true;
        (x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_font_size_2px_buckets() {
        // 11px and 12px both round to 12
        assert_eq!(quantize_font_size(11.0), 12);
        assert_eq!(quantize_font_size(12.0), 12);
        // 13px rounds to 14
        assert_eq!(quantize_font_size(13.0), 14);
        // 16px stays 16
        assert_eq!(quantize_font_size(16.0), 16);
        // 24px stays 24
        assert_eq!(quantize_font_size(24.0), 24);
    }

    #[test]
    fn same_char_different_sizes_produces_different_cache_entries() {
        let mut atlas = TextAtlas::new(256, 256);
        // Without a font, ensure_glyph returns fallback glyphs, but the
        // cache keys should still be distinct.
        let _g12 = atlas.ensure_glyph('A', 12.0);
        let _g24 = atlas.ensure_glyph('A', 24.0);

        // Both should be independently retrievable.
        assert!(atlas.get_cached_glyph('A', 12.0).is_some());
        assert!(atlas.get_cached_glyph('A', 24.0).is_some());

        // The cache should contain two entries (quantized 12 and 24).
        assert_eq!(atlas.glyphs.len(), 2);
    }

    #[test]
    fn quantized_sizes_share_cache_entry() {
        let mut atlas = TextAtlas::new(256, 256);
        let g11 = atlas.ensure_glyph('B', 11.0);
        let g12 = atlas.ensure_glyph('B', 12.0);

        // 11px and 12px quantize to the same bucket (12), so they should
        // return the same cached glyph and only one entry should exist.
        assert_eq!(atlas.glyphs.len(), 1);
        assert_eq!(g11.advance, g12.advance);
    }

    #[test]
    fn cache_lookup_miss_returns_none() {
        let atlas = TextAtlas::new(256, 256);
        // No glyphs cached yet — lookup should return None.
        assert!(atlas.get_cached_glyph('Z', 16.0).is_none());
    }

    #[test]
    fn ensure_glyphs_cached_populates_all_chars() {
        let mut atlas = TextAtlas::new(256, 256);
        atlas.ensure_glyphs_cached("AB", 16.0);

        assert!(atlas.get_cached_glyph('A', 16.0).is_some());
        assert!(atlas.get_cached_glyph('B', 16.0).is_some());
        // Different size should miss.
        assert!(atlas.get_cached_glyph('A', 24.0).is_none());
    }
}
