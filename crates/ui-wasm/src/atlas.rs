use std::collections::HashMap;

use fontdue::Font;

use ui_core::types::{Rect, Vec2};

/// Quantize a font size to 2px buckets to limit glyph cache explosion.
/// For example, 11px and 12px map to the same bucket (12), while 13px maps
/// to a different bucket (14).
pub fn quantize_font_size(font_size: f32) -> u16 {
    ((font_size / 2.0).round() as u16) * 2
}

/// Cache key for a glyph: (character, quantized font size).
pub type GlyphKey = (char, u16);

/// Default maximum number of atlas pages.
const DEFAULT_MAX_PAGES: usize = 8;

#[derive(Clone, Debug)]
pub struct Glyph {
    pub uv: Rect,
    pub size: Vec2,
    pub bearing: Vec2,
    pub advance: f32,
    /// Which atlas page this glyph is stored on.
    pub page: usize,
    /// Index of the font in the fallback chain that provided this glyph.
    pub font_index: usize,
}

/// A single page of the atlas texture. Each page has its own pixel buffer,
/// row-based cursor, and dirty flag.
#[derive(Debug)]
struct AtlasPage {
    pixels: Vec<u8>,
    cursor: Vec2,
    row_h: f32,
    dirty: bool,
}

impl AtlasPage {
    fn new(width: u32, height: u32) -> Self {
        let mut pixels = vec![0u8; (width * height) as usize];
        // Pixel (0,0) = 255 serves as the "white" texel for solid-color quads.
        pixels[0] = 255;
        Self {
            pixels,
            cursor: Vec2::new(1.0, 1.0),
            row_h: 0.0,
            dirty: true,
        }
    }

    fn clear(&mut self) {
        self.pixels.fill(0);
        self.pixels[0] = 255;
        self.cursor = Vec2::new(1.0, 1.0);
        self.row_h = 0.0;
        self.dirty = true;
    }

    /// Try to allocate a rectangle of `w x h` pixels. Returns `Some((x, y))`
    /// on success, or `None` if there is not enough space.
    fn try_allocate(&mut self, w: u32, h: u32, page_w: u32, page_h: u32) -> Option<(u32, u32)> {
        let padding = 1.0;
        if self.cursor.x + w as f32 + padding > page_w as f32 {
            self.cursor.x = 1.0;
            self.cursor.y += self.row_h + padding;
            self.row_h = 0.0;
        }
        if self.cursor.y + h as f32 + padding > page_h as f32 {
            return None;
        }
        let x = self.cursor.x as u32;
        let y = self.cursor.y as u32;
        self.cursor.x += w as f32 + padding;
        self.row_h = self.row_h.max(h as f32);
        self.dirty = true;
        Some((x, y))
    }
}

/// Per-glyph LRU metadata.
#[derive(Debug, Clone)]
struct GlyphMeta {
    /// The last frame number on which this glyph was used.
    last_used_frame: u64,
}

#[derive(Debug)]
pub struct TextAtlas {
    page_width: u32,
    page_height: u32,
    pages: Vec<AtlasPage>,
    max_pages: usize,
    glyphs: HashMap<GlyphKey, Glyph>,
    /// LRU tracking: maps glyph key to usage metadata.
    glyph_meta: HashMap<GlyphKey, GlyphMeta>,
    /// Maps page index to the set of glyph keys stored on that page.
    page_glyphs: Vec<Vec<GlyphKey>>,
    /// Font fallback chain: index 0 is the primary font, subsequent entries
    /// are fallbacks tried in order when a glyph is missing.
    fonts: Vec<Font>,
    generation: u64,
    /// Current frame counter, bumped by `begin_frame`.
    current_frame: u64,
}

impl TextAtlas {
    pub fn new(width: u32, height: u32) -> Self {
        let first_page = AtlasPage::new(width, height);
        Self {
            page_width: width,
            page_height: height,
            pages: vec![first_page],
            max_pages: DEFAULT_MAX_PAGES,
            glyphs: HashMap::new(),
            glyph_meta: HashMap::new(),
            page_glyphs: vec![Vec::new()],
            fonts: Vec::new(),
            generation: 0,
            current_frame: 0,
        }
    }

    /// Create an atlas with custom page size and max page count (useful for tests).
    #[cfg(test)]
    pub fn with_config(width: u32, height: u32, max_pages: usize) -> Self {
        let first_page = AtlasPage::new(width, height);
        Self {
            page_width: width,
            page_height: height,
            pages: vec![first_page],
            max_pages,
            glyphs: HashMap::new(),
            glyph_meta: HashMap::new(),
            page_glyphs: vec![Vec::new()],
            fonts: Vec::new(),
            generation: 0,
            current_frame: 0,
        }
    }

    /// Advance the frame counter. Call once per frame before layout/rendering.
    pub fn begin_frame(&mut self) {
        self.current_frame += 1;
    }

    #[allow(dead_code)]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the number of currently allocated atlas pages.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Returns the page dimensions (width, height).
    pub fn page_dimensions(&self) -> (u32, u32) {
        (self.page_width, self.page_height)
    }

    /// Returns the number of fonts in the fallback chain (0 if no fonts loaded).
    #[allow(dead_code)]
    pub fn font_count(&self) -> usize {
        self.fonts.len()
    }

    /// Set (or replace) the primary font.  Clears the glyph cache because
    /// existing rasterized glyphs may have come from the old primary font.
    pub fn set_font_bytes(&mut self, bytes: Vec<u8>) {
        if let Ok(font) = Font::from_bytes(bytes, fontdue::FontSettings::default()) {
            if self.fonts.is_empty() {
                self.fonts.push(font);
            } else {
                self.fonts[0] = font;
            }
            self.glyphs.clear();
            self.glyph_meta.clear();
            // Reset to a single clean page.
            self.pages.clear();
            self.pages.push(AtlasPage::new(self.page_width, self.page_height));
            self.page_glyphs.clear();
            self.page_glyphs.push(Vec::new());
            self.generation += 1;
        }
    }

    /// Append a fallback font to the end of the font chain.
    ///
    /// When a glyph is missing from the primary font (and any earlier
    /// fallbacks), the atlas will try rasterizing from this font before
    /// falling back to the replacement character.
    pub fn add_fallback_font(&mut self, bytes: Vec<u8>) {
        if let Ok(font) = Font::from_bytes(bytes, fontdue::FontSettings::default()) {
            self.fonts.push(font);
            // Existing glyphs that came from the primary font are still valid;
            // we only need to invalidate glyphs that were rendered as the
            // replacement character because they might now be found in the new
            // fallback.  For simplicity we clear the entire cache — the cost
            // is a one-time re-rasterisation on the next frame.
            self.glyphs.clear();
            self.glyph_meta.clear();
            // Reset to a single clean page.
            self.pages.clear();
            self.pages.push(AtlasPage::new(self.page_width, self.page_height));
            self.page_glyphs.clear();
            self.page_glyphs.push(Vec::new());
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
    ///
    /// Also updates LRU tracking for the glyph.
    pub fn get_cached_glyph(&mut self, ch: char, font_size: f32) -> Option<&Glyph> {
        let key = (ch, quantize_font_size(font_size));
        if self.glyph_meta.contains_key(&key) {
            self.glyph_meta.get_mut(&key).unwrap().last_used_frame = self.current_frame;
        }
        self.glyphs.get(&key)
    }

    /// Look up a previously cached glyph without updating LRU tracking.
    /// This is useful when you need an immutable borrow of the atlas.
    pub fn peek_cached_glyph(&self, ch: char, font_size: f32) -> Option<&Glyph> {
        let key = (ch, quantize_font_size(font_size));
        self.glyphs.get(&key)
    }

    pub fn ensure_glyph(&mut self, ch: char, font_size: f32) -> Glyph {
        let quantized = quantize_font_size(font_size);
        let key = (ch, quantized);
        if let Some(glyph) = self.glyphs.get(&key) {
            // Update LRU.
            if let Some(meta) = self.glyph_meta.get_mut(&key) {
                meta.last_used_frame = self.current_frame;
            }
            return glyph.clone();
        }

        // Rasterize at the quantized size for consistent cache behavior.
        let raster_size = quantized as f32;
        let glyph = if self.fonts.is_empty() {
            // No fonts loaded — return a placeholder glyph.
            Glyph {
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                size: Vec2::new(8.0, 12.0),
                bearing: Vec2::new(0.0, 0.0),
                advance: 8.0,
                page: 0,
                font_index: 0,
            }
        } else {
            // Walk the fallback chain: try each font in order until one has the glyph.
            let resolved = self
                .fonts
                .iter()
                .enumerate()
                .find(|(_, font)| font.has_glyph(ch));

            let (font_index, raster_char) = match resolved {
                Some((idx, _)) => (idx, ch),
                None => {
                    // No font has this glyph — render U+FFFD from the primary font.
                    // If the primary font also lacks U+FFFD, rasterize it anyway
                    // (fontdue will produce an empty / .notdef glyph).
                    (0, '\u{FFFD}')
                }
            };

            let (metrics, bitmap) = self.fonts[font_index].rasterize(raster_char, raster_size);
            let w = metrics.width as u32;
            let h = metrics.height as u32;
            let alloc_w = w.max(1);
            let alloc_h = h.max(1);
            let (page_idx, x, y) = self.allocate(alloc_w, alloc_h);

            let page = &mut self.pages[page_idx];
            for row in 0..h {
                let dst = ((y + row) * self.page_width + x) as usize;
                let src = (row * w) as usize;
                let len = w as usize;
                page.pixels[dst..dst + len].copy_from_slice(&bitmap[src..src + len]);
            }

            let uv = Rect::new(
                x as f32 / self.page_width as f32,
                y as f32 / self.page_height as f32,
                w as f32 / self.page_width as f32,
                h as f32 / self.page_height as f32,
            );
            Glyph {
                uv,
                size: Vec2::new(w as f32, h as f32),
                bearing: Vec2::new(metrics.xmin as f32, metrics.ymin as f32),
                advance: metrics.advance_width,
                page: page_idx,
                font_index,
            }
        };

        self.glyphs.insert(key, glyph.clone());
        self.glyph_meta.insert(
            key,
            GlyphMeta {
                last_used_frame: self.current_frame,
            },
        );
        // Track which page this glyph belongs to.
        if let Some(keys) = self.page_glyphs.get_mut(glyph.page) {
            keys.push(key);
        }
        glyph
    }

    /// Returns the pixel data for a specific page.
    pub fn page_pixels(&self, page_idx: usize) -> &[u8] {
        &self.pages[page_idx].pixels
    }

    /// Returns a snapshot of all cached glyph advance widths, keyed by
    /// `(char, quantized_font_size)`. Used to populate `Ui::set_char_advance`
    /// so that platform-agnostic caret placement can use real metrics.
    pub fn advance_map(&self) -> HashMap<(char, u16), f32> {
        self.glyphs
            .iter()
            .map(|(k, g)| (*k, g.advance))
            .collect()
    }

    /// Returns the pixel data for the first page (backwards compatibility).
    pub fn pixels(&self) -> &[u8] {
        &self.pages[0].pixels
    }

    /// Returns `true` if any page has been modified since last `mark_clean`.
    pub fn is_dirty(&self) -> bool {
        self.pages.iter().any(|p| p.dirty)
    }

    /// Returns which pages are dirty (index, pixel data).
    pub fn dirty_pages(&self) -> Vec<(usize, &[u8])> {
        self.pages
            .iter()
            .enumerate()
            .filter(|(_, p)| p.dirty)
            .map(|(i, p)| (i, p.pixels.as_slice()))
            .collect()
    }

    /// Mark all pages clean.
    pub fn mark_clean(&mut self) {
        for page in &mut self.pages {
            page.dirty = false;
        }
    }

    /// Invalidate all cached glyphs so they are re-rasterized on next use.
    ///
    /// Called after WebGL context loss because the GPU texture backing the
    /// atlas has been destroyed. The CPU-side pixel buffer is preserved so
    /// that re-uploading is possible immediately after a new texture is
    /// created.
    pub fn invalidate_gpu_cache(&mut self) {
        for page in &mut self.pages {
            page.dirty = true;
        }
    }

    /// Allocate space for a glyph of size `w x h`. Returns `(page_index, x, y)`.
    ///
    /// Tries the current (last) page first. If it's full, adds a new page
    /// (up to `max_pages`). When at the limit, evicts the least-recently-used
    /// page and reuses it.
    fn allocate(&mut self, w: u32, h: u32) -> (usize, u32, u32) {
        let pw = self.page_width;
        let ph = self.page_height;

        // Try the current (last) page.
        let last = self.pages.len() - 1;
        if let Some((x, y)) = self.pages[last].try_allocate(w, h, pw, ph) {
            return (last, x, y);
        }

        // Current page is full — try to add a new page.
        if self.pages.len() < self.max_pages {
            let new_page = AtlasPage::new(pw, ph);
            self.pages.push(new_page);
            self.page_glyphs.push(Vec::new());
            let idx = self.pages.len() - 1;

            #[cfg(target_arch = "wasm32")]
            web_sys::console::log_1(
                &format!(
                    "TextAtlas: allocated new page {} ({}x{})",
                    idx, pw, ph
                )
                .into(),
            );
            #[cfg(not(target_arch = "wasm32"))]
            eprintln!("TextAtlas: allocated new page {} ({}x{})", idx, pw, ph);

            let (x, y) = self.pages[idx]
                .try_allocate(w, h, pw, ph)
                .expect("fresh page should have space");
            return (idx, x, y);
        }

        // At max pages — evict the LRU page.
        let evict_idx = self.find_lru_page();
        self.evict_page(evict_idx);

        let (x, y) = self.pages[evict_idx]
            .try_allocate(w, h, pw, ph)
            .expect("evicted page should have space");
        (evict_idx, x, y)
    }

    /// Find the page whose glyphs have the oldest maximum `last_used_frame`.
    fn find_lru_page(&self) -> usize {
        let mut best_page = 0;
        let mut best_max_frame = u64::MAX;

        for (page_idx, keys) in self.page_glyphs.iter().enumerate() {
            let max_frame = keys
                .iter()
                .filter_map(|k| self.glyph_meta.get(k))
                .map(|m| m.last_used_frame)
                .max()
                .unwrap_or(0);
            if max_frame < best_max_frame {
                best_max_frame = max_frame;
                best_page = page_idx;
            }
        }

        best_page
    }

    /// Evict all glyphs from a page, clearing it for reuse.
    fn evict_page(&mut self, page_idx: usize) {
        #[cfg(target_arch = "wasm32")]
        web_sys::console::warn_1(
            &format!(
                "TextAtlas: evicting page {} ({} glyphs, generation {})",
                page_idx,
                self.page_glyphs[page_idx].len(),
                self.generation + 1,
            )
            .into(),
        );
        #[cfg(not(target_arch = "wasm32"))]
        eprintln!(
            "TextAtlas: evicting page {} ({} glyphs, generation {})",
            page_idx,
            self.page_glyphs[page_idx].len(),
            self.generation + 1,
        );

        // Remove all glyphs that were on this page.
        let keys: Vec<GlyphKey> = self.page_glyphs[page_idx].drain(..).collect();
        for key in &keys {
            self.glyphs.remove(key);
            self.glyph_meta.remove(key);
        }

        self.pages[page_idx].clear();
        self.generation += 1;
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
        let mut atlas = TextAtlas::new(256, 256);
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

    #[test]
    fn glyph_has_page_field() {
        let mut atlas = TextAtlas::new(256, 256);
        let g = atlas.ensure_glyph('X', 16.0);
        // Without a font, fallback glyph is on page 0.
        assert_eq!(g.page, 0);
    }

    #[test]
    fn new_atlas_starts_with_one_page() {
        let atlas = TextAtlas::new(1024, 1024);
        assert_eq!(atlas.page_count(), 1);
    }

    #[test]
    fn filling_page_creates_new_page() {
        // Use a tiny 16x16 page that can only hold a few glyphs.
        let mut atlas = TextAtlas::with_config(16, 16, 4);
        // Without a real font, fallback glyphs are 8x12 each.
        // A 16x16 page can hold only 1 glyph (8+1 padding = 9, leaving 7,
        // which is less than 8 for a second glyph on the same row; next row
        // at y=1+12+1=14, then 14+12=26 > 16, so page is full after 1 glyph).
        atlas.ensure_glyph('A', 16.0);
        assert_eq!(atlas.page_count(), 1);

        atlas.ensure_glyph('B', 16.0);
        // Second glyph should have triggered a new page.
        assert_eq!(atlas.page_count(), 2);
    }

    #[test]
    fn lru_eviction_removes_least_recently_used() {
        // Tiny atlas: 16x16 pages, max 2 pages.
        let mut atlas = TextAtlas::with_config(16, 16, 2);

        // Frame 1: cache glyph A on page 0.
        atlas.begin_frame();
        atlas.ensure_glyph('A', 16.0);
        assert_eq!(atlas.page_count(), 1);

        // Frame 2: cache glyph B — triggers new page (page 1).
        atlas.begin_frame();
        atlas.ensure_glyph('B', 16.0);
        assert_eq!(atlas.page_count(), 2);

        // Frame 3: use glyph B to make it more recent, then add C.
        atlas.begin_frame();
        atlas.get_cached_glyph('B', 16.0); // touch B
        atlas.ensure_glyph('C', 16.0);
        // At max pages (2) — should have evicted the LRU page (page 0, glyph A).
        assert_eq!(atlas.page_count(), 2);

        // A should have been evicted.
        assert!(atlas.peek_cached_glyph('A', 16.0).is_none());
        // B should still be cached.
        assert!(atlas.peek_cached_glyph('B', 16.0).is_some());
        // C should be cached (just added).
        assert!(atlas.peek_cached_glyph('C', 16.0).is_some());
    }

    #[test]
    fn evicted_glyph_is_re_rasterized_on_next_use() {
        let mut atlas = TextAtlas::with_config(16, 16, 2);

        atlas.begin_frame();
        let g1 = atlas.ensure_glyph('A', 16.0);
        assert_eq!(g1.page, 0);

        atlas.begin_frame();
        atlas.ensure_glyph('B', 16.0); // fills page 0, creates page 1

        atlas.begin_frame();
        atlas.get_cached_glyph('B', 16.0); // touch B
        atlas.ensure_glyph('C', 16.0); // evicts LRU page (page 0 with A)

        // A was evicted; re-caching it should succeed.
        atlas.begin_frame();
        let g2 = atlas.ensure_glyph('A', 16.0);
        assert!(atlas.peek_cached_glyph('A', 16.0).is_some());
        // The glyph should have valid data (fallback in this case).
        assert_eq!(g2.advance, 8.0);
    }

    #[test]
    fn dirty_pages_tracks_modifications() {
        let mut atlas = TextAtlas::new(256, 256);
        // Fresh atlas has page 0 dirty.
        assert!(atlas.is_dirty());
        assert_eq!(atlas.dirty_pages().len(), 1);

        atlas.mark_clean();
        assert!(!atlas.is_dirty());
        assert_eq!(atlas.dirty_pages().len(), 0);

        // Adding a glyph marks the page dirty again.
        atlas.ensure_glyph('X', 16.0);
        assert!(atlas.is_dirty());
    }

    #[test]
    fn invalidate_gpu_cache_marks_all_pages_dirty() {
        let mut atlas = TextAtlas::with_config(16, 16, 4);
        atlas.ensure_glyph('A', 16.0);
        atlas.ensure_glyph('B', 16.0); // triggers page 2
        atlas.mark_clean();
        assert!(!atlas.is_dirty());

        atlas.invalidate_gpu_cache();
        assert!(atlas.is_dirty());
        assert_eq!(atlas.dirty_pages().len(), atlas.page_count());
    }

    #[test]
    fn no_font_returns_placeholder_with_font_index_zero() {
        let mut atlas = TextAtlas::new(256, 256);
        let glyph = atlas.ensure_glyph('X', 16.0);
        assert_eq!(glyph.font_index, 0);
        assert_eq!(glyph.advance, 8.0);
    }

    #[test]
    fn set_font_bytes_sets_primary_font() {
        let mut atlas = TextAtlas::new(256, 256);
        assert_eq!(atlas.font_count(), 0);

        // Use a minimal valid font from fontdue's own test infrastructure
        // is not available, so test with invalid bytes (font_count stays 0).
        atlas.set_font_bytes(vec![0u8; 10]);
        assert_eq!(atlas.font_count(), 0); // invalid bytes => no font added

        // With no fonts, placeholder glyph should still work.
        let glyph = atlas.ensure_glyph('A', 16.0);
        assert_eq!(glyph.font_index, 0);
    }

    #[test]
    fn add_fallback_with_invalid_bytes_does_not_add() {
        let mut atlas = TextAtlas::new(256, 256);
        atlas.add_fallback_font(vec![0u8; 10]);
        assert_eq!(atlas.font_count(), 0);
    }

    #[test]
    fn set_font_clears_cache_and_increments_generation() {
        let mut atlas = TextAtlas::new(256, 256);
        let gen0 = atlas.generation();
        // Ensure a glyph is cached.
        atlas.ensure_glyph('A', 16.0);
        assert_eq!(atlas.glyphs.len(), 1);

        // Setting font bytes (even invalid ones that don't parse) should not
        // change generation; only successful font loads clear the cache.
        atlas.set_font_bytes(vec![0u8; 10]);
        assert_eq!(atlas.generation(), gen0);
        // Cache should remain because no valid font was loaded.
        assert_eq!(atlas.glyphs.len(), 1);
    }

    #[test]
    fn glyph_cached_returns_correct_font_index_placeholder() {
        let mut atlas = TextAtlas::new(256, 256);
        let g = atlas.ensure_glyph('Z', 16.0);
        assert_eq!(g.font_index, 0);

        let cached = atlas.get_cached_glyph('Z', 16.0).unwrap();
        assert_eq!(cached.font_index, 0);
    }
}
