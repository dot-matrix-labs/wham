//! Visual regression test helpers for the wham GPU-rendered forms library.
//!
//! This crate exposes a [`visual_test`] function that renders a widget tree to
//! a headless off-screen pixel buffer (no browser / WebGL required) and
//! compares the result against a reference PNG image.
//!
//! # Example
//!
//! ```rust,ignore
//! use wham_test::{visual_test, ReferenceImage, Size};
//! use std::path::PathBuf;
//!
//! #[test]
//! fn my_widget_looks_right() {
//!     visual_test(
//!         ReferenceImage::FromPng(PathBuf::from("tests/snapshots/my_widget.png")),
//!         Size { width: 400, height: 300 },
//!         |ui| {
//!             ui.label("Hello, world!");
//!         },
//!     )
//!     .tolerance(0.01)
//!     .diff_output("tests/snapshots/my_widget.diff.png")
//!     .assert_matches();
//! }
//! ```
//!
//! ## Update mode
//!
//! Set the environment variable `WHAM_UPDATE_SNAPSHOTS=1` to write the
//! rendered output as the new reference PNG instead of comparing.

use std::path::{Path, PathBuf};

use ui_core::{
    batch::{TextRun, Vertex},
    theme::Theme,
    types::{Color, Rect},
    ui::Ui,
};

static FONT_BYTES: &[u8] = include_bytes!("../assets/Fallback.ttf");

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Dimensions of the off-screen render surface.
#[derive(Clone, Copy, Debug)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

/// Where the reference image comes from.
#[derive(Clone, Debug)]
pub enum ReferenceImage {
    /// Load from a PNG file on disk.
    ///
    /// When `WHAM_UPDATE_SNAPSHOTS=1` is set the rendered output is written to
    /// this path instead of being compared against it.
    FromPng(PathBuf),
}

/// A pending visual test.  Call [`VisualTest::assert_matches`] to execute it.
pub struct VisualTest {
    reference: ReferenceImage,
    size: Size,
    pixels: Vec<u8>, // RGBA, row-major, top-to-bottom
    tolerance: f64,
    diff_path: Option<String>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Render `build` into a headless pixel buffer and return a [`VisualTest`]
/// ready for comparison.
///
/// `build` receives a freshly constructed [`Ui`] whose frame has already been
/// started.  After `build` returns, `end_frame` is called and the resulting
/// draw commands are rasterized by the built-in software renderer.
pub fn visual_test(reference: ReferenceImage, size: Size, build: impl Fn(&mut Ui)) -> VisualTest {
    let pixels = render_to_pixels(size, build);
    VisualTest {
        reference,
        size,
        pixels,
        tolerance: 0.0,
        diff_path: None,
    }
}

// ---------------------------------------------------------------------------
// VisualTest builder / assertion
// ---------------------------------------------------------------------------

impl VisualTest {
    /// Set the per-pixel mismatch tolerance as a fraction of the maximum
    /// possible per-channel error.
    ///
    /// `0.0` (the default) requires an exact pixel match.
    /// `1.0` accepts any pixel regardless of color.
    pub fn tolerance(mut self, t: f64) -> Self {
        self.tolerance = t.clamp(0.0, 1.0);
        self
    }

    /// If set, write a diff PNG (mismatched pixels highlighted in red) to
    /// `path` when the comparison fails.
    pub fn diff_output(mut self, path: &str) -> Self {
        self.diff_path = Some(path.to_owned());
        self
    }

    /// Execute the comparison (or snapshot update) and panic on failure.
    pub fn assert_matches(self) {
        let update_mode =
            std::env::var("WHAM_UPDATE_SNAPSHOTS").map(|v| v == "1").unwrap_or(false);

        match &self.reference {
            ReferenceImage::FromPng(path) => {
                if update_mode {
                    write_png(path, self.size, &self.pixels)
                        .unwrap_or_else(|e| panic!("wham-test: failed to write snapshot: {e}"));
                    return;
                }

                let reference_pixels = load_png(path).unwrap_or_else(|e| {
                    panic!("wham-test: failed to load reference PNG '{}': {e}", path.display())
                });

                let expected_len = (self.size.width * self.size.height * 4) as usize;
                assert_eq!(
                    reference_pixels.len(),
                    expected_len,
                    "wham-test: reference PNG dimensions do not match test size {}x{}",
                    self.size.width,
                    self.size.height,
                );
                assert_eq!(
                    self.pixels.len(),
                    expected_len,
                    "wham-test: rendered pixel buffer has unexpected length"
                );

                let mismatches = compare_pixels(&self.pixels, &reference_pixels, self.tolerance);

                if !mismatches.is_empty() {
                    if let Some(ref diff_path) = self.diff_path {
                        let diff_pixels = build_diff_image(
                            &self.pixels,
                            &reference_pixels,
                            self.size,
                            &mismatches,
                        );
                        let _ = write_png(Path::new(diff_path), self.size, &diff_pixels);
                    }
                    panic!(
                        "wham-test: visual mismatch — {} of {} pixels differ (tolerance {:.2}%)",
                        mismatches.len(),
                        self.size.width * self.size.height,
                        self.tolerance * 100.0,
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public render helper
// ---------------------------------------------------------------------------

/// Render a widget tree to a raw RGBA pixel buffer without performing any
/// comparison.  Useful for generating reference snapshots programmatically.
pub fn render_to_pixels(size: Size, build: impl Fn(&mut Ui)) -> Vec<u8> {
    let width = size.width as f32;
    let height = size.height as f32;
    let theme = Theme::default_light();

    let mut ui = Ui::new(width, height, theme);
    ui.begin_frame(vec![], width, height, 1.0, 0.0);
    build(&mut ui);
    let _a11y = ui.end_frame();

    // Fill background with the theme's background color.
    let bg = color_to_rgba8(ui.theme().colors.background);
    let pixel_count = (size.width * size.height) as usize;
    let mut pixels: Vec<u8> = Vec::with_capacity(pixel_count * 4);
    for _ in 0..pixel_count {
        pixels.extend_from_slice(&bg);
    }

    // Clone batch data to avoid borrow issues.
    let commands: Vec<_> = ui.batch().commands.clone();
    let vertices: Vec<Vertex> = ui.batch().vertices.clone();
    let indices: Vec<u32> = ui.batch().indices.clone();
    let text_runs: Vec<_> = ui.batch().text_runs.clone();

    // Rasterize solid-colour draw commands.
    for cmd in &commands {
        rasterize_batch(cmd, &vertices, &indices, &mut pixels, size);
    }

    // Rasterize text runs using real glyph rendering via fontdue.
    let font = fontdue::Font::from_bytes(FONT_BYTES, fontdue::FontSettings::default())
        .expect("wham-test: bundled font must parse");
    for run in &text_runs {
        render_text_run(&font, run, &mut pixels, size);
    }

    pixels
}

// ---------------------------------------------------------------------------
// Glyph text run renderer
// ---------------------------------------------------------------------------

/// Render a single text run into the pixel buffer using fontdue glyph
/// rasterization, with Porter-Duff "over" compositing and optional clipping.
fn render_text_run(
    font: &fontdue::Font,
    run: &TextRun,
    pixels: &mut Vec<u8>,
    size: Size,
) {
    let font_size = run.font_size;
    let color = run.color;
    let clip = run.clip;

    // Vertical centre: approximate line height from metrics.
    let metrics = font.horizontal_line_metrics(font_size).unwrap_or(fontdue::LineMetrics {
        ascent: font_size * 0.8,
        descent: -(font_size * 0.2),
        line_gap: 0.0,
        new_line_size: font_size,
    });
    let line_height = metrics.ascent - metrics.descent;
    let baseline_y = run.rect.y + (run.rect.h - line_height) * 0.5 + metrics.ascent;

    let mut cursor_x = run.rect.x + 4.0; // small left padding

    for ch in run.text.chars() {
        if ch == ' ' {
            let (m, _) = font.rasterize(' ', font_size);
            cursor_x += m.advance_width;
            continue;
        }
        let (glyph_metrics, bitmap) = font.rasterize(ch, font_size);
        if glyph_metrics.width == 0 || glyph_metrics.height == 0 {
            cursor_x += glyph_metrics.advance_width;
            continue;
        }

        let glyph_x = (cursor_x + glyph_metrics.xmin as f32).round() as i32;
        let glyph_y =
            (baseline_y - glyph_metrics.ymin as f32 - glyph_metrics.height as f32).round() as i32;

        for gy in 0..glyph_metrics.height {
            for gx in 0..glyph_metrics.width {
                let px = glyph_x + gx as i32;
                let py = glyph_y + gy as i32;

                if px < 0 || py < 0 || px >= size.width as i32 || py >= size.height as i32 {
                    continue;
                }

                // Clip rect check.
                if let Some(c) = clip {
                    if px < c.x as i32
                        || py < c.y as i32
                        || px >= (c.x + c.w) as i32
                        || py >= (c.y + c.h) as i32
                    {
                        continue;
                    }
                }

                // Also clip to run rect.
                if px < run.rect.x as i32 || px >= (run.rect.x + run.rect.w) as i32 {
                    continue;
                }

                let coverage = bitmap[gy * glyph_metrics.width + gx] as f32 / 255.0;
                if coverage < 0.01 {
                    continue;
                }

                let idx = ((py as u32 * size.width + px as u32) * 4) as usize;
                if idx + 3 >= pixels.len() {
                    continue;
                }

                let src_a = coverage * color.a;
                let dst_a = pixels[idx + 3] as f32 / 255.0;
                let out_a = src_a + dst_a * (1.0 - src_a);
                if out_a < 1e-6 {
                    continue;
                }

                pixels[idx] =
                    blend_channel((color.r * 255.0) as u8, src_a, pixels[idx], dst_a, out_a);
                pixels[idx + 1] =
                    blend_channel((color.g * 255.0) as u8, src_a, pixels[idx + 1], dst_a, out_a);
                pixels[idx + 2] =
                    blend_channel((color.b * 255.0) as u8, src_a, pixels[idx + 2], dst_a, out_a);
                pixels[idx + 3] = (out_a * 255.0).round() as u8;
            }
        }
        cursor_x += glyph_metrics.advance_width;
    }
}

// ---------------------------------------------------------------------------
// Software rasterizer
// ---------------------------------------------------------------------------

/// Rasterize a single draw command by iterating over its index range.
///
/// Quads are stored as two triangles with indices `[a,b,c, a,c,d]`.  For
/// solid-colour quads all four vertices share the same colour, so we fill the
/// axis-aligned bounding box of each set of six indices (one quad).
fn rasterize_batch(
    cmd: &ui_core::batch::DrawCmd,
    vertices: &[Vertex],
    indices: &[u32],
    pixels: &mut Vec<u8>,
    size: Size,
) {
    let start = cmd.start as usize;
    let end = start + cmd.count as usize;
    if end > indices.len() {
        return;
    }
    let tri_indices = &indices[start..end];

    // Process in groups of 6 indices = 1 quad = 2 triangles.
    let mut i = 0;
    while i + 6 <= tri_indices.len() {
        let ia = tri_indices[i] as usize;
        let ib = tri_indices[i + 1] as usize;
        let ic = tri_indices[i + 2] as usize;
        // Pattern: [a, b, c, a, c, d] — index [i+5] is the 4th vertex.
        let id = tri_indices[i + 5] as usize;

        if ia < vertices.len()
            && ib < vertices.len()
            && ic < vertices.len()
            && id < vertices.len()
        {
            let va = &vertices[ia];
            let vb = &vertices[ib];
            let vc = &vertices[ic];
            let vd = &vertices[id];

            let min_x = va.pos.x.min(vb.pos.x).min(vc.pos.x).min(vd.pos.x);
            let min_y = va.pos.y.min(vb.pos.y).min(vc.pos.y).min(vd.pos.y);
            let max_x = va.pos.x.max(vb.pos.x).max(vc.pos.x).max(vd.pos.x);
            let max_y = va.pos.y.max(vb.pos.y).max(vc.pos.y).max(vd.pos.y);

            let rect = Rect::new(min_x, min_y, max_x - min_x, max_y - min_y);
            fill_rect(pixels, size, rect, va.color, cmd.clip);
        }
        i += 6;
    }

    // Handle any remaining individual triangles.
    while i + 3 <= tri_indices.len() {
        let ia = tri_indices[i] as usize;
        let ib = tri_indices[i + 1] as usize;
        let ic = tri_indices[i + 2] as usize;
        if ia < vertices.len() && ib < vertices.len() && ic < vertices.len() {
            let va = &vertices[ia];
            let vb = &vertices[ib];
            let vc = &vertices[ic];
            let min_x = va.pos.x.min(vb.pos.x).min(vc.pos.x);
            let min_y = va.pos.y.min(vb.pos.y).min(vc.pos.y);
            let max_x = va.pos.x.max(vb.pos.x).max(vc.pos.x);
            let max_y = va.pos.y.max(vb.pos.y).max(vc.pos.y);
            let rect = Rect::new(min_x, min_y, max_x - min_x, max_y - min_y);
            fill_rect(pixels, size, rect, va.color, cmd.clip);
        }
        i += 3;
    }
}

/// Fill an axis-aligned rectangle in the pixel buffer using Porter-Duff "over"
/// compositing, optionally clipped.
fn fill_rect(
    pixels: &mut Vec<u8>,
    size: Size,
    rect: Rect,
    color: Color,
    clip: Option<Rect>,
) {
    let surface_rect = Rect::new(0.0, 0.0, size.width as f32, size.height as f32);

    let effective = {
        let clipped = match clip {
            Some(c) => rect.intersect(c),
            None => Some(rect),
        };
        match clipped.and_then(|r| r.intersect(surface_rect)) {
            Some(r) => r,
            None => return,
        }
    };

    let x0 = (effective.x.floor() as i32).max(0) as u32;
    let y0 = (effective.y.floor() as i32).max(0) as u32;
    let x1 = ((effective.x + effective.w).ceil() as u32).min(size.width);
    let y1 = ((effective.y + effective.h).ceil() as u32).min(size.height);

    if x1 <= x0 || y1 <= y0 {
        return;
    }

    let src = color_to_rgba8(color);
    let src_a = src[3] as f32 / 255.0;

    for y in y0..y1 {
        for x in x0..x1 {
            let idx = ((y * size.width + x) * 4) as usize;
            // Porter-Duff "over" compositing.
            let dst_a = pixels[idx + 3] as f32 / 255.0;
            let out_a = src_a + dst_a * (1.0 - src_a);
            if out_a < 1e-6 {
                pixels[idx] = 0;
                pixels[idx + 1] = 0;
                pixels[idx + 2] = 0;
                pixels[idx + 3] = 0;
            } else {
                pixels[idx] = blend_channel(src[0], src_a, pixels[idx], dst_a, out_a);
                pixels[idx + 1] = blend_channel(src[1], src_a, pixels[idx + 1], dst_a, out_a);
                pixels[idx + 2] = blend_channel(src[2], src_a, pixels[idx + 2], dst_a, out_a);
                pixels[idx + 3] = (out_a * 255.0).round() as u8;
            }
        }
    }
}

#[inline]
fn blend_channel(src: u8, src_a: f32, dst: u8, dst_a: f32, out_a: f32) -> u8 {
    let s = src as f32 / 255.0;
    let d = dst as f32 / 255.0;
    let out = (s * src_a + d * dst_a * (1.0 - src_a)) / out_a;
    (out.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[inline]
fn color_to_rgba8(c: Color) -> [u8; 4] {
    [
        (c.r.clamp(0.0, 1.0) * 255.0).round() as u8,
        (c.g.clamp(0.0, 1.0) * 255.0).round() as u8,
        (c.b.clamp(0.0, 1.0) * 255.0).round() as u8,
        (c.a.clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

// ---------------------------------------------------------------------------
// PNG I/O
// ---------------------------------------------------------------------------

pub(crate) fn write_png(path: &Path, size: Size, pixels: &[u8]) -> Result<(), String> {
    use std::io::BufWriter;

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create_dir_all failed: {e}"))?;
        }
    }
    let file = std::fs::File::create(path)
        .map_err(|e| format!("cannot create '{}': {e}", path.display()))?;
    let mut enc = png::Encoder::new(BufWriter::new(file), size.width, size.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(pixels).map_err(|e| e.to_string())?;
    Ok(())
}

fn load_png(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path)
        .map_err(|e| format!("cannot open '{}': {e}", path.display()))?;
    let dec = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;

    let raw = &buf[..info.buffer_size()];
    let rgba = match (info.color_type, info.bit_depth) {
        (png::ColorType::Rgba, png::BitDepth::Eight) => raw.to_vec(),
        (png::ColorType::Rgb, png::BitDepth::Eight) => {
            raw.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect()
        }
        _ => {
            return Err(format!(
                "unsupported PNG format: {:?} {:?}",
                info.color_type, info.bit_depth
            ))
        }
    };
    Ok(rgba)
}

// ---------------------------------------------------------------------------
// Pixel comparison helpers
// ---------------------------------------------------------------------------

/// Returns the list of pixel indices (not byte offsets) where the two RGBA
/// buffers differ beyond `tolerance`.
fn compare_pixels(actual: &[u8], reference: &[u8], tolerance: f64) -> Vec<usize> {
    let threshold = (tolerance * 255.0) as u32;
    let pixel_count = actual.len() / 4;
    let mut mismatches = Vec::new();
    for i in 0..pixel_count {
        let base = i * 4;
        let diff = channel_diff(actual[base], reference[base])
            .max(channel_diff(actual[base + 1], reference[base + 1]))
            .max(channel_diff(actual[base + 2], reference[base + 2]))
            .max(channel_diff(actual[base + 3], reference[base + 3]));
        if diff > threshold {
            mismatches.push(i);
        }
    }
    mismatches
}

#[inline]
fn channel_diff(a: u8, b: u8) -> u32 {
    (a as i32 - b as i32).unsigned_abs()
}

/// Build a diff image: copy `actual` as base and highlight mismatched pixels
/// in red.
fn build_diff_image(
    actual: &[u8],
    _reference: &[u8],
    _size: Size,
    mismatches: &[usize],
) -> Vec<u8> {
    let mut diff = actual.to_vec();
    for &idx in mismatches {
        let base = idx * 4;
        diff[base] = 255;
        diff[base + 1] = 0;
        diff[base + 2] = 0;
        diff[base + 3] = 255;
    }
    diff
}

// ---------------------------------------------------------------------------
// Interactive test session
// ---------------------------------------------------------------------------

use ui_core::{
    accessibility::A11yTree,
    input::InputEvent,
    types::Vec2,
    ui::{WidgetInfo, WidgetKind},
};

/// The observable output of a single rendered frame.
///
/// Returned by [`Session::next_frame`]. Contains all widgets, text runs, and
/// the accessibility tree produced by calling [`Ui::end_frame`].
pub struct FrameResult {
    pub widgets: Vec<WidgetInfo>,
    pub text_runs: Vec<TextRun>,
    /// Number of solid quads in the draw batch (vertex count / 4).
    pub quad_count: usize,
    pub a11y: A11yTree,
}

impl FrameResult {
    /// Find the first widget whose label equals `label`.
    pub fn widget(&self, label: &str) -> Option<&WidgetInfo> {
        self.widgets.iter().find(|w| w.label == label)
    }

    /// Returns `true` if any text run's text contains `needle`.
    pub fn has_text(&self, needle: &str) -> bool {
        self.text_runs.iter().any(|r| r.text.contains(needle))
    }

    /// Count widgets of the given kind.
    pub fn count_kind(&self, kind: WidgetKind) -> usize {
        self.widgets.iter().filter(|w| w.kind == kind).count()
    }
}

/// A persistent headless UI session. Keeps a [`Ui`] alive across multiple
/// frames so interaction tests can simulate focus changes, typed text, and
/// multi-step flows exactly as they occur in the browser.
pub struct Session {
    ui: Ui,
    pub size: Size,
}

impl Session {
    /// Create a new session with the default light theme.
    pub fn new(size: Size) -> Self {
        let theme = Theme::default_light();
        let ui = Ui::new(size.width as f32, size.height as f32, theme);
        Self { ui, size }
    }

    /// Create a new session with the dark theme.
    pub fn new_dark(size: Size) -> Self {
        let theme = Theme::dark();
        let ui = Ui::new(size.width as f32, size.height as f32, theme);
        Self { ui, size }
    }

    /// Run a single frame: inject events, call `build` to emit widgets, and
    /// return the resulting [`FrameResult`].
    pub fn next_frame(
        &mut self,
        events: Vec<InputEvent>,
        time_ms: f64,
        build: impl FnOnce(&mut Ui),
    ) -> FrameResult {
        let w = self.size.width as f32;
        let h = self.size.height as f32;
        self.ui.begin_frame(events, w, h, 1.0, time_ms);
        build(&mut self.ui);
        let a11y = self.ui.end_frame();
        let widgets = self.ui.widgets().to_vec();
        let text_runs = self.ui.batch().text_runs.clone();
        let quad_count = self.ui.batch().vertices.len() / 4;
        FrameResult { widgets, text_runs, quad_count, a11y }
    }
}

// ---------------------------------------------------------------------------
// Event builder helpers
// ---------------------------------------------------------------------------

/// Build a pointer click (down then up) at the given canvas position.
pub fn click_at(pos: Vec2) -> Vec<InputEvent> {
    use ui_core::input::{Modifiers, PointerButton, PointerEvent};
    let ev = PointerEvent {
        pos,
        button: Some(PointerButton::Left),
        modifiers: Modifiers::default(),
    };
    vec![InputEvent::PointerDown(ev), InputEvent::PointerUp(ev)]
}

/// Build a [`TextInput`](InputEvent::TextInput) event for each character.
pub fn type_text(text: &str) -> Vec<InputEvent> {
    use ui_core::input::TextInputEvent;
    text.chars()
        .map(|c| InputEvent::TextInput(TextInputEvent { text: c.to_string() }))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join("wham_test_snapshots").join(name)
    }

    // -----------------------------------------------------------------------
    // render_to_pixels
    // -----------------------------------------------------------------------

    #[test]
    fn render_produces_correct_buffer_size() {
        let size = Size { width: 100, height: 80 };
        let pixels = render_to_pixels(size, |_ui| {});
        assert_eq!(pixels.len(), (100 * 80 * 4) as usize);
    }

    #[test]
    fn render_background_is_near_white() {
        let size = Size { width: 64, height: 64 };
        let pixels = render_to_pixels(size, |_ui| {});
        // Light theme background is near-white; all channels should be > 200.
        assert!(pixels[0] > 200, "expected near-white red, got {}", pixels[0]);
        assert!(pixels[1] > 200, "expected near-white green, got {}", pixels[1]);
        assert!(pixels[2] > 200, "expected near-white blue, got {}", pixels[2]);
        assert_eq!(pixels[3], 255, "background alpha should be fully opaque");
    }

    #[test]
    fn render_with_label_widget_has_correct_size() {
        let size = Size { width: 320, height: 240 };
        let pixels = render_to_pixels(size, |ui| {
            ui.label("Hello");
        });
        assert_eq!(pixels.len(), (320 * 240 * 4) as usize);
    }

    #[test]
    fn render_with_button_widget_has_correct_size() {
        let size = Size { width: 400, height: 300 };
        let pixels = render_to_pixels(size, |ui| {
            ui.label("Name");
            ui.button("Submit");
        });
        assert_eq!(pixels.len(), (400 * 300 * 4) as usize);
    }

    // -----------------------------------------------------------------------
    // PNG round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn png_round_trip_preserves_pixels() {
        let size = Size { width: 4, height: 4 };
        let pixels = render_to_pixels(size, |_ui| {});

        let path = tmp_path("round_trip.png");
        let _ = std::fs::remove_file(&path);

        write_png(&path, size, &pixels).expect("write failed");
        let loaded = load_png(&path).expect("load failed");
        let _ = std::fs::remove_file(&path);

        assert_eq!(pixels, loaded, "PNG round-trip changed pixel values");
    }

    // -----------------------------------------------------------------------
    // Pixel comparison
    // -----------------------------------------------------------------------

    #[test]
    fn compare_identical_pixels_no_mismatches() {
        let pixels: Vec<u8> = (0u16..256).flat_map(|i| [(i % 256) as u8; 4]).collect();
        let mismatches = compare_pixels(&pixels, &pixels, 0.0);
        assert!(mismatches.is_empty());
    }

    #[test]
    fn compare_single_mismatch_detected() {
        let mut a = vec![0u8; 16]; // 4 pixels x 4 bytes
        let mut b = vec![0u8; 16];
        // Differ on pixel 2 (bytes 8-11).
        a[8] = 200;
        b[8] = 0;
        let mismatches = compare_pixels(&a, &b, 0.0);
        assert_eq!(mismatches, vec![2]);
    }

    #[test]
    fn compare_within_tolerance_passes() {
        let a = vec![100u8, 0, 0, 255];
        let b = vec![110u8, 0, 0, 255];
        // Difference is 10/255 ~= 3.9%; tolerance = 5% => should pass.
        let mismatches = compare_pixels(&a, &b, 0.05);
        assert!(mismatches.is_empty());
    }

    #[test]
    fn compare_exceeds_tolerance_fails() {
        let a = vec![0u8, 0, 0, 255];
        let b = vec![200u8, 0, 0, 255];
        let mismatches = compare_pixels(&a, &b, 0.0);
        assert_eq!(mismatches.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Diff image
    // -----------------------------------------------------------------------

    #[test]
    fn diff_image_highlights_mismatches_in_red() {
        let actual = vec![0u8, 128, 64, 255];
        let reference = vec![255u8, 0, 0, 255];
        let size = Size { width: 1, height: 1 };
        let mismatches = compare_pixels(&actual, &reference, 0.0);
        let diff = build_diff_image(&actual, &reference, size, &mismatches);
        assert_eq!(diff[0], 255, "diff r should be 255");
        assert_eq!(diff[1], 0, "diff g should be 0");
        assert_eq!(diff[2], 0, "diff b should be 0");
        assert_eq!(diff[3], 255, "diff a should be 255");
    }

    // -----------------------------------------------------------------------
    // update_mode helper
    // -----------------------------------------------------------------------

    #[test]
    fn update_mode_writes_png_without_panicking() {
        let path = tmp_path("update_mode_test.png");
        let _ = std::fs::remove_file(&path);

        let size = Size { width: 32, height: 32 };
        let pixels = render_to_pixels(size, |_ui| {});
        write_png(&path, size, &pixels).expect("write_png failed");

        assert!(path.exists(), "snapshot PNG was not written");
        let _ = std::fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // visual_test passes when rendered output matches the reference
    // -----------------------------------------------------------------------

    #[test]
    fn visual_test_passes_against_self() {
        let size = Size { width: 200, height: 150 };

        // Write reference from the first render.
        let path = tmp_path("self_compare.png");
        let _ = std::fs::remove_file(&path);
        let pixels = render_to_pixels(size, |ui| {
            ui.label("Snapshot");
        });
        write_png(&path, size, &pixels).expect("write reference failed");

        // Compare a second identical render against it.
        visual_test(ReferenceImage::FromPng(path.clone()), size, |ui| {
            ui.label("Snapshot");
        })
        .tolerance(0.01)
        .assert_matches();

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn visual_test_with_multiple_widgets() {
        let size = Size { width: 400, height: 300 };

        let path = tmp_path("multi_widget.png");
        let _ = std::fs::remove_file(&path);
        let pixels = render_to_pixels(size, |ui| {
            ui.label("Name");
            ui.button("Submit");
        });
        write_png(&path, size, &pixels).expect("write reference failed");

        visual_test(ReferenceImage::FromPng(path.clone()), size, |ui| {
            ui.label("Name");
            ui.button("Submit");
        })
        .assert_matches();

        let _ = std::fs::remove_file(&path);
    }
}
