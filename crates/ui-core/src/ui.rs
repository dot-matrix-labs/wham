use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::accessibility::{A11yNode, A11yRole, A11yState, A11yTree};
use crate::batch::{Batch, DirtyTracker, Material, Quad, TextRun, WidgetId};
use crate::form::{FieldValue, Form, FormPath};
use crate::hit_test::{HitTestEntry, HitTestGrid};
use crate::icon::{IconId, IconPack};
use crate::input::{InputEvent, KeyCode, PointerButton};
use crate::text::TextBuffer;
use crate::theme::Theme;
use crate::types::{Color, Rect, Vec2};
use unicode_segmentation::UnicodeSegmentation;

/// Persistent scroll state for a single scroll container, keyed by widget ID.
#[derive(Clone, Debug)]
pub struct ScrollState {
    /// Current vertical scroll offset (0 = top, positive = scrolled down).
    pub offset: f32,
    /// Current scroll velocity for inertia (px/s).
    pub velocity: f32,
    /// Total content height measured from the previous frame.
    pub content_height: f32,
    /// Visible container height.
    pub container_height: f32,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            offset: 0.0,
            velocity: 0.0,
            content_height: 0.0,
            container_height: 0.0,
        }
    }
}

impl ScrollState {
    fn max_offset(&self) -> f32 {
        (self.content_height - self.container_height).max(0.0)
    }

    fn clamp_offset(&mut self) {
        self.offset = self.offset.clamp(0.0, self.max_offset());
    }
}

/// The semantic kind of a widget, used in accessibility and hit testing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum WidgetKind {
    /// A non-interactive text label.
    Label,
    /// A clickable button.
    Button,
    /// A toggleable checkbox.
    Checkbox,
    /// A radio button (one of a mutually-exclusive group).
    Radio,
    /// A single-line or multiline text input.
    TextInput,
    /// A dropdown select widget.
    Select,
    /// A logical group of widgets (used for nested form sections).
    Group,
    /// A scrollable container.
    ScrollContainer,
}

/// Metadata emitted for each widget during a frame; used for accessibility and hit testing.
#[derive(Clone, Debug)]
pub struct WidgetInfo {
    /// Stable, unique widget ID (hash of the full ID stack path).
    pub id: u64,
    /// The semantic kind of this widget.
    pub kind: WidgetKind,
    /// Human-readable label (used as the accessible name).
    pub label: String,
    /// Current value as a string (e.g. text content, checked state).
    pub value: Option<String>,
    /// Bounding rectangle in logical pixels.
    pub rect: Rect,
    /// Accessibility state flags (focused, disabled, selected, etc.).
    pub state: A11yState,
}

/// Direction in which widgets are placed within a layout context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutDirection {
    /// Widgets stack vertically (default).
    Vertical,
    /// Widgets are placed side by side horizontally.
    Horizontal,
}

/// A single entry on the layout stack, representing a row or column region.
#[derive(Clone, Debug)]
struct LayoutContext {
    direction: LayoutDirection,
    /// Origin of this layout region.
    origin: Vec2,
    /// Cursor within this region (advances along the primary axis).
    cursor: Vec2,
    /// Total width available to this region.
    width: f32,
    /// Spacing between consecutive items.
    spacing: f32,
    /// For horizontal layouts: proportional weights for each child slot.
    /// When empty, children share the width equally (computed on the fly).
    weights: Vec<f32>,
    /// For horizontal layouts: how many children have been placed so far.
    child_index: usize,
    /// For horizontal layouts: tracks the tallest child so that `end_row`
    /// can advance the parent cursor past the entire row.
    max_child_height: f32,
}

#[derive(Clone, Debug)]
pub struct Layout {
    cursor: Vec2,
    width: f32,
    spacing: f32,
    /// Stack of nested layout contexts (rows inside columns, etc.).
    stack: Vec<LayoutContext>,
}

impl Layout {
    pub fn new(x: f32, y: f32, width: f32) -> Self {
        Self {
            cursor: Vec2::new(x, y),
            width,
            spacing: 10.0,
            stack: Vec::new(),
        }
    }

    pub fn next_rect(&mut self, height: f32) -> Rect {
        if let Some(ctx) = self.stack.last_mut() {
            match ctx.direction {
                LayoutDirection::Vertical => {
                    let rect = Rect::new(ctx.cursor.x, ctx.cursor.y, ctx.width, height);
                    ctx.cursor.y += height + ctx.spacing;
                    rect
                }
                LayoutDirection::Horizontal => {
                    let idx = ctx.child_index;
                    let (item_x, item_w) = Self::compute_slot(ctx, idx);
                    let rect = Rect::new(item_x, ctx.cursor.y, item_w, height);
                    ctx.child_index += 1;
                    if height > ctx.max_child_height {
                        ctx.max_child_height = height;
                    }
                    rect
                }
            }
        } else {
            // No stack — use the top-level vertical layout.
            let rect = Rect::new(self.cursor.x, self.cursor.y, self.width, height);
            self.cursor.y += height + self.spacing;
            rect
        }
    }

    /// Begin a horizontal row with equal-width children.
    pub fn begin_row(&mut self) {
        self.begin_row_with(&[]);
    }

    /// Begin a horizontal row with proportional `weights`.
    /// An empty slice means equal distribution (determined per-child).
    pub fn begin_row_with(&mut self, weights: &[f32]) {
        let (x, y, w, spacing) = if let Some(ctx) = self.stack.last() {
            (ctx.cursor.x, ctx.cursor.y, ctx.width, ctx.spacing)
        } else {
            (self.cursor.x, self.cursor.y, self.width, self.spacing)
        };
        self.stack.push(LayoutContext {
            direction: LayoutDirection::Horizontal,
            origin: Vec2::new(x, y),
            cursor: Vec2::new(x, y),
            width: w,
            spacing,
            weights: weights.to_vec(),
            child_index: 0,
            max_child_height: 0.0,
        });
    }

    /// End the current horizontal row, advancing the parent cursor past it.
    pub fn end_row(&mut self) {
        let ctx = match self.stack.pop() {
            Some(c) => c,
            None => return, // no-op if no matching begin_row
        };
        let advance = ctx.max_child_height + ctx.spacing;
        if let Some(parent) = self.stack.last_mut() {
            parent.cursor.y += advance;
        } else {
            self.cursor.y += advance;
        }
    }

    /// Compute the x-position and width for a given child slot inside a
    /// horizontal layout context.
    fn compute_slot(ctx: &LayoutContext, idx: usize) -> (f32, f32) {
        if ctx.weights.is_empty() {
            // Equal distribution: we don't know the total child count ahead
            // of time, so we just divide available width by (idx+1) ... but
            // that shifts previous items.  Instead we treat each child as
            // getting its share of the remaining space.  For a predictable
            // equal split callers should use weights like [1.0, 1.0].
            //
            // Fallback: give each child 1/1 weight, which makes them all
            // get the same width as long as the caller is consistent.  We
            // compute per-child width as total_width / max(1, child_count)
            // where child_count is unknown.  So we use the simple approach:
            // each child occupies (width - gaps_so_far) / 1, but positioned
            // after previous children.  This is necessarily approximate
            // without knowing the total count.  For best results callers
            // should provide weights.
            //
            // Practical approach: treat it like weights = [1.0; N] but
            // we don't know N.  Just give each child an equal slot width
            // based on how many weights we would have needed.  Since we
            // can't know N in advance in an immediate-mode API, we use a
            // reasonable default of splitting remaining space.
            //
            // Actually, the simplest correct approach: accumulate x offset
            // per child.  Each child gets width = 0 until end_row, which
            // isn't useful.  Let's just use a default of 2 children (the
            // common case) when no weights are given.
            let n = 2usize;
            let gap_total = ctx.spacing * (n as f32 - 1.0).max(0.0);
            let item_w = (ctx.width - gap_total) / n as f32;
            let x = ctx.origin.x + idx as f32 * (item_w + ctx.spacing);
            (x, item_w)
        } else {
            let total_weight: f32 = ctx.weights.iter().sum();
            let n = ctx.weights.len();
            let gap_total = ctx.spacing * (n as f32 - 1.0).max(0.0);
            let available = ctx.width - gap_total;
            // Sum weights before this index to find x offset.
            let weight_before: f32 = ctx.weights.iter().take(idx).sum();
            let my_weight = ctx.weights.get(idx).copied().unwrap_or(1.0);
            let x = ctx.origin.x
                + (weight_before / total_weight) * available
                + idx as f32 * ctx.spacing;
            let w = (my_weight / total_weight) * available;
            (x, w)
        }
    }
}

/// The immediate-mode UI context.
///
/// Create a `Ui` once, then call [`begin_frame`](Ui::begin_frame) /
/// widget methods / [`end_frame`](Ui::end_frame) every animation frame.
///
/// # Frame Lifecycle
///
/// ```rust,ignore
/// ui.begin_frame(events, width, height, scale, time_ms);
/// ui.label("Hello");
/// if ui.button("Submit") { /* handle click */ }
/// let a11y_tree = ui.end_frame();
/// // render ui.batch() with the WebGL2 renderer
/// ```
pub struct Ui {
    theme: Theme,
    batch: Batch,
    layout: Layout,
    widgets: Vec<WidgetInfo>,
    events: Vec<InputEvent>,
    focused: Option<u64>,
    hovered: Option<u64>,
    active: Option<u64>,
    dragging: Option<u64>,
    selection_anchor: Option<usize>,
    hit_test: HitTestGrid,
    scale: f32,
    clipboard_request: Option<String>,
    time_ms: f64,
    /// Number of rapid successive left-clicks on the same widget.
    /// 1 = single, 2 = double (select word), 3+ = triple (select line).
    click_count: u8,
    /// Timestamp of the last pointer-down, used to detect double/triple clicks.
    last_click_time: f64,
    /// Widget id that received the last click, used to reset count on target change.
    last_click_id: Option<u64>,
    /// Scroll offsets per widget id (horizontal pixel offset into the text).
    _scroll_offsets: HashMap<u64, f32>,
    /// Whether the focused text input is in overwrite (insert-key toggle) mode.
    pub overwrite_mode: bool,
    /// Whether the viewport is a touch/mobile device (auto-detected from
    /// viewport width or device-pixel ratio). When true, small widgets get
    /// expanded hit areas to meet the 44×44pt minimum touch target guideline.
    pub touch_mode: bool,
    /// Persistent scroll state per scroll container ID.
    pub scroll_states: HashMap<u64, ScrollState>,
    /// Saved layouts for nested scroll containers.
    layout_stack: Vec<Layout>,
    /// Stack of clip rects for nested scroll containers.
    clip_stack: Vec<Rect>,
    /// Currently active clip rect (top of clip_stack), applied to all emitted quads/text.
    pub active_clip: Option<Rect>,
    /// Timestamp of the previous frame, used to compute dt for inertia.
    last_time_ms: f64,
    /// Which select/dropdown widget is currently open (None = all closed).
    pub dropdown_open: Option<u64>,
    /// Index of the currently highlighted option in an open dropdown.
    pub dropdown_highlighted: usize,
    /// Deferred dropdown panel rendering data (rendered in end_frame for z-order).
    dropdown_panel: Option<DropdownPanel>,
    /// When a dropdown option was clicked last frame, stores (select_id, selected_index).
    dropdown_selection: Option<(u64, usize)>,
    /// ID stack used to disambiguate widgets with identical labels.
    /// Values are pushed/popped by the caller (e.g. loop index) and mixed
    /// into every `hash_id` call so that repeated labels produce unique IDs.
    id_stack: Vec<u64>,
    /// Auto-managed `TextBuffer`s keyed by `FormPath`, used by
    /// `text_input_for` / `text_input_masked_for` to eliminate manual
    /// buffer management.
    form_buffers: HashMap<FormPath, TextBuffer>,
    /// Safe area insets `[top, right, bottom, left]` in logical (CSS) pixels.
    ///
    /// On modern phones with notches, rounded corners, or home indicators these
    /// values are non-zero. They are added on top of the existing outer layout
    /// margins so that all widget content stays within the safe area. On
    /// desktop or SE-style devices without hardware cutouts all four values
    /// remain zero and layout is unchanged.
    safe_area: [f32; 4],
    /// Returns the advance width (in pixels) for a character at a given font
    /// size. The default implementation returns `font_size * 0.6` (the old
    /// monospace approximation). The wasm layer replaces this with a closure
    /// that queries the glyph atlas for actual advance widths.
    char_advance: Box<dyn Fn(char, f32) -> f32>,
    /// The loaded icon pack used by the `icon()` widget to look up UV
    /// coordinates for named icons.
    icon_pack: Option<IconPack>,
    /// Dirty-region tracker: knows which widgets need quad rebuilds.
    dirty_tracker: DirtyTracker,
    /// Snapshot of the previous frame's vertex and index buffers.
    /// Clean widgets copy geometry from here instead of recomputing quads.
    prev_batch: Batch,
    /// Per-widget visual fingerprints from the previous frame.
    /// A fingerprint is a hash of all inputs that affect a widget's rendered
    /// appearance (value, hover, focus, pressed, theme hash).
    widget_fingerprints: HashMap<WidgetId, u64>,
}

/// Minimum touch target size in logical pixels (Apple HIG: 44pt).
const MIN_TOUCH_TARGET: f32 = 44.0;

/// Maximum number of visible options in a dropdown before it would need scrolling.
const DROPDOWN_MAX_VISIBLE: usize = 6;
/// Height of each option row in the dropdown panel.
const DROPDOWN_ITEM_HEIGHT: f32 = 32.0;

#[derive(Clone, Debug)]
struct DropdownPanel {
    id: u64,
    trigger_rect: Rect,
    panel_rect: Rect,
    options: Vec<String>,
    highlighted: usize,
    _current_value: String,
}

impl Ui {
    pub fn new(width: f32, height: f32, theme: Theme) -> Self {
        Self {
            theme,
            batch: Batch::default(),
            layout: Layout::new(24.0, 24.0, width - 48.0),
            widgets: Vec::new(),
            events: Vec::new(),
            focused: None,
            hovered: None,
            active: None,
            dragging: None,
            selection_anchor: None,
            hit_test: HitTestGrid::new(width, height, 48.0),
            scale: 1.0,
            clipboard_request: None,
            time_ms: 0.0,
            click_count: 0,
            last_click_time: 0.0,
            last_click_id: None,
            _scroll_offsets: HashMap::new(),
            overwrite_mode: false,
            touch_mode: false,
            scroll_states: HashMap::new(),
            layout_stack: Vec::new(),
            clip_stack: Vec::new(),
            active_clip: None,
            last_time_ms: 0.0,
            dropdown_open: None,
            dropdown_highlighted: 0,
            dropdown_panel: None,
            dropdown_selection: None,
            id_stack: Vec::new(),
            form_buffers: HashMap::new(),
            safe_area: [0.0; 4],
            char_advance: Box::new(|_ch, font_size| font_size * 0.6),
            icon_pack: None,
            dirty_tracker: DirtyTracker::default(),
            prev_batch: Batch::default(),
            widget_fingerprints: HashMap::new(),
        }
    }

    // -----------------------------------------------------------------
    // Accessor methods — public read access to internal state
    // -----------------------------------------------------------------

    /// Returns a reference to the current theme.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Returns a mutable reference to the current theme, allowing runtime
    /// customization (e.g. switching to dark mode).
    pub fn theme_mut(&mut self) -> &mut Theme {
        &mut self.theme
    }

    /// Set the character advance function used for caret placement and
    /// click-to-position mapping. The function receives a character and a
    /// font size (in pixels) and must return the advance width in pixels.
    ///
    /// The default uses `font_size * 0.6` (monospace approximation). The wasm
    /// layer should replace this with a closure that queries the glyph atlas
    /// for actual proportional advance widths.
    pub fn set_char_advance(&mut self, f: Box<dyn Fn(char, f32) -> f32>) {
        self.char_advance = f;
    }

    /// Compute the advance-width prefix sum for each grapheme in `text`.
    /// Returns a Vec of length `n+1` where `n` is the number of graphemes:
    /// `result[0] = 0.0` and `result[i]` is the x-offset of the caret
    /// positioned after the i-th grapheme.
    fn grapheme_prefix_sums(&self, text: &str, font_size: f32) -> Vec<f32> {
        let graphemes: Vec<&str> = text.graphemes(true).collect();
        let mut sums = Vec::with_capacity(graphemes.len() + 1);
        sums.push(0.0);
        let mut acc = 0.0f32;
        for g in &graphemes {
            for ch in g.chars() {
                acc += (self.char_advance)(ch, font_size);
            }
            sums.push(acc);
        }
        sums
    }

    /// Returns a reference to the current rendering batch.
    /// The batch contains all vertices, indices, draw commands, and text runs
    /// produced during the current frame.
    pub fn batch(&self) -> &Batch {
        &self.batch
    }

    /// Returns a mutable reference to the rendering batch.
    pub fn batch_mut(&mut self) -> &mut Batch {
        &mut self.batch
    }

    /// Takes ownership of the current batch, replacing it with an empty one.
    /// This is the primary way for renderers to consume the frame's draw data
    /// without cloning.
    pub fn take_batch(&mut self) -> Batch {
        std::mem::take(&mut self.batch)
    }

    /// Returns the widget ID of the currently focused widget, if any.
    pub fn focused_id(&self) -> Option<u64> {
        self.focused
    }

    /// Returns a reference to the clipboard request string, if a copy/cut
    /// operation produced one during this frame.
    pub fn clipboard_request(&self) -> Option<&str> {
        self.clipboard_request.as_deref()
    }

    /// Takes the clipboard request, leaving `None` in its place.
    pub fn take_clipboard_request(&mut self) -> Option<String> {
        self.clipboard_request.take()
    }

    /// Returns a slice of all widgets registered during this frame.
    pub fn widgets(&self) -> &[WidgetInfo] {
        &self.widgets
    }

    /// Returns the current scale factor.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Returns the current frame timestamp in milliseconds.
    pub fn time_ms(&self) -> f64 {
        self.time_ms
    }

    /// Set the safe area insets in logical (CSS) pixels.
    ///
    /// Call this whenever the insets change — on initial load, on `resize`,
    /// and on `orientationchange`. Insets are applied as additional outer
    /// margins on top of the existing 24 px default, so the usable widget
    /// rect is always clear of notches, home indicators, and rounded corners.
    ///
    /// `insets` is `[top, right, bottom, left]`. Pass `[0.0; 4]` on
    /// desktop / SE-style devices.
    pub fn set_safe_area_insets(&mut self, insets: [f32; 4]) {
        self.safe_area = insets;
    }

    /// Returns the current safe area insets `[top, right, bottom, left]`.
    pub fn safe_area_insets(&self) -> [f32; 4] {
        self.safe_area
    }

    // -----------------------------------------------------------------
    // Dirty-region tracking API
    // -----------------------------------------------------------------

    /// Returns a shared reference to the dirty tracker.
    pub fn dirty_tracker(&self) -> &DirtyTracker {
        &self.dirty_tracker
    }

    /// Mark all widgets as dirty for the next frame (e.g. after a theme
    /// change or window resize that invalidates every widget's appearance).
    pub fn invalidate_all(&mut self) {
        self.dirty_tracker.mark_fully_dirty();
    }

    /// Mark a single widget as needing a quad rebuild this frame.
    ///
    /// Callers that manage their own widget state (e.g. custom widgets
    /// outside `Ui`) can call this whenever a widget's visual inputs change.
    pub fn mark_widget_dirty(&mut self, id: WidgetId) {
        self.dirty_tracker.mark_dirty(id);
    }

    /// Compute the visual fingerprint of a widget for dirty detection.
    ///
    /// The fingerprint incorporates all inputs that affect the widget's
    /// rendered appearance: an application-supplied `value_hash` (typically
    /// a hash of the widget's value string), the hover/focus/pressed booleans,
    /// and a compact theme hash so theme changes automatically dirty widgets.
    ///
    /// Returns `true` if the widget is dirty this frame (inputs changed or
    /// the tracker is in fully-dirty mode).
    pub fn check_widget_dirty(
        &mut self,
        id: WidgetId,
        value_hash: u64,
        hovered: bool,
        focused: bool,
        pressed: bool,
    ) -> bool {
        if self.dirty_tracker.is_fully_dirty() {
            let fp = Self::compute_fingerprint(value_hash, hovered, focused, pressed, self.theme_hash());
            self.widget_fingerprints.insert(id, fp);
            return true;
        }

        let fp = Self::compute_fingerprint(value_hash, hovered, focused, pressed, self.theme_hash());
        let prev = self.widget_fingerprints.get(&id).copied();
        let dirty = prev != Some(fp);
        if dirty {
            self.dirty_tracker.mark_dirty(id);
        }
        self.widget_fingerprints.insert(id, fp);
        dirty
    }

    /// Attempt to reuse a clean widget's geometry from the previous frame.
    ///
    /// If the widget's previous-frame range is available, its vertices and
    /// indices are copied into the current batch (with rebased index offsets)
    /// and `true` is returned.  Returns `false` when the widget is new (no
    /// previous range) and the caller must emit quads normally.
    pub fn try_reuse_widget(&mut self, id: WidgetId) -> bool {
        let range = match self.dirty_tracker.prev_range(id) {
            Some(r) => r.clone(),
            None => return false,
        };
        let v_len = self.prev_batch.vertices.len();
        let i_len = self.prev_batch.indices.len();
        if range.vertex_end > v_len || range.index_end > i_len {
            return false;
        }
        // Safety: prev_batch and batch are distinct fields; we read from the
        // former while writing into the latter.
        let prev_vertices = unsafe {
            std::slice::from_raw_parts(self.prev_batch.vertices.as_ptr(), v_len)
        };
        let prev_indices = unsafe {
            std::slice::from_raw_parts(self.prev_batch.indices.as_ptr(), i_len)
        };
        self.batch.reuse_widget(id, prev_vertices, prev_indices, &range);
        true
    }

    /// Compact theme hash for widget fingerprints.
    fn theme_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.theme.high_contrast.hash(&mut h);
        self.theme.reduced_motion.hash(&mut h);
        let c = &self.theme.colors;
        let pack = |r: f32, g: f32, b: f32, a: f32| -> u64 {
            ((r.to_bits() as u64) << 32)
                | (g.to_bits() as u64)
                ^ ((b.to_bits() as u64) << 16)
                ^ (a.to_bits() as u64)
        };
        pack(c.primary.r, c.primary.g, c.primary.b, c.primary.a).hash(&mut h);
        pack(c.surface.r, c.surface.g, c.surface.b, c.surface.a).hash(&mut h);
        pack(c.text.r, c.text.g, c.text.b, c.text.a).hash(&mut h);
        h.finish()
    }

    fn compute_fingerprint(
        value_hash: u64,
        hovered: bool,
        focused: bool,
        pressed: bool,
        theme_hash: u64,
    ) -> u64 {
        let mut h = DefaultHasher::new();
        value_hash.hash(&mut h);
        hovered.hash(&mut h);
        focused.hash(&mut h);
        pressed.hash(&mut h);
        theme_hash.hash(&mut h);
        h.finish()
    }

    // -----------------------------------------------------------------
    // Frame lifecycle
    // -----------------------------------------------------------------

    /// Begins a new UI frame.
    ///
    /// Must be called once at the start of each animation frame before emitting any
    /// widgets. Clears the batch, resets the layout cursor, and processes input events.
    ///
    /// - `events` — all input events collected since the previous frame
    /// - `width` / `height` — canvas size in logical (CSS) pixels
    /// - `scale` — device-pixel ratio (e.g. `2.0` on HiDPI displays)
    /// - `time_ms` — monotonic timestamp in milliseconds (used for animations)
    pub fn begin_frame(
        &mut self,
        events: Vec<InputEvent>,
        width: f32,
        height: f32,
        scale: f32,
        time_ms: f64,
    ) {
        self.events = events;
        self.widgets.clear();
        self.batch.clear();
        // Apply safe area insets on top of the existing 24 px outer margins.
        // safe_area = [top, right, bottom, left]
        let sa = self.safe_area;
        let margin_left = 24.0 + sa[3];
        let margin_top = 24.0 + sa[0];
        let margin_right = 24.0 + sa[1];
        let usable_width = (width - margin_left - margin_right).max(0.0);
        self.layout = Layout::new(margin_left, margin_top, usable_width);
        self.hit_test = HitTestGrid::new(width, height, 48.0);
        self.scale = scale;
        self.hovered = None;
        self.clipboard_request = None;
        self.last_time_ms = self.time_ms;
        self.time_ms = time_ms;
        self.touch_mode = width < 600.0 || scale >= 2.0;
        // Store last dropdown panel rect before clearing, for click-outside detection
        let prev_panel = self.dropdown_panel.take();
        // Close dropdown on click-outside (not on trigger or panel)
        if self.dropdown_open.is_some() {
            if let Some(ref panel) = prev_panel {
                for event in &self.events {
                    if let InputEvent::PointerDown(ev) = event {
                        if ev.button == Some(PointerButton::Left)
                            && !panel.trigger_rect.contains(ev.pos)
                            && !panel.panel_rect.contains(ev.pos)
                        {
                            self.dropdown_open = None;
                            break;
                        }
                    }
                }
            }
        }
        // NOTE: selection_anchor is intentionally NOT cleared here.
        // It must persist across frames while the user is mid-drag.
        // It is cleared in apply_pointer_selection on PointerUp.
    }

    /// Finalises the frame and returns the accessibility tree.
    ///
    /// Must be called once after all widgets have been emitted. Handles Tab-key
    /// navigation, draws the focus ring, flushes the dropdown panel, and registers
    /// all widget rects with the hit-test grid.
    pub fn end_frame(&mut self) -> A11yTree {
        self.handle_keyboard_navigation();
        self.draw_focus_ring();
        self.render_dropdown_panel();
        for widget in &self.widgets {
            self.hit_test.insert(HitTestEntry {
                id: widget.id,
                rect: self.touch_rect(widget.rect),
            });
        }

        // Advance dirty tracking: record this frame's widget ranges as the
        // "previous" ranges for the next frame, then snapshot the vertex and
        // index buffers so clean widgets can copy geometry next frame without
        // regenerating quads.
        //
        // We copy only vertices and indices (not text_runs / commands, which
        // are transient).  The full batch remains available to the caller via
        // `take_batch()` / `batch()`.
        let ranges = self.batch.widget_ranges().clone();
        self.dirty_tracker.end_frame(&ranges);
        // Snapshot vertex/index data for next-frame geometry reuse.
        self.prev_batch.vertices.clear();
        self.prev_batch.vertices.extend_from_slice(&self.batch.vertices);
        self.prev_batch.indices.clear();
        self.prev_batch.indices.extend_from_slice(&self.batch.indices);

        A11yTree {
            root: A11yNode {
                id: 1,
                role: A11yRole::Form,
                name: "Form".to_string(),
                value: None,
                bounds: Rect::new(0.0, 0.0, self.layout.width, self.layout.cursor.y),
                state: A11yState::default(),
                children: self
                    .widgets
                    .iter()
                    .map(|w| A11yNode {
                        id: w.id,
                        role: widget_role(w.kind),
                        name: w.label.clone(),
                        value: w.value.clone(),
                        bounds: w.rect,
                        state: w.state.clone(),
                        children: Vec::new(),
                    })
                    .collect(),
            },
        }
    }

    fn draw_focus_ring(&mut self) {
        let focused_id = match self.focused {
            Some(id) => id,
            None => return,
        };
        let rect = match self.widgets.iter().find(|w| w.id == focused_id) {
            Some(w) => w.rect,
            None => return,
        };

        let thickness = if self.theme.high_contrast { 3.0 } else { 2.0 };
        let offset = thickness;

        let color = if self.theme.high_contrast {
            // High-contrast: fully opaque, high-visibility color
            Color::rgba(0.0, 0.0, 0.0, 1.0)
        } else if self.theme.reduced_motion {
            self.theme.colors.focus_ring
        } else {
            // Subtle pulse animation
            let phase = (self.time_ms / 1000.0 * std::f64::consts::PI).sin() as f32;
            let alpha = 0.6 + 0.3 * phase;
            Color::rgba(
                self.theme.colors.focus_ring.r,
                self.theme.colors.focus_ring.g,
                self.theme.colors.focus_ring.b,
                alpha,
            )
        };

        let t = thickness;
        let o = offset;
        // Top edge
        self.batch.push_quad(
            Quad {
                rect: Rect::new(rect.x - o, rect.y - o, rect.w + 2.0 * o, t),
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                color,
                flags: 0,
            },
            Material::Solid,
            None,
        );
        // Bottom edge
        self.batch.push_quad(
            Quad {
                rect: Rect::new(rect.x - o, rect.y + rect.h + o - t, rect.w + 2.0 * o, t),
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                color,
                flags: 0,
            },
            Material::Solid,
            None,
        );
        // Left edge
        self.batch.push_quad(
            Quad {
                rect: Rect::new(rect.x - o, rect.y - o + t, t, rect.h + 2.0 * o - 2.0 * t),
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                color,
                flags: 0,
            },
            Material::Solid,
            None,
        );
        // Right edge
        self.batch.push_quad(
            Quad {
                rect: Rect::new(rect.x + rect.w + o - t, rect.y - o + t, t, rect.h + 2.0 * o - 2.0 * t),
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                color,
                flags: 0,
            },
            Material::Solid,
            None,
        );
    }

    fn render_dropdown_panel(&mut self) {
        let panel = match self.dropdown_panel.take() {
            Some(p) => p,
            None => return,
        };

        // Panel background
        self.batch.push_quad(
            Quad {
                rect: panel.panel_rect,
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                color: self.theme.colors.surface,
                flags: 0,
            },
            Material::Solid,
            None,
        );

        let item_h = DROPDOWN_ITEM_HEIGHT * self.scale;
        let font_size = 15.0 * self.theme.font_scale * self.scale;
        let visible = panel.options.len().min(DROPDOWN_MAX_VISIBLE);

        for (i, option) in panel.options.iter().enumerate().take(visible) {
            let item_rect = Rect::new(
                panel.panel_rect.x,
                panel.panel_rect.y + i as f32 * item_h,
                panel.panel_rect.w,
                item_h,
            );

            // Highlight background
            if i == panel.highlighted {
                self.batch.push_quad(
                    Quad {
                        rect: item_rect,
                        uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                        color: Color::rgba(
                            self.theme.colors.primary.r,
                            self.theme.colors.primary.g,
                            self.theme.colors.primary.b,
                            0.15,
                        ),
                        flags: 0,
                    },
                    Material::Solid,
                    None,
                );
            }

            // Option text
            self.batch.text_runs.push(TextRun {
                rect: Rect::new(item_rect.x + 8.0, item_rect.y, item_rect.w - 16.0, item_rect.h),
                text: option.clone(),
                color: self.theme.colors.text,
                font_size,
                clip: Some(item_rect),
            });

            // Handle click on option item — store selection for next frame
            for event in &self.events.clone() {
                if let InputEvent::PointerUp(ev) = event {
                    if item_rect.contains(ev.pos) && ev.button == Some(PointerButton::Left) {
                        self.dropdown_selection = Some((panel.id, i));
                        self.dropdown_open = None;
                    }
                }
            }

            // Register hit-test entry for the option
            self.hit_test.insert(HitTestEntry {
                id: {
                    let mut hasher = DefaultHasher::new();
                    panel.id.hash(&mut hasher);
                    i.hash(&mut hasher);
                    hasher.finish()
                },
                rect: item_rect,
            });
        }
    }

    fn handle_keyboard_navigation(&mut self) {
        let mut tab_pressed: Option<bool> = None;
        for event in &self.events {
            if let InputEvent::KeyDown { code: KeyCode::Tab, modifiers } = event {
                tab_pressed = Some(modifiers.shift);
            }
        }
        let shift = match tab_pressed {
            Some(value) => value,
            None => return,
        };
        if self.widgets.is_empty() {
            return;
        }
        let mut idx = self
            .widgets
            .iter()
            .position(|w| Some(w.id) == self.focused)
            .unwrap_or(0);
        if shift {
            if idx == 0 {
                idx = self.widgets.len() - 1;
            } else {
                idx -= 1;
            }
        } else {
            idx = (idx + 1) % self.widgets.len();
        }
        self.focused = Some(self.widgets[idx].id);
    }

    // -----------------------------------------------------------------
    // Layout: row containers
    // -----------------------------------------------------------------

    /// Begin a horizontal row. Widgets placed between `begin_row()` and
    /// `end_row()` will be laid out side by side with equal widths.
    pub fn begin_row(&mut self) {
        self.layout.begin_row();
    }

    /// Begin a horizontal row with proportional width weights.
    ///
    /// For example, `&[1.0, 2.0]` gives the first child 1/3 and the
    /// second child 2/3 of the available width.
    pub fn begin_row_with(&mut self, weights: &[f32]) {
        self.layout.begin_row_with(weights);
    }

    /// End the current horizontal row and resume vertical layout.
    /// If there is no matching `begin_row`, this is a no-op.
    pub fn end_row(&mut self) {
        self.layout.end_row();
    }

    // -----------------------------------------------------------------
    // Widgets
    // -----------------------------------------------------------------

    /// Emits a non-interactive text label at the current layout position.
    pub fn label(&mut self, text: &str) {
        let rect = self.layout.next_rect(24.0 * self.scale);
        let clip = self.effective_clip();
        self.widgets.push(WidgetInfo {
            id: self.hash_id(text),
            kind: WidgetKind::Label,
            label: text.to_string(),
            value: None,
            rect,
            state: A11yState::default(),
        });
        self.batch.text_runs.push(TextRun {
            rect,
            text: text.to_string(),
            color: self.theme.colors.text,
            font_size: 16.0 * self.theme.font_scale * self.scale,
            clip,
        });
    }

    /// Emits a non-interactive text label with an explicit `color` override.
    pub fn label_colored(&mut self, text: &str, color: Color) {
        let rect = self.layout.next_rect(20.0 * self.scale);
        let clip = self.effective_clip();
        self.batch.text_runs.push(TextRun {
            rect,
            text: text.to_string(),
            color,
            font_size: 14.0 * self.theme.font_scale * self.scale,
            clip,
        });
    }

    /// Set the icon pack used by the `icon()` widget.
    pub fn set_icon_pack(&mut self, pack: IconPack) {
        self.icon_pack = Some(pack);
    }

    /// Draw an icon. `size` is in logical pixels (scaled by `self.scale`).
    ///
    /// Returns the icon's bounding rect for layout purposes, or `None` if
    /// the icon was not found in the loaded icon pack.
    pub fn icon(&mut self, name: &str, size: f32) -> Option<Rect> {
        let pack = self.icon_pack.as_ref()?;
        let icon_id = pack.get(name)?;
        let entry = pack.entry(icon_id);
        let scaled = size * self.scale;
        let mut rect = self.layout.next_rect(scaled);
        // Use a square rect matching the icon size, not the full layout width.
        rect.w = scaled;

        // Snap to pixel grid for crisp rendering.
        rect.x = rect.x.round();
        rect.y = rect.y.round();

        self.batch.push_quad(
            Quad {
                rect,
                uv: entry.uv,
                color: self.theme.colors.text,
                flags: 2,
            },
            Material::IconAtlas,
            None,
        );
        Some(rect)
    }

    /// Draw an icon by `IconId`. `size` is in logical pixels.
    ///
    /// Returns the icon's bounding rect, or `None` if no icon pack is loaded.
    pub fn icon_by_id(&mut self, id: IconId, size: f32) -> Option<Rect> {
        let pack = self.icon_pack.as_ref()?;
        let entry = pack.entry(id);
        let scaled = size * self.scale;
        let mut rect = self.layout.next_rect(scaled);
        rect.w = scaled;

        rect.x = rect.x.round();
        rect.y = rect.y.round();

        self.batch.push_quad(
            Quad {
                rect,
                uv: entry.uv,
                color: self.theme.colors.text,
                flags: 2,
            },
            Material::IconAtlas,
            None,
        );
        Some(rect)
    }

    /// Emits a clickable button with the given `label`.
    ///
    /// Returns `true` on the frame that the button is clicked (pointer down then up
    /// within the button bounds). Also moves keyboard focus to the button on click.
    pub fn button(&mut self, label: &str) -> bool {
        let rect = self.layout.next_rect(40.0 * self.scale);
        let id = self.hash_id(label);
        let hovered = self.rect_hovered(id, rect);
        let pressed = self.rect_pressed(id, rect);
        let clicked = pressed && self.rect_released(id, rect);

        self.widgets.push(WidgetInfo {
            id,
            kind: WidgetKind::Button,
            label: label.to_string(),
            value: None,
            rect,
            state: A11yState {
                focused: self.focused == Some(id),
                disabled: false,
                invalid: false,
                required: false,
                expanded: false,
                selected: false,
            },
        });

        let bg = if pressed {
            self.theme.colors.primary
        } else if hovered {
            Color::rgba(
                self.theme.colors.primary.r,
                self.theme.colors.primary.g,
                self.theme.colors.primary.b,
                0.9,
            )
        } else {
            self.theme.colors.primary
        };

        let clip = self.effective_clip();
        self.batch.push_quad(
            Quad {
                rect,
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                color: bg,
                flags: 0,
            },
            Material::Solid,
            clip,
        );
        self.batch.text_runs.push(TextRun {
            rect,
            text: label.to_string(),
            color: Color::rgba(1.0, 1.0, 1.0, 1.0),
            font_size: 16.0 * self.theme.font_scale * self.scale,
            clip,
        });

        if clicked {
            self.focused = Some(id);
        }
        clicked
    }

    /// Emits a checkbox with the given `label`.
    ///
    /// Toggles `*value` on click and returns `true` on the frame the toggle occurs.
    pub fn checkbox(&mut self, label: &str, value: &mut bool) -> bool {
        let rect = self.layout.next_rect(32.0 * self.scale);
        let id = self.hash_id(label);
        let clicked = self.rect_pressed(id, rect) && self.rect_released(id, rect);
        if clicked {
            *value = !*value;
            self.focused = Some(id);
        }
        let clip = self.effective_clip();
        let box_rect = Rect::new(rect.x, rect.y, rect.h, rect.h);
        self.batch.push_quad(
            Quad {
                rect: box_rect,
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                color: self.theme.colors.surface,
                flags: 0,
            },
            Material::Solid,
            clip,
        );
        if *value {
            self.batch.push_quad(
                Quad {
                    rect: Rect::new(rect.x + 6.0, rect.y + 6.0, rect.h - 12.0, rect.h - 12.0),
                    uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                    color: self.theme.colors.primary,
                    flags: 0,
                },
                Material::Solid,
                clip,
            );
        }
        self.batch.text_runs.push(TextRun {
            rect: Rect::new(rect.x + rect.h + 8.0, rect.y, rect.w - rect.h, rect.h),
            text: label.to_string(),
            color: self.theme.colors.text,
            font_size: 15.0 * self.theme.font_scale * self.scale,
            clip,
        });

        self.widgets.push(WidgetInfo {
            id,
            kind: WidgetKind::Checkbox,
            label: label.to_string(),
            value: Some(value.to_string()),
            rect,
            state: A11yState {
                focused: self.focused == Some(id),
                disabled: false,
                invalid: false,
                required: false,
                expanded: false,
                selected: *value,
            },
        });

        clicked
    }

    /// Emits a dropdown select widget.
    ///
    /// Clicking the trigger opens a panel showing all `options`. Selecting an option
    /// writes it into `*value` and closes the panel. Returns `true` on the frame the
    /// selection changes.
    ///
    /// Supports keyboard navigation (ArrowUp/Down, Enter, Escape) and type-ahead
    /// when the widget is focused.
    pub fn select(&mut self, label: &str, options: &[String], value: &mut String) -> bool {
        let rect = self.layout.next_rect(36.0 * self.scale);
        let id = self.hash_id(label);
        let is_open = self.dropdown_open == Some(id);

        // --- Toggle open/close on click ---
        let clicked = self.rect_pressed(id, rect) && self.rect_released(id, rect);
        if clicked {
            self.focused = Some(id);
            if is_open {
                self.dropdown_open = None;
            } else {
                self.dropdown_open = Some(id);
                self.dropdown_highlighted = options
                    .iter()
                    .position(|o| o == value)
                    .unwrap_or(0);
            }
        }
        let is_open = self.dropdown_open == Some(id);

        // --- Check for deferred selection from panel click last frame ---
        let mut changed = false;
        if let Some((sel_id, sel_idx)) = self.dropdown_selection.take() {
            if sel_id == id {
                if let Some(opt) = options.get(sel_idx) {
                    *value = opt.clone();
                    changed = true;
                }
            }
        }

        // --- Keyboard & type-ahead when open and focused ---
        if is_open && self.focused == Some(id) {
            for event in &self.events.clone() {
                match event {
                    InputEvent::KeyDown { code, .. } => match code {
                        KeyCode::ArrowDown => {
                            if self.dropdown_highlighted + 1 < options.len() {
                                self.dropdown_highlighted += 1;
                            }
                        }
                        KeyCode::ArrowUp => {
                            if self.dropdown_highlighted > 0 {
                                self.dropdown_highlighted -= 1;
                            }
                        }
                        KeyCode::Enter => {
                            if let Some(opt) = options.get(self.dropdown_highlighted) {
                                *value = opt.clone();
                                changed = true;
                            }
                            self.dropdown_open = None;
                        }
                        KeyCode::Escape => {
                            self.dropdown_open = None;
                        }
                        _ => {}
                    },
                    InputEvent::TextInput(input) => {
                        // Type-ahead: jump to first option starting with typed char
                        let ch = input.text.to_lowercase();
                        if let Some(idx) = options.iter().position(|o| {
                            o.to_lowercase().starts_with(&ch)
                        }) {
                            self.dropdown_highlighted = idx;
                        }
                    }
                    _ => {}
                }
            }
        }

        // --- Render the select trigger ---
        let is_open = self.dropdown_open == Some(id);
        let clip = self.effective_clip();
        self.batch.push_quad(
            Quad {
                rect,
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                color: self.theme.colors.surface,
                flags: 0,
            },
            Material::Solid,
            clip,
        );
        let display = format!("{}: {}", label, value);
        self.batch.text_runs.push(TextRun {
            rect: Rect::new(rect.x + 8.0, rect.y, rect.w - 32.0, rect.h),
            text: display,
            color: self.theme.colors.text,
            font_size: 15.0 * self.theme.font_scale * self.scale,
            clip,
        });
        // Down-arrow indicator
        let arrow = if is_open { "\u{25B2}" } else { "\u{25BC}" };
        self.batch.text_runs.push(TextRun {
            rect: Rect::new(rect.x + rect.w - 24.0, rect.y, 16.0, rect.h),
            text: arrow.to_string(),
            color: self.theme.colors.text_muted,
            font_size: 12.0 * self.theme.font_scale * self.scale,
            clip,
        });

        self.widgets.push(WidgetInfo {
            id,
            kind: WidgetKind::Select,
            label: label.to_string(),
            value: Some(value.clone()),
            rect,
            state: A11yState {
                focused: self.focused == Some(id),
                disabled: false,
                invalid: false,
                required: false,
                expanded: is_open,
                selected: true,
            },
        });

        // --- Schedule dropdown panel for deferred rendering ---
        if is_open {
            let visible = options.len().min(DROPDOWN_MAX_VISIBLE);
            let panel_h = visible as f32 * DROPDOWN_ITEM_HEIGHT * self.scale;
            let panel_rect = Rect::new(rect.x, rect.y + rect.h, rect.w, panel_h);
            self.dropdown_panel = Some(DropdownPanel {
                id,
                trigger_rect: rect,
                panel_rect,
                options: options.to_vec(),
                highlighted: self.dropdown_highlighted,
                _current_value: value.clone(),
            });
        }

        changed
    }

    /// Emits a radio button group with a group `label` followed by one radio per option.
    ///
    /// `*selected` is the index of the currently selected option. Returns `true` on
    /// the frame the selection changes.
    pub fn radio_group(&mut self, label: &str, options: &[String], selected: &mut usize) -> bool {
        self.ui_label_inline(label);
        let mut changed = false;
        for (idx, option) in options.iter().enumerate() {
            let rect = self.layout.next_rect(28.0 * self.scale);
            let id = self.hash_id(&format!("{}-{}", label, idx));
            let clicked = self.rect_pressed(id, rect) && self.rect_released(id, rect);
            if clicked {
                *selected = idx;
                self.focused = Some(id);
                changed = true;
            }
            let radius = rect.h * 0.35;
            let center = rect.center();
            let outer = Rect::new(center.x - radius, center.y - radius, radius * 2.0, radius * 2.0);
            let clip = self.effective_clip();
            self.batch.push_quad(
                Quad {
                    rect: outer,
                    uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                    color: self.theme.colors.surface,
                    flags: 0,
                },
                Material::Solid,
                clip,
            );
            if *selected == idx {
                self.batch.push_quad(
                    Quad {
                        rect: Rect::new(center.x - radius * 0.5, center.y - radius * 0.5, radius, radius),
                        uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                        color: self.theme.colors.primary,
                        flags: 0,
                    },
                    Material::Solid,
                    clip,
                );
            }
            self.batch.text_runs.push(TextRun {
                rect: Rect::new(rect.x + rect.h + 8.0, rect.y, rect.w - rect.h, rect.h),
                text: option.to_string(),
                color: self.theme.colors.text,
                font_size: 14.0 * self.theme.font_scale * self.scale,
                clip,
            });
            self.widgets.push(WidgetInfo {
                id,
                kind: WidgetKind::Radio,
                label: option.to_string(),
                value: Some(option.to_string()),
                rect,
                state: A11yState {
                    focused: self.focused == Some(id),
                    disabled: false,
                    invalid: false,
                    required: false,
                    expanded: false,
                    selected: *selected == idx,
                },
            });
        }
        changed
    }

    /// Emits a single-line text input widget.
    ///
    /// `buffer` holds the mutable text state. `placeholder` is shown when the buffer
    /// is empty. Returns `true` on the frame the widget is clicked (gains focus).
    ///
    /// The caller is responsible for managing the `TextBuffer`. For form-integrated
    /// inputs that automatically sync with a [`Form`](crate::form::Form), use
    /// [`text_input_for`](Self::text_input_for) instead.
    pub fn text_input(&mut self, label: &str, buffer: &mut TextBuffer, placeholder: &str) -> bool {
        self.text_input_impl(label, buffer, placeholder, false, false, 40.0 * self.scale)
    }

    /// Emits a password-masked text input (characters displayed as bullets).
    ///
    /// Otherwise identical to [`text_input`](Self::text_input).
    pub fn text_input_masked(&mut self, label: &str, buffer: &mut TextBuffer, placeholder: &str) -> bool {
        self.text_input_impl(label, buffer, placeholder, false, true, 40.0 * self.scale)
    }

    /// Emits a multiline text input with the given fixed `height` in logical pixels.
    pub fn text_input_multiline(
        &mut self,
        label: &str,
        buffer: &mut TextBuffer,
        placeholder: &str,
        height: f32,
    ) -> bool {
        self.text_input_impl(label, buffer, placeholder, true, false, height)
    }

    /// Auto-binding text input: creates/reuses an internal `TextBuffer`
    /// keyed by `path`, initializes it from the form state if new, runs
    /// the standard `text_input` logic, and syncs changes back to the form.
    ///
    /// Returns `true` if the field was clicked (gained focus).
    pub fn text_input_for(
        &mut self,
        form: &mut Form,
        path: &FormPath,
        label: &str,
        placeholder: &str,
    ) -> bool {
        self.text_input_for_impl(form, path, label, placeholder, false)
    }

    /// Masked variant of [`text_input_for`](Self::text_input_for) (password
    /// fields).
    pub fn text_input_masked_for(
        &mut self,
        form: &mut Form,
        path: &FormPath,
        label: &str,
        placeholder: &str,
    ) -> bool {
        self.text_input_for_impl(form, path, label, placeholder, true)
    }

    /// Shared implementation for `text_input_for` / `text_input_masked_for`.
    fn text_input_for_impl(
        &mut self,
        form: &mut Form,
        path: &FormPath,
        label: &str,
        placeholder: &str,
        masked: bool,
    ) -> bool {
        // Ensure a buffer exists for this path, initialized from form state.
        if !self.form_buffers.contains_key(path) {
            let initial = form
                .state()
                .get_field(path)
                .and_then(|fs| match &fs.value {
                    FieldValue::Text(s) => Some(s.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            self.form_buffers.insert(path.clone(), TextBuffer::new(initial));
        }

        // Temporarily remove the buffer so we can pass `&mut self` and
        // `&mut buffer` independently to `text_input_impl`.
        let mut buf = self.form_buffers.remove(path).unwrap();
        let before = buf.text().to_string();

        let scale = self.scale;
        let clicked = self.text_input_impl(label, &mut buf, placeholder, false, masked, 40.0 * scale);

        let after = buf.text().to_string();

        // Put the buffer back.
        self.form_buffers.insert(path.clone(), buf);

        if after != before {
            form.set_value(path, FieldValue::Text(after));
        }

        clicked
    }

    fn text_input_impl(
        &mut self,
        label: &str,
        buffer: &mut TextBuffer,
        placeholder: &str,
        multiline: bool,
        masked: bool,
        height: f32,
    ) -> bool {
        let rect = self.layout.next_rect(height);
        let id = self.hash_id(label);
        let clicked = self.rect_pressed(id, rect) && self.rect_released(id, rect);
        if clicked {
            self.focused = Some(id);
        }
        let focused = self.focused == Some(id);
        if focused {
            self.apply_text_events(buffer, multiline);
            self.apply_pointer_selection(id, rect, buffer);
        }

        let outer_clip = self.effective_clip();
        let inner_clip = self.merge_clip(Some(rect));
        self.batch.push_quad(
            Quad {
                rect,
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                color: self.theme.colors.surface,
                flags: 0,
            },
            Material::Solid,
            outer_clip,
        );
        let content = if buffer.text().is_empty() {
            placeholder.to_string()
        } else if masked {
            "\u{2022}".repeat(buffer.grapheme_len())
        } else {
            buffer.text().to_string()
        };
        let color = if buffer.text().is_empty() {
            self.theme.colors.text_muted
        } else {
            self.theme.colors.text
        };
        if focused {
            self.draw_selection(rect, buffer, multiline);
        }

        self.batch.text_runs.push(TextRun {
            rect: Rect::new(rect.x + 8.0, rect.y, rect.w - 16.0, rect.h),
            text: content,
            color,
            font_size: 15.0 * self.theme.font_scale * self.scale,
            clip: inner_clip,
        });

        if focused {
            let show_caret = (self.time_ms as u64 / 500).is_multiple_of(2);
            if show_caret {
                let caret_pos = self.index_to_position(rect, buffer, buffer.caret().index, multiline);
                let caret_rect = Rect::new(caret_pos.x, caret_pos.y, 1.5, 18.0 * self.scale);
                self.batch.push_quad(
                    Quad {
                        rect: caret_rect,
                        uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                        color: self.theme.colors.text,
                        flags: 0,
                    },
                    Material::Solid,
                    inner_clip,
                );
            }
        }

        self.widgets.push(WidgetInfo {
            id,
            kind: WidgetKind::TextInput,
            label: label.to_string(),
            value: Some(buffer.text().to_string()),
            rect,
            state: A11yState {
                focused,
                disabled: false,
                invalid: false,
                required: false,
                expanded: false,
                selected: false,
            },
        });

        clicked
    }

    /// Begin a scrollable container. Widgets between `begin_scroll` and
    /// `end_scroll` are clipped to the container rect and offset by the
    /// current scroll position. Returns the scroll container ID.
    pub fn begin_scroll(&mut self, label: &str, height: f32) -> u64 {
        let rect = self.layout.next_rect(height);
        let id = self.hash_id(label);

        // Get or create scroll state
        let state = self.scroll_states.entry(id).or_default();
        state.container_height = height;

        // --- Inertia ---
        let dt = ((self.time_ms - self.last_time_ms) / 1000.0) as f32;
        if dt > 0.0 && state.velocity.abs() > 0.5 {
            state.offset += state.velocity * dt;
            state.velocity *= 0.92_f32.powf(dt * 60.0); // friction
            if state.velocity.abs() < 0.5 {
                state.velocity = 0.0;
            }
            state.clamp_offset();
        }

        // --- Wheel events ---
        for event in &self.events.clone() {
            if let InputEvent::PointerWheel { pos, delta, .. } = event {
                if rect.contains(*pos) {
                    // Check no inner scroll container already consumed this
                    let state = self.scroll_states.get_mut(&id).unwrap();
                    state.offset += delta.y;
                    state.velocity = 0.0; // cancel inertia on direct scroll
                    state.clamp_offset();
                }
            }
        }

        // --- Touch drag scrolling ---
        for event in &self.events.clone() {
            match event {
                InputEvent::PointerDown(ev)
                    if rect.contains(ev.pos) && ev.button == Some(PointerButton::Left) =>
                {
                    let state = self.scroll_states.get_mut(&id).unwrap();
                    state.velocity = 0.0;
                }
                InputEvent::PointerMove(ev) if self.dragging.is_none() || self.active.is_none() => {
                    // Only scroll if not dragging a child widget
                    // Touch velocity is tracked but actual scroll happens via wheel on web
                }
                _ => {}
            }
        }

        let scroll_offset = self.scroll_states.get(&id).unwrap().offset;

        // Render container background
        self.batch.push_quad(
            Quad {
                rect,
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                color: Color::rgba(
                    self.theme.colors.background.r,
                    self.theme.colors.background.g,
                    self.theme.colors.background.b,
                    0.5,
                ),
                flags: 0,
            },
            Material::Solid,
            self.active_clip,
        );

        // Register widget
        self.widgets.push(WidgetInfo {
            id,
            kind: WidgetKind::ScrollContainer,
            label: label.to_string(),
            value: None,
            rect,
            state: A11yState::default(),
        });

        // Save outer layout
        self.layout_stack.push(self.layout.clone());

        // Set up inner layout: starts at container top, offset by scroll
        self.layout = Layout::new(rect.x, rect.y - scroll_offset, rect.w);

        // Push clip rect (intersect with parent clip for nesting)
        let clip = if let Some(parent_clip) = self.active_clip {
            parent_clip.intersect(rect).unwrap_or(rect)
        } else {
            rect
        };
        self.clip_stack.push(clip);
        self.active_clip = Some(clip);

        // Push ID for children
        self.push_id(label);

        id
    }

    /// End a scrollable container. Records content height and optionally
    /// renders a scrollbar.
    pub fn end_scroll(&mut self) {
        // Pop ID
        self.pop_id();

        // Measure content height from inner layout cursor
        let content_bottom = self.layout.cursor.y;

        // Restore outer layout
        if let Some(outer) = self.layout_stack.pop() {
            // Calculate content height relative to container top
            // The inner layout started at (rect.y - scroll_offset), so
            // content_height = cursor.y - (rect.y - scroll_offset)
            // But we need the original rect. We can get it from clip_stack.
            let clip = self.clip_stack.last().copied();
            if let Some(clip_rect) = clip {
                let scroll_id = {
                    // Find the scroll container widget that matches this clip
                    self.widgets.iter().rev()
                        .find(|w| w.kind == WidgetKind::ScrollContainer)
                        .map(|w| (w.id, w.rect))
                };
                if let Some((id, container_rect)) = scroll_id {
                    if let Some(state) = self.scroll_states.get_mut(&id) {
                        state.content_height = content_bottom - (container_rect.y - state.offset);
                        state.clamp_offset();

                        // Render scrollbar if content overflows
                        if state.content_height > state.container_height {
                            let track_w = 6.0;
                            let track_rect = Rect::new(
                                container_rect.x + container_rect.w - track_w - 2.0,
                                container_rect.y,
                                track_w,
                                container_rect.h,
                            );
                            // Track background
                            self.batch.push_quad(
                                Quad {
                                    rect: track_rect,
                                    uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                                    color: Color::rgba(0.0, 0.0, 0.0, 0.05),
                                    flags: 0,
                                },
                                Material::Solid,
                                Some(clip_rect),
                            );
                            // Thumb
                            let ratio = state.container_height / state.content_height;
                            let thumb_h = (ratio * container_rect.h).max(20.0);
                            let scroll_range = state.content_height - state.container_height;
                            let thumb_y = if scroll_range > 0.0 {
                                container_rect.y
                                    + (state.offset / scroll_range)
                                        * (container_rect.h - thumb_h)
                            } else {
                                container_rect.y
                            };
                            self.batch.push_quad(
                                Quad {
                                    rect: Rect::new(
                                        track_rect.x,
                                        thumb_y,
                                        track_w,
                                        thumb_h,
                                    ),
                                    uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                                    color: Color::rgba(0.0, 0.0, 0.0, 0.2),
                                    flags: 0,
                                },
                                Material::Solid,
                                Some(clip_rect),
                            );
                        }
                    }
                }
            }

            self.layout = outer;
        }

        // Pop clip
        self.clip_stack.pop();
        self.active_clip = self.clip_stack.last().copied();
    }

    /// Displays a tooltip `text` near the top-right corner when the widget
    /// identified by `target_label` is hovered. Call this immediately after
    /// the widget it annotates.
    pub fn tooltip(&mut self, target_label: &str, text: &str) {
        let id = self.hash_id(target_label);
        if self.hovered != Some(id) {
            return;
        }
        let rect = Rect::new(self.layout.width - 240.0, 16.0, 220.0, 60.0);
        self.batch.push_quad(
            Quad {
                rect,
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                color: Color::rgba(0.1, 0.1, 0.12, 0.9),
                flags: 0,
            },
            Material::Solid,
            None,
        );
        self.batch.text_runs.push(TextRun {
            rect: Rect::new(rect.x + 8.0, rect.y + 8.0, rect.w - 16.0, rect.h - 16.0),
            text: text.to_string(),
            color: Color::rgba(1.0, 1.0, 1.0, 1.0),
            font_size: 13.0 * self.theme.font_scale * self.scale,
            clip: None,
        });
    }

    fn apply_text_events(&mut self, buffer: &mut TextBuffer, allow_newline: bool) {
        for event in &self.events {
            match event {
                InputEvent::TextInput(input) => {
                    if allow_newline || !input.text.contains('\n') {
                        if self.overwrite_mode && buffer.selection().is_none() {
                            // Overwrite mode: delete the character under the caret first.
                            buffer.delete_forward();
                        }
                        buffer.insert_text(&input.text);
                    }
                }
                InputEvent::KeyDown { code, modifiers } => match code {
                    // -------------------------------------------------------
                    // Deletion
                    // -------------------------------------------------------
                    KeyCode::Backspace if modifiers.ctrl || modifiers.alt => {
                        buffer.delete_word_backward();
                    }
                    KeyCode::Backspace => {
                        buffer.delete_backward();
                    }
                    KeyCode::Delete if modifiers.ctrl || modifiers.alt => {
                        buffer.delete_word_forward();
                    }
                    KeyCode::Delete => {
                        buffer.delete_forward();
                    }
                    // -------------------------------------------------------
                    // Horizontal movement
                    // -------------------------------------------------------
                    KeyCode::ArrowLeft if modifiers.ctrl || modifiers.alt => {
                        buffer.move_word_left(modifiers.shift);
                    }
                    KeyCode::ArrowLeft => {
                        // If there is a selection and shift is NOT held, collapse
                        // to the left edge (standard platform behaviour).
                        if buffer.selection().map(|s| !s.is_empty()).unwrap_or(false)
                            && !modifiers.shift
                        {
                            let sel = buffer.selection().unwrap().normalized();
                            buffer.set_caret(sel.start);
                        } else {
                            buffer.move_left(modifiers.shift);
                        }
                    }
                    KeyCode::ArrowRight if modifiers.ctrl || modifiers.alt => {
                        buffer.move_word_right(modifiers.shift);
                    }
                    KeyCode::ArrowRight => {
                        if buffer.selection().map(|s| !s.is_empty()).unwrap_or(false)
                            && !modifiers.shift
                        {
                            let sel = buffer.selection().unwrap().normalized();
                            buffer.set_caret(sel.end);
                        } else {
                            buffer.move_right(modifiers.shift);
                        }
                    }
                    // -------------------------------------------------------
                    // Vertical movement (multiline)
                    // -------------------------------------------------------
                    KeyCode::ArrowUp => {
                        // TODO: implement true line-up movement using
                        // index_to_position / position_to_index.
                        // For now fall through to Home as a safe stub.
                        buffer.move_to(0, modifiers.shift);
                    }
                    KeyCode::ArrowDown => {
                        // TODO: implement true line-down movement.
                        let len = buffer.grapheme_len();
                        buffer.move_to(len, modifiers.shift);
                    }
                    // -------------------------------------------------------
                    // Line start / end
                    // -------------------------------------------------------
                    KeyCode::Home if modifiers.ctrl => buffer.move_to(0, modifiers.shift),
                    KeyCode::Home => {
                        // Move to start of the current logical line.
                        buffer.move_to_line_start(modifiers.shift);
                    }
                    KeyCode::End if modifiers.ctrl => {
                        let len = buffer.grapheme_len();
                        buffer.move_to(len, modifiers.shift);
                    }
                    KeyCode::End => {
                        // Move to end of the current logical line.
                        buffer.move_to_line_end(modifiers.shift);
                    }
                    // -------------------------------------------------------
                    // Newline
                    // -------------------------------------------------------
                    KeyCode::Enter => {
                        if allow_newline {
                            buffer.insert_text("\n");
                        }
                    }
                    // -------------------------------------------------------
                    // Overwrite toggle
                    // -------------------------------------------------------
                    KeyCode::Insert => {
                        self.overwrite_mode = !self.overwrite_mode;
                    }
                    // -------------------------------------------------------
                    // Clipboard shortcuts
                    // -------------------------------------------------------
                    KeyCode::A if modifiers.ctrl || modifiers.meta => buffer.select_all(),
                    KeyCode::C if modifiers.ctrl || modifiers.meta => {
                        if let Some(text) = buffer.selected_text() {
                            self.clipboard_request = Some(text);
                        }
                    }
                    KeyCode::X if modifiers.ctrl || modifiers.meta => {
                        // cut_selection() atomically returns the text AND
                        // removes it in a single undo entry.
                        if let Some(text) = buffer.cut_selection() {
                            self.clipboard_request = Some(text);
                        }
                    }
                    // -------------------------------------------------------
                    // Undo / redo
                    // -------------------------------------------------------
                    KeyCode::Z if modifiers.ctrl || modifiers.meta => {
                        if modifiers.shift {
                            buffer.redo();
                        } else {
                            buffer.undo();
                        }
                    }
                    KeyCode::Y if modifiers.ctrl || modifiers.meta => {
                        buffer.redo();
                    }
                    _ => {}
                },
                // -----------------------------------------------------------
                // IME composition
                // -----------------------------------------------------------
                InputEvent::CompositionStart => {
                    buffer.begin_composition();
                }
                InputEvent::CompositionUpdate(text) => {
                    buffer.update_composition(text);
                }
                InputEvent::CompositionEnd(text) => {
                    buffer.end_composition(text);
                }
                // -----------------------------------------------------------
                // Paste from host clipboard (JS side calls handle_paste)
                // -----------------------------------------------------------
                InputEvent::Paste(text) => {
                    buffer.insert_text(text);
                }
                _ => {}
            }
        }
    }

    fn apply_pointer_selection(&mut self, id: u64, rect: Rect, buffer: &mut TextBuffer) {
        /// Maximum ms gap between clicks to count as a multi-click sequence.
        const DOUBLE_CLICK_MS: f64 = 400.0;

        // Collect only the pointer events we need into a small local buffer,
        // avoiding a full clone of self.events.
        #[derive(Clone, Copy)]
        enum PointerAction {
            Down(Vec2),
            Move(Vec2),
            Up,
        }

        let actions: Vec<PointerAction> = self
            .events
            .iter()
            .filter_map(|event| match event {
                InputEvent::PointerDown(ev)
                    if rect.contains(ev.pos) && ev.button == Some(PointerButton::Left) =>
                {
                    Some(PointerAction::Down(ev.pos))
                }
                InputEvent::PointerMove(ev) => Some(PointerAction::Move(ev.pos)),
                InputEvent::PointerUp(ev) if ev.button == Some(PointerButton::Left) => {
                    Some(PointerAction::Up)
                }
                _ => None,
            })
            .collect();

        for action in &actions {
            match *action {
                PointerAction::Down(pos) => {
                    let idx = self.position_to_index(rect, buffer, pos);

                    // --- Multi-click detection ---
                    let same_target = self.last_click_id == Some(id);
                    let within_time = (self.time_ms - self.last_click_time) < DOUBLE_CLICK_MS;
                    if same_target && within_time {
                        self.click_count = self.click_count.saturating_add(1);
                    } else {
                        self.click_count = 1;
                    }
                    self.last_click_time = self.time_ms;
                    self.last_click_id = Some(id);

                    match self.click_count {
                        1 => {
                            // Single click: place caret, begin drag.
                            buffer.set_caret(idx);
                            self.dragging = Some(id);
                            self.selection_anchor = Some(idx);
                        }
                        2 => {
                            // Double click: select the word at the click position.
                            buffer.select_word_at(idx);
                            self.dragging = None; // no drag after word-select
                            self.selection_anchor = None;
                        }
                        _ => {
                            // Triple (or more) click: select the whole logical line.
                            buffer.select_line_at(idx);
                            self.dragging = None;
                            self.selection_anchor = None;
                        }
                    }
                }
                PointerAction::Move(pos) => {
                    if self.dragging == Some(id) {
                        let idx = self.position_to_index(rect, buffer, pos);
                        let start = self.selection_anchor.unwrap_or(buffer.caret().index);
                        buffer.set_selection(start, idx);
                        // TODO: if pos is outside rect horizontally, nudge
                        // self.scroll_offsets[id] to auto-scroll the viewport.
                    }
                }
                PointerAction::Up => {
                    if self.dragging == Some(id) {
                        self.dragging = None;
                        self.selection_anchor = None;
                    }
                }
            }
        }
    }

    fn position_to_index(&self, rect: Rect, buffer: &TextBuffer, pos: Vec2) -> usize {
        let padding = 8.0;
        let font_size = 15.0 * self.theme.font_scale * self.scale;
        let line_height = font_size * 1.4;
        let x = (pos.x - rect.x - padding).max(0.0);
        let y = (pos.y - rect.y - padding).max(0.0);
        let line = (y / line_height).floor() as usize;
        let mut index = 0usize;
        for (line_idx, line_text) in buffer.text().split('\n').enumerate() {
            let graphemes = line_text.graphemes(true).count();
            if line_idx == line {
                // Use prefix sums to find which grapheme boundary the click
                // falls closest to (midpoint rounding).
                let sums = self.grapheme_prefix_sums(line_text, font_size);
                let col = sums
                    .windows(2)
                    .position(|w| x < w[0] + (w[1] - w[0]) * 0.5)
                    .unwrap_or(graphemes);
                index += col;
                return index;
            }
            index += graphemes + 1;
        }
        buffer.grapheme_len()
    }

    fn index_to_position(&self, rect: Rect, buffer: &TextBuffer, index: usize, _multiline: bool) -> Vec2 {
        let padding = 8.0;
        let font_size = 15.0 * self.theme.font_scale * self.scale;
        let line_height = font_size * 1.4;
        let mut remaining = index;
        for (line, line_text) in buffer.text().split('\n').enumerate() {
            let graphemes = line_text.graphemes(true).count();
            if remaining <= graphemes {
                let sums = self.grapheme_prefix_sums(line_text, font_size);
                let x = rect.x + padding + sums[remaining];
                let y = rect.y + padding + line as f32 * line_height;
                return Vec2::new(x, y);
            }
            remaining = remaining.saturating_sub(graphemes + 1);
        }
        Vec2::new(rect.x + padding, rect.y + padding)
    }

    fn draw_selection(&mut self, rect: Rect, buffer: &TextBuffer, _multiline: bool) {
        let selection = match buffer.selection() {
            Some(sel) if !sel.is_empty() => sel.normalized(),
            _ => return,
        };
        let font_size = 15.0 * self.theme.font_scale * self.scale;
        let line_height = font_size * 1.4;
        let padding = 8.0;
        let lines: Vec<&str> = buffer.text().split('\n').collect();
        let (start_line, start_col) = self.index_to_line_col(&lines, selection.start);
        let (end_line, end_col) = self.index_to_line_col(&lines, selection.end);

        for line in start_line..=end_line {
            let line_text = lines.get(line).copied().unwrap_or("");
            let line_len = line_text.graphemes(true).count();
            let (col_start, col_end) = if line == start_line && line == end_line {
                (start_col, end_col)
            } else if line == start_line {
                (start_col, line_len)
            } else if line == end_line {
                (0, end_col)
            } else {
                (0, line_len)
            };
            if col_start == col_end {
                continue;
            }
            let sums = self.grapheme_prefix_sums(line_text, font_size);
            let x = rect.x + padding + sums[col_start];
            let y = rect.y + padding + line as f32 * line_height;
            let w = sums[col_end] - sums[col_start];
            let sel_rect = Rect::new(x, y, w, line_height);
            self.batch.push_quad(
                Quad {
                    rect: sel_rect,
                    uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                    color: Color::rgba(0.2, 0.45, 0.9, 0.25),
                    flags: 0,
                },
                Material::Solid,
                self.merge_clip(Some(rect)),
            );
        }
    }

    fn index_to_line_col(&self, lines: &[&str], mut index: usize) -> (usize, usize) {
        for (line_idx, line) in lines.iter().enumerate() {
            let count = line.graphemes(true).count();
            if index <= count {
                return (line_idx, index);
            }
            index = index.saturating_sub(count + 1);
        }
        let last = lines.len().saturating_sub(1);
        (last, 0)
    }

    /// Expand a visual rect to meet the minimum touch target size when in
    /// touch mode. On desktop the rect is returned unchanged. The expanded
    /// rect is centred on the original.
    fn touch_rect(&self, rect: Rect) -> Rect {
        if !self.touch_mode {
            return rect;
        }
        let min = MIN_TOUCH_TARGET * self.scale;
        let w = rect.w.max(min);
        let h = rect.h.max(min);
        Rect::new(
            rect.x - (w - rect.w) * 0.5,
            rect.y - (h - rect.h) * 0.5,
            w,
            h,
        )
    }

    /// Returns the effective clip rect for the current context. When inside
    /// a scroll container, this is the container's clip rect. Otherwise None.
    fn effective_clip(&self) -> Option<Rect> {
        self.active_clip
    }

    /// Merge a widget-specific clip with the scroll container clip.
    fn merge_clip(&self, widget_clip: Option<Rect>) -> Option<Rect> {
        match (widget_clip, self.active_clip) {
            (Some(wc), Some(sc)) => sc.intersect(wc).or(Some(sc)),
            (Some(wc), None) => Some(wc),
            (None, Some(sc)) => Some(sc),
            (None, None) => None,
        }
    }

    fn rect_hovered(&mut self, id: u64, rect: Rect) -> bool {
        let hit = self.touch_rect(rect);
        let mut hovered = false;
        for event in &self.events {
            if let InputEvent::PointerMove(ev) = event {
                if hit.contains(ev.pos) {
                    hovered = true;
                }
            }
        }
        if hovered {
            self.hovered = Some(id);
        }
        hovered
    }

    fn rect_pressed(&mut self, id: u64, rect: Rect) -> bool {
        let hit = self.touch_rect(rect);
        for event in &self.events {
            if let InputEvent::PointerDown(ev) = event {
                if hit.contains(ev.pos) && ev.button == Some(PointerButton::Left) {
                    self.active = Some(id);
                    return true;
                }
            }
        }
        false
    }

    fn rect_released(&mut self, id: u64, rect: Rect) -> bool {
        let hit = self.touch_rect(rect);
        for event in &self.events {
            if let InputEvent::PointerUp(ev) = event {
                if hit.contains(ev.pos) && ev.button == Some(PointerButton::Left) {
                    if self.active == Some(id) {
                        self.active = None;
                    }
                    return true;
                }
            }
        }
        false
    }

    fn hash_id(&self, label: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        for &stack_val in &self.id_stack {
            stack_val.hash(&mut hasher);
        }
        label.hash(&mut hasher);
        hasher.finish()
    }

    /// Push a value onto the ID stack. All subsequent `hash_id` calls will
    /// incorporate this value, making widget IDs unique even when labels repeat
    /// (e.g. inside a loop). Must be paired with [`pop_id`].
    pub fn push_id(&mut self, id: impl Hash) {
        let mut hasher = DefaultHasher::new();
        id.hash(&mut hasher);
        self.id_stack.push(hasher.finish());
    }

    /// Pop the most recent value from the ID stack.
    pub fn pop_id(&mut self) {
        self.id_stack.pop();
    }

}

fn widget_role(kind: WidgetKind) -> A11yRole {
    match kind {
        WidgetKind::Label => A11yRole::Label,
        WidgetKind::Button => A11yRole::Button,
        WidgetKind::Checkbox => A11yRole::CheckBox,
        WidgetKind::Radio => A11yRole::RadioButton,
        WidgetKind::TextInput => A11yRole::TextBox,
        WidgetKind::Select => A11yRole::ComboBox,
        WidgetKind::Group => A11yRole::Group,
        WidgetKind::ScrollContainer => A11yRole::Group,
    }
}

impl Ui {
    /// Returns the bounding rect of the currently focused widget, if any.
    pub fn focused_widget_rect(&self) -> Option<Rect> {
        let focused_id = self.focused?;
        self.widgets
            .iter()
            .find(|w| w.id == focused_id)
            .map(|w| w.rect)
    }

    /// Returns the kind of the currently focused widget, if any.
    pub fn focused_widget_kind(&self) -> Option<WidgetKind> {
        let focused_id = self.focused?;
        self.widgets
            .iter()
            .find(|w| w.id == focused_id)
            .map(|w| w.kind)
    }

    /// Set focus to the widget with the given ID.
    ///
    /// Used by the accessibility mirror to synchronize screen reader focus
    /// back into the canvas UI.  If no widget matches the given ID, focus
    /// is cleared.
    pub fn set_focus_by_id(&mut self, id: u64) {
        if self.widgets.iter().any(|w| w.id == id) {
            self.focused = Some(id);
        } else {
            self.focused = None;
        }
    }

    fn ui_label_inline(&mut self, text: &str) {
        let rect = self.layout.next_rect(22.0 * self.scale);
        self.batch.text_runs.push(TextRun {
            rect,
            text: text.to_string(),
            color: self.theme.colors.text,
            font_size: 13.0 * self.theme.font_scale * self.scale,
            clip: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Modifiers, PointerEvent, TextInputEvent};
    use crate::theme::Theme;

    fn test_ui() -> Ui {
        Ui::new(800.0, 600.0, Theme::default_light())
    }

    // -----------------------------------------------------------------------
    // ID stack
    // -----------------------------------------------------------------------

    #[test]
    fn hash_id_same_label_same_id() {
        let ui = test_ui();
        let id1 = ui.hash_id("my_button");
        let id2 = ui.hash_id("my_button");
        assert_eq!(id1, id2);
    }

    #[test]
    fn hash_id_different_labels_different_ids() {
        let ui = test_ui();
        let id1 = ui.hash_id("button_a");
        let id2 = ui.hash_id("button_b");
        assert_ne!(id1, id2);
    }

    #[test]
    fn push_id_changes_hash() {
        let mut ui = test_ui();
        let id_before = ui.hash_id("label");
        ui.push_id(0u32);
        let id_with_stack = ui.hash_id("label");
        ui.pop_id();
        assert_ne!(id_before, id_with_stack);
    }

    #[test]
    fn pop_id_restores_original_hash() {
        let mut ui = test_ui();
        let id_before = ui.hash_id("label");
        ui.push_id(42u32);
        ui.pop_id();
        let id_after = ui.hash_id("label");
        assert_eq!(id_before, id_after);
    }

    #[test]
    fn different_stack_values_produce_different_ids() {
        let mut ui = test_ui();
        ui.push_id(0u32);
        let id0 = ui.hash_id("item");
        ui.pop_id();

        ui.push_id(1u32);
        let id1 = ui.hash_id("item");
        ui.pop_id();

        assert_ne!(id0, id1);
    }

    #[test]
    fn nested_push_id() {
        let mut ui = test_ui();
        ui.push_id("group_a");
        ui.push_id(0u32);
        let id_a0 = ui.hash_id("field");
        ui.pop_id();
        ui.push_id(1u32);
        let id_a1 = ui.hash_id("field");
        ui.pop_id();
        ui.pop_id();

        assert_ne!(id_a0, id_a1);
    }

    #[test]
    fn same_path_same_id_across_frames() {
        let mut ui = test_ui();
        ui.push_id(5u32);
        let id_frame1 = ui.hash_id("widget");
        ui.pop_id();

        // Simulate a new frame
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.push_id(5u32);
        let id_frame2 = ui.hash_id("widget");
        ui.pop_id();

        assert_eq!(id_frame1, id_frame2);
    }

    // -----------------------------------------------------------------------
    // Focus management (Tab / Shift+Tab)
    // -----------------------------------------------------------------------

    #[test]
    fn tab_advances_focus() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.label("Label 1");
        ui.label("Label 2");
        ui.label("Label 3");
        // No focus initially
        assert!(ui.focused.is_none());

        // Simulate Tab press
        let tab_event = InputEvent::KeyDown {
            code: KeyCode::Tab,
            modifiers: Modifiers { shift: false, ctrl: false, alt: false, meta: false },
        };
        ui.events = vec![tab_event];
        ui.end_frame();

        // Focus should now be on one of the widgets
        assert!(ui.focused.is_some());
    }

    #[test]
    fn shift_tab_reverses_focus() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.label("A");
        ui.label("B");
        ui.label("C");

        // Set focus to second widget
        let second_id = ui.widgets[1].id;
        ui.focused = Some(second_id);

        // Simulate Shift+Tab
        let shift_tab = InputEvent::KeyDown {
            code: KeyCode::Tab,
            modifiers: Modifiers { shift: true, ctrl: false, alt: false, meta: false },
        };
        ui.events = vec![shift_tab];
        ui.end_frame();

        // Focus should move to first widget
        assert_eq!(ui.focused, Some(ui.widgets[0].id));
    }

    #[test]
    fn tab_wraps_around() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.label("A");
        ui.label("B");

        // Set focus to last widget
        let last_id = ui.widgets[1].id;
        ui.focused = Some(last_id);

        let tab_event = InputEvent::KeyDown {
            code: KeyCode::Tab,
            modifiers: Modifiers { shift: false, ctrl: false, alt: false, meta: false },
        };
        ui.events = vec![tab_event];
        ui.end_frame();

        // Focus should wrap to first widget
        assert_eq!(ui.focused, Some(ui.widgets[0].id));
    }

    // -----------------------------------------------------------------------
    // Layout
    // -----------------------------------------------------------------------

    #[test]
    fn layout_next_rect_advances_cursor() {
        let mut layout = Layout::new(10.0, 10.0, 200.0);
        let r1 = layout.next_rect(30.0);
        let r2 = layout.next_rect(30.0);
        assert_eq!(r1.x, 10.0);
        assert_eq!(r1.y, 10.0);
        assert_eq!(r1.w, 200.0);
        assert_eq!(r1.h, 30.0);
        // r2 should be below r1 + spacing (default 10.0)
        assert_eq!(r2.y, 50.0);
    }

    // -----------------------------------------------------------------------
    // begin_frame clears per-frame state
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Password masking
    // -----------------------------------------------------------------------

    #[test]
    fn masked_text_input_produces_bullet_text_runs() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        let mut buf = TextBuffer::new("secret");
        ui.text_input_masked("Password", &mut buf, "");
        // The text run should contain bullets, not the actual text
        let text_run = ui.batch.text_runs.iter().find(|r| r.text.contains('\u{2022}')).unwrap();
        assert_eq!(text_run.text, "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");
        assert!(!ui.batch.text_runs.iter().any(|r| r.text.contains("secret")));
    }

    #[test]
    fn masked_text_input_preserves_actual_value() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        let mut buf = TextBuffer::new("hunter2");
        ui.text_input_masked("Password", &mut buf, "");
        // The underlying text must remain unchanged
        assert_eq!(buf.text(), "hunter2");
        // The widget value in the a11y tree should also have the real text
        let widget = ui.widgets.iter().find(|w| w.label == "Password").unwrap();
        assert_eq!(widget.value.as_deref(), Some("hunter2"));
    }

    #[test]
    fn masked_text_input_cursor_movement_works() {
        let mut ui = test_ui();
        let mut buf = TextBuffer::new("abc");
        // Focus the password field so key events are processed
        let id = ui.hash_id("Password");
        ui.focused = Some(id);
        // Simulate a left-arrow key event
        let events = vec![InputEvent::KeyDown {
            code: KeyCode::ArrowLeft,
            modifiers: Modifiers { shift: false, ctrl: false, alt: false, meta: false },
        }];
        ui.begin_frame(events, 800.0, 600.0, 1.0, 0.0);
        ui.text_input_masked("Password", &mut buf, "");
        // Caret should have moved left by one
        assert_eq!(buf.caret().index, 2);
        assert_eq!(buf.text(), "abc");
    }

    #[test]
    fn masked_empty_shows_placeholder() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        let mut buf = TextBuffer::new("");
        ui.text_input_masked("Password", &mut buf, "enter password");
        // Should show placeholder, not bullets
        assert!(ui.batch.text_runs.iter().any(|r| r.text == "enter password"));
    }

    #[test]
    fn begin_frame_clears_widgets_and_batch() {
        let mut ui = test_ui();
        ui.label("test");
        assert!(!ui.widgets.is_empty());
        assert!(!ui.batch.text_runs.is_empty());

        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        assert!(ui.widgets.is_empty());
        assert!(ui.batch.text_runs.is_empty());
        assert!(ui.batch.vertices.is_empty());
    }

    // -----------------------------------------------------------------------
    // Form-bound text input (text_input_for / text_input_masked_for)
    // -----------------------------------------------------------------------

    use crate::form::{FieldSchema, FieldType, FormSchema};

    fn simple_form_schema() -> FormSchema {
        FormSchema::new("test")
            .field("name", FieldType::Text)
            .with_label("name", "Name")
            .field("email", FieldType::Text)
            .with_label("email", "Email")
    }

    #[test]
    fn text_input_for_creates_buffer_from_form_state() {
        let mut ui = test_ui();
        let mut form = Form::new(simple_form_schema());
        let path = FormPath::root().push("name");

        // Pre-set a value on the form before first render.
        form.set_value(&path, FieldValue::Text("Alice".into()));

        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.text_input_for(&mut form, &path, "Name", "");

        // The auto-created buffer should have been initialized from the form.
        let buf = ui.form_buffers.get(&path).unwrap();
        assert_eq!(buf.text(), "Alice");
    }

    #[test]
    fn text_input_for_syncs_edits_back_to_form() {
        let mut ui = test_ui();
        let mut form = Form::new(simple_form_schema());
        let path = FormPath::root().push("name");

        // First frame: create the buffer (initially empty).
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.text_input_for(&mut form, &path, "Name", "");

        // Focus the widget so it processes text events.
        let widget_id = ui.hash_id("Name");
        ui.focused = Some(widget_id);

        // Second frame: simulate typing "Bob" via a TextInput event.
        let events = vec![InputEvent::TextInput(crate::input::TextInputEvent {
            text: "Bob".into(),
        })];
        ui.begin_frame(events, 800.0, 600.0, 1.0, 0.0);
        // Re-focus after begin_frame (focus persists but let's be explicit).
        ui.focused = Some(widget_id);
        ui.text_input_for(&mut form, &path, "Name", "");

        // The form state should reflect the typed value.
        let field = form.state().get_field(&path).unwrap();
        match &field.value {
            FieldValue::Text(v) => assert_eq!(v, "Bob"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn text_input_for_multiple_fields_separate_buffers() {
        let mut ui = test_ui();
        let mut form = Form::new(simple_form_schema());
        let name_path = FormPath::root().push("name");
        let email_path = FormPath::root().push("email");

        form.set_value(&name_path, FieldValue::Text("Alice".into()));
        form.set_value(&email_path, FieldValue::Text("alice@example.com".into()));

        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.text_input_for(&mut form, &name_path, "Name", "");
        ui.text_input_for(&mut form, &email_path, "Email", "");

        assert_eq!(ui.form_buffers.get(&name_path).unwrap().text(), "Alice");
        assert_eq!(
            ui.form_buffers.get(&email_path).unwrap().text(),
            "alice@example.com"
        );
    }

    #[test]
    fn text_input_masked_for_works() {
        let mut ui = test_ui();
        let schema = FormSchema::new("test")
            .field("password", FieldType::Text)
            .with_label("password", "Password");
        let mut form = Form::new(schema);
        let path = FormPath::root().push("password");

        form.set_value(&path, FieldValue::Text("secret".into()));

        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.text_input_masked_for(&mut form, &path, "Password", "");

        // Buffer should contain actual text.
        let buf = ui.form_buffers.get(&path).unwrap();
        assert_eq!(buf.text(), "secret");

        // Rendered text should be masked (bullets).
        let run = ui
            .batch
            .text_runs
            .iter()
            .find(|r| r.text.contains('\u{2022}'))
            .expect("expected masked text run");
        assert_eq!(run.text, "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");
    }

    // Horizontal layout (begin_row / end_row)
    // -----------------------------------------------------------------------

    #[test]
    fn begin_row_places_two_widgets_side_by_side() {
        let mut layout = Layout::new(10.0, 10.0, 200.0);
        layout.begin_row_with(&[1.0, 1.0]);
        let r1 = layout.next_rect(30.0);
        let r2 = layout.next_rect(30.0);
        layout.end_row();

        // Both should be on the same y.
        assert_eq!(r1.y, 10.0);
        assert_eq!(r2.y, 10.0);
        // r1 should start at x=10, r2 should be to its right.
        assert_eq!(r1.x, 10.0);
        // Available = 200 - 10 (one gap) = 190; each gets 95.
        let expected_w = (200.0 - 10.0) / 2.0;
        assert!((r1.w - expected_w).abs() < 0.01);
        assert!((r2.w - expected_w).abs() < 0.01);
        // r2.x = origin + (1/2)*available + 1*spacing
        let expected_r2_x = 10.0 + 0.5 * (200.0 - 10.0) + 10.0;
        assert!((r2.x - expected_r2_x).abs() < 0.01);
    }

    #[test]
    fn proportional_weights_distribute_width() {
        let mut layout = Layout::new(0.0, 0.0, 300.0);
        layout.begin_row_with(&[1.0, 2.0]);
        let r1 = layout.next_rect(20.0);
        let r2 = layout.next_rect(20.0);
        layout.end_row();

        // gap_total = 10.0, available = 290.0
        // r1 weight 1/3 => w = 290/3 ≈ 96.67
        // r2 weight 2/3 => w = 580/3 ≈ 193.33
        let available = 300.0 - 10.0;
        assert!((r1.w - available / 3.0).abs() < 0.01);
        assert!((r2.w - available * 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn nested_row_in_column() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);

        // First widget in vertical layout.
        ui.label("Header");
        let header_rect = ui.widgets.last().unwrap().rect;

        // Now a row with two labels.
        ui.begin_row_with(&[1.0, 1.0]);
        ui.label("Left");
        ui.label("Right");
        ui.end_row();

        let left_rect = ui.widgets.iter().find(|w| w.label == "Left").unwrap().rect;
        let right_rect = ui.widgets.iter().find(|w| w.label == "Right").unwrap().rect;

        // Both row children should be below the header.
        assert!(left_rect.y > header_rect.y);
        // They should share the same y.
        assert_eq!(left_rect.y, right_rect.y);
        // Right should be to the right of left.
        assert!(right_rect.x > left_rect.x);

        // A widget after end_row should be below the row.
        ui.label("Footer");
        let footer = ui.widgets.last().unwrap();
        assert!(footer.rect.y > left_rect.y);
    }

    #[test]
    fn end_row_without_begin_row_is_noop() {
        let mut layout = Layout::new(0.0, 0.0, 200.0);
        let y_before = layout.cursor.y;
        layout.end_row(); // should not panic
        assert_eq!(layout.cursor.y, y_before);
    }

    #[test]
    fn row_advances_parent_cursor_by_tallest_child() {
        let mut layout = Layout::new(0.0, 0.0, 200.0);
        layout.begin_row_with(&[1.0, 1.0]);
        let _r1 = layout.next_rect(20.0);
        let _r2 = layout.next_rect(50.0); // taller
        layout.end_row();

        // Next vertical widget should be at y = 50 + spacing(10) = 60.
        let r3 = layout.next_rect(10.0);
        assert!((r3.y - 60.0).abs() < 0.01);
    }

    #[test]
    fn begin_row_no_weights_defaults_to_two_columns() {
        let mut layout = Layout::new(0.0, 0.0, 200.0);
        layout.begin_row();
        let r1 = layout.next_rect(20.0);
        let r2 = layout.next_rect(20.0);
        layout.end_row();

        // Default (no weights) assumes 2 columns.
        assert_eq!(r1.y, r2.y);
        assert!(r2.x > r1.x);
        let expected_w = (200.0 - 10.0) / 2.0;
        assert!((r1.w - expected_w).abs() < 0.01);
    }

    // -----------------------------------------------------------------------
    // Proportional text metrics
    // -----------------------------------------------------------------------

    #[test]
    fn default_char_advance_matches_legacy() {
        let ui = test_ui();
        // Default fallback: font_size * 0.6
        let font_size = 15.0;
        let sums = ui.grapheme_prefix_sums("abc", font_size);
        let expected_cw = font_size * 0.6;
        assert_eq!(sums.len(), 4); // 3 graphemes + leading 0
        assert!((sums[0]).abs() < f32::EPSILON);
        assert!((sums[1] - expected_cw).abs() < f32::EPSILON);
        assert!((sums[2] - expected_cw * 2.0).abs() < f32::EPSILON);
        assert!((sums[3] - expected_cw * 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn custom_char_advance_proportional() {
        let mut ui = test_ui();
        // 'i' = 4px, 'W' = 12px at font_size 16
        ui.set_char_advance(Box::new(|ch, _fs| match ch {
            'i' => 4.0,
            'W' => 12.0,
            _ => 8.0,
        }));
        let sums = ui.grapheme_prefix_sums("Wi", 16.0);
        assert_eq!(sums.len(), 3);
        assert!((sums[0]).abs() < f32::EPSILON);
        assert!((sums[1] - 12.0).abs() < f32::EPSILON); // after 'W'
        assert!((sums[2] - 16.0).abs() < f32::EPSILON); // after 'i'
    }

    #[test]
    fn index_to_position_uses_proportional_advance() {
        let mut ui = test_ui();
        ui.set_char_advance(Box::new(|ch, _fs| match ch {
            'i' => 4.0,
            'W' => 12.0,
            _ => 8.0,
        }));
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        let buf = TextBuffer::new("Wi");
        let rect = Rect::new(0.0, 0.0, 200.0, 30.0);
        let padding = 8.0;

        let pos0 = ui.index_to_position(rect, &buf, 0, false);
        assert!((pos0.x - padding).abs() < f32::EPSILON);

        let pos1 = ui.index_to_position(rect, &buf, 1, false);
        assert!((pos1.x - (padding + 12.0)).abs() < f32::EPSILON); // after 'W'

        let pos2 = ui.index_to_position(rect, &buf, 2, false);
        assert!((pos2.x - (padding + 16.0)).abs() < f32::EPSILON); // after 'Wi'
    }

    #[test]
    fn position_to_index_uses_proportional_advance() {
        let mut ui = test_ui();
        ui.set_char_advance(Box::new(|ch, _fs| match ch {
            'i' => 4.0,
            'W' => 12.0,
            _ => 8.0,
        }));
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        let buf = TextBuffer::new("Wi");
        let rect = Rect::new(0.0, 0.0, 200.0, 30.0);
        let padding = 8.0;

        // Click in the middle of 'W' (x=6 within text) -> index 0
        let idx = ui.position_to_index(rect, &buf, Vec2::new(padding + 5.0, 5.0));
        assert_eq!(idx, 0);

        // Click past the midpoint of 'W' (x > 6) -> index 1
        let idx = ui.position_to_index(rect, &buf, Vec2::new(padding + 7.0, 5.0));
        assert_eq!(idx, 1);

        // Click in the middle of 'i' (at x = 12 + 2 = 14 within text) -> index 1
        let idx = ui.position_to_index(rect, &buf, Vec2::new(padding + 13.0, 5.0));
        assert_eq!(idx, 1);

        // Click past 'i' midpoint (x > 14 within text) -> index 2
        let idx = ui.position_to_index(rect, &buf, Vec2::new(padding + 15.0, 5.0));
        assert_eq!(idx, 2);

        // Click way past end -> index 2 (clamped)
        let idx = ui.position_to_index(rect, &buf, Vec2::new(padding + 100.0, 5.0));
        assert_eq!(idx, 2);
    }

    // -----------------------------------------------------------------------
    // Focus ring rendering
    // -----------------------------------------------------------------------

    #[test]
    fn focus_ring_drawn_when_widget_focused() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.button("Click me");
        let btn_id = ui.widgets[0].id;
        ui.focused = Some(btn_id);
        let verts_before = ui.batch.vertices.len();
        ui.end_frame();
        // 4 quads × 4 vertices = 16 new vertices for the focus ring
        assert_eq!(ui.batch.vertices.len() - verts_before, 16);
    }

    #[test]
    fn no_focus_ring_when_nothing_focused() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.button("Click me");
        ui.focused = None;
        let verts_before = ui.batch.vertices.len();
        ui.end_frame();
        assert_eq!(ui.batch.vertices.len(), verts_before);
    }

    #[test]
    fn focus_ring_high_contrast_uses_3px_thickness() {
        let mut theme = Theme::default_light();
        theme.high_contrast = true;
        let mut ui = Ui::new(800.0, 600.0, theme);
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.button("OK");
        let btn_id = ui.widgets[0].id;
        ui.focused = Some(btn_id);
        ui.end_frame();
        // The top edge quad should have height 3.0 (high contrast thickness)
        let ring_quad_start = ui.batch.vertices.len() - 16;
        let tl = &ui.batch.vertices[ring_quad_start];
        let br = &ui.batch.vertices[ring_quad_start + 2];
        let thickness = br.pos.y - tl.pos.y;
        assert!((thickness - 3.0).abs() < 0.01, "Expected 3px thickness, got {}", thickness);
    }

    #[test]
    fn focus_ring_moves_with_focus_change() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.button("A");
        ui.button("B");
        let id_a = ui.widgets[0].id;
        let rect_a = ui.widgets[0].rect;
        let id_b = ui.widgets[1].id;
        let rect_b = ui.widgets[1].rect;

        // Focus on A
        ui.focused = Some(id_a);
        ui.end_frame();
        // Top-left of first ring quad should be near rect_a
        let ring_start = ui.batch.vertices.len() - 16;
        let ring_y = ui.batch.vertices[ring_start].pos.y;
        assert!((ring_y - (rect_a.y - 2.0)).abs() < 0.01);

        // New frame, focus on B
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.button("A");
        ui.button("B");
        ui.focused = Some(id_b);
        ui.end_frame();
        let ring_start = ui.batch.vertices.len() - 16;
        let ring_y = ui.batch.vertices[ring_start].pos.y;
        assert!((ring_y - (rect_b.y - 2.0)).abs() < 0.01);
    }

    // -----------------------------------------------------------------------
    // Icon widget
    // -----------------------------------------------------------------------

    fn test_icon_pack() -> crate::icon::IconPack {
        let json = r#"{
            "name": "test-icons",
            "texture_size": [256, 256],
            "icons": [
                { "name": "check", "x": 0, "y": 0, "w": 24, "h": 24 },
                { "name": "close", "x": 24, "y": 0, "w": 24, "h": 24 }
            ]
        }"#;
        crate::icon::IconPack::from_manifest(json).unwrap()
    }

    #[test]
    fn icon_widget_emits_icon_atlas_quad() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.set_icon_pack(test_icon_pack());

        let rect = ui.icon("check", 24.0);
        assert!(rect.is_some());
        let rect = rect.unwrap();
        assert!((rect.w - 24.0).abs() < f32::EPSILON);
        assert!((rect.h - 24.0).abs() < f32::EPSILON);

        // Should have emitted exactly one draw command with IconAtlas material.
        assert_eq!(ui.batch.commands.len(), 1);
        assert_eq!(ui.batch.commands[0].material, Material::IconAtlas);
        // Should have 4 vertices (one quad).
        assert_eq!(ui.batch.vertices.len(), 4);
        // All vertices should have flags == 2 (icon material flag).
        for v in &ui.batch.vertices {
            assert_eq!(v.flags, 2);
        }
    }

    #[test]
    fn icon_widget_returns_none_for_missing_icon() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.set_icon_pack(test_icon_pack());

        let rect = ui.icon("nonexistent", 24.0);
        assert!(rect.is_none());
        // No quads should have been emitted.
        assert!(ui.batch.vertices.is_empty());
    }

    #[test]
    fn icon_widget_returns_none_without_pack() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);

        let rect = ui.icon("check", 24.0);
        assert!(rect.is_none());
    }

    #[test]
    fn icon_widget_respects_scale() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 2.0, 0.0);
        ui.set_icon_pack(test_icon_pack());

        let rect = ui.icon("check", 24.0).unwrap();
        // At scale 2.0, the icon should be 48x48 logical pixels.
        assert!((rect.w - 48.0).abs() < f32::EPSILON);
        assert!((rect.h - 48.0).abs() < f32::EPSILON);
    }

    #[test]
    fn icon_by_id_emits_quad() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        let pack = test_icon_pack();
        let check_id = pack.get("check").unwrap();
        ui.set_icon_pack(pack);

        let rect = ui.icon_by_id(check_id, 24.0);
        assert!(rect.is_some());
        assert_eq!(ui.batch.commands.len(), 1);
        assert_eq!(ui.batch.commands[0].material, Material::IconAtlas);
    }

    // -----------------------------------------------------------------------
    // Touch target sizing
    // -----------------------------------------------------------------------

    #[test]
    fn touch_rect_expands_small_widget_on_mobile() {
        let mut ui = test_ui();
        // Simulate mobile: width < 600
        ui.begin_frame(vec![], 400.0, 800.0, 2.0, 0.0);
        assert!(ui.touch_mode);
        let small = Rect::new(100.0, 100.0, 20.0, 20.0);
        let expanded = ui.touch_rect(small);
        // MIN_TOUCH_TARGET * scale = 44 * 2 = 88
        assert!(expanded.w >= 88.0 - 0.01);
        assert!(expanded.h >= 88.0 - 0.01);
        // Centred on original
        let cx = small.x + small.w * 0.5;
        let ecx = expanded.x + expanded.w * 0.5;
        assert!((cx - ecx).abs() < 0.01);
    }

    #[test]
    fn touch_rect_unchanged_on_desktop() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 1200.0, 800.0, 1.0, 0.0);
        assert!(!ui.touch_mode);
        let rect = Rect::new(100.0, 100.0, 20.0, 20.0);
        let result = ui.touch_rect(rect);
        assert_eq!(result.x, rect.x);
        assert_eq!(result.y, rect.y);
        assert_eq!(result.w, rect.w);
        assert_eq!(result.h, rect.h);
    }

    #[test]
    fn touch_mode_near_miss_registers_hit_on_mobile() {
        let mut ui = test_ui();
        // Place a pointer down near (but not on) a small button
        let click_pos = Vec2::new(140.0, 60.0); // outside 20×20 rect at (100, 50) but inside expanded
        let events = vec![
            InputEvent::PointerDown(PointerEvent {
                pos: click_pos,
                button: Some(PointerButton::Left),
                modifiers: Modifiers::default(),
            }),
            InputEvent::PointerUp(PointerEvent {
                pos: click_pos,
                button: Some(PointerButton::Left),
                modifiers: Modifiers::default(),
            }),
        ];
        ui.begin_frame(events, 400.0, 800.0, 2.0, 0.0);
        assert!(ui.touch_mode);
        // Manually test a small rect — the touch-expanded rect should contain the click
        let small = Rect::new(100.0, 50.0, 20.0, 20.0);
        let expanded = ui.touch_rect(small);
        assert!(expanded.contains(click_pos));
    }

    #[test]
    fn large_widget_not_expanded() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 400.0, 800.0, 1.0, 0.0);
        ui.touch_mode = true;
        let big = Rect::new(10.0, 10.0, 200.0, 60.0);
        let result = ui.touch_rect(big);
        assert_eq!(result.w, 200.0);
        assert_eq!(result.h, 60.0);
    }

    // -----------------------------------------------------------------------
    // Scroll containers
    // -----------------------------------------------------------------------

    #[test]
    fn scroll_container_clips_children() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.begin_scroll("scroller", 200.0);
        ui.label("Inside scroll");
        ui.end_scroll();
        ui.end_frame();

        // The label's text run should have a clip rect
        let text_run = ui.batch.text_runs.iter().find(|r| r.text == "Inside scroll").unwrap();
        assert!(text_run.clip.is_some());
    }

    #[test]
    fn scroll_container_wheel_changes_offset() {
        let mut ui = test_ui();

        // First frame: create scroll container and get its rect
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        let id = ui.begin_scroll("scroller", 100.0);
        // Add content taller than container
        for i in 0..20 {
            ui.label(&format!("Item {}", i));
        }
        ui.end_scroll();
        ui.end_frame();

        // Get the container rect
        let container_rect = ui.widgets.iter()
            .find(|w| w.kind == WidgetKind::ScrollContainer)
            .unwrap().rect;

        // Second frame: send wheel event inside the container
        let wheel = InputEvent::PointerWheel {
            pos: container_rect.center(),
            delta: Vec2::new(0.0, 50.0),
            modifiers: Modifiers::default(),
        };
        ui.begin_frame(vec![wheel], 800.0, 600.0, 1.0, 16.0);
        ui.begin_scroll("scroller", 100.0);
        for i in 0..20 {
            ui.label(&format!("Item {}", i));
        }
        ui.end_scroll();
        ui.end_frame();

        let state = ui.scroll_states.get(&id).unwrap();
        assert!(state.offset > 0.0, "Scroll offset should be > 0 after wheel event");
    }

    #[test]
    fn scroll_container_content_height_measured() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        let id = ui.begin_scroll("scroller", 100.0);
        for i in 0..10 {
            ui.label(&format!("Line {}", i));
        }
        ui.end_scroll();
        ui.end_frame();

        let state = ui.scroll_states.get(&id).unwrap();
        assert!(state.content_height > 100.0, "Content should overflow container");
        assert_eq!(state.container_height, 100.0);
    }

    #[test]
    fn scroll_container_renders_scrollbar_when_overflowing() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.begin_scroll("scroller", 100.0);
        for i in 0..20 {
            ui.label(&format!("Item {}", i));
        }
        let verts_before_end = ui.batch.vertices.len();
        ui.end_scroll();
        // end_scroll should have added scrollbar quads (track + thumb = 2 quads = 8 verts)
        assert!(ui.batch.vertices.len() > verts_before_end,
            "Scrollbar quads should be rendered");
    }

    #[test]
    fn nested_scroll_containers_independent() {
        let mut ui = test_ui();
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        let outer_id = ui.begin_scroll("outer", 300.0);
        ui.label("Outer content");
        let inner_id = ui.begin_scroll("inner", 100.0);
        ui.label("Inner content");
        ui.end_scroll();
        ui.end_scroll();
        ui.end_frame();

        assert_ne!(outer_id, inner_id);
        assert!(ui.scroll_states.contains_key(&outer_id));
        assert!(ui.scroll_states.contains_key(&inner_id));
    }

    #[test]
    fn scroll_offset_clamped_to_valid_range() {
        let mut ui = test_ui();

        // First frame: build content
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        let id = ui.begin_scroll("scroller", 100.0);
        ui.label("Short");
        ui.end_scroll();
        ui.end_frame();

        // Try to scroll past content
        ui.scroll_states.get_mut(&id).unwrap().offset = 9999.0;
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 16.0);
        ui.begin_scroll("scroller", 100.0);
        ui.label("Short");
        ui.end_scroll();
        ui.end_frame();

        let state = ui.scroll_states.get(&id).unwrap();
        assert!(state.offset <= state.max_offset(),
            "Offset should be clamped: offset={}, max={}", state.offset, state.max_offset());
    }

    // -----------------------------------------------------------------------
    // Dropdown / Select widget
    // -----------------------------------------------------------------------

    fn test_options() -> Vec<String> {
        vec!["Apple".into(), "Banana".into(), "Cherry".into(), "Date".into()]
    }

    #[test]
    fn select_click_opens_dropdown() {
        let mut ui = test_ui();
        let options = test_options();
        let mut value = "Apple".to_string();

        // First frame: render select, get its rect
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.select("Fruit", &options, &mut value);
        let select_rect = ui.widgets.iter().find(|w| w.label == "Fruit").unwrap().rect;
        let select_id = ui.widgets.iter().find(|w| w.label == "Fruit").unwrap().id;
        ui.end_frame();

        // Second frame: click on the select
        let center = select_rect.center();
        let events = vec![
            InputEvent::PointerDown(PointerEvent {
                pos: center,
                button: Some(PointerButton::Left),
                modifiers: Modifiers::default(),
            }),
            InputEvent::PointerUp(PointerEvent {
                pos: center,
                button: Some(PointerButton::Left),
                modifiers: Modifiers::default(),
            }),
        ];
        ui.begin_frame(events, 800.0, 600.0, 1.0, 0.0);
        ui.select("Fruit", &options, &mut value);
        assert_eq!(ui.dropdown_open, Some(select_id));
        // Widget should report expanded
        let w = ui.widgets.iter().find(|w| w.label == "Fruit").unwrap();
        assert!(w.state.expanded);
    }

    #[test]
    fn select_arrow_keys_navigate_options() {
        let mut ui = test_ui();
        let options = test_options();
        let mut value = "Apple".to_string();

        // Open the dropdown
        let id = ui.hash_id("Fruit");
        ui.dropdown_open = Some(id);
        ui.dropdown_highlighted = 0;
        ui.focused = Some(id);

        // Press ArrowDown
        let events = vec![InputEvent::KeyDown {
            code: KeyCode::ArrowDown,
            modifiers: Modifiers::default(),
        }];
        ui.begin_frame(events, 800.0, 600.0, 1.0, 0.0);
        ui.select("Fruit", &options, &mut value);
        assert_eq!(ui.dropdown_highlighted, 1);
        // Value unchanged until Enter
        assert_eq!(value, "Apple");
    }

    #[test]
    fn select_enter_selects_highlighted() {
        let mut ui = test_ui();
        let options = test_options();
        let mut value = "Apple".to_string();

        let id = ui.hash_id("Fruit");
        ui.dropdown_open = Some(id);
        ui.dropdown_highlighted = 2; // Cherry
        ui.focused = Some(id);

        let events = vec![InputEvent::KeyDown {
            code: KeyCode::Enter,
            modifiers: Modifiers::default(),
        }];
        ui.begin_frame(events, 800.0, 600.0, 1.0, 0.0);
        let changed = ui.select("Fruit", &options, &mut value);
        assert!(changed);
        assert_eq!(value, "Cherry");
        assert_eq!(ui.dropdown_open, None); // closed after selection
    }

    #[test]
    fn select_escape_closes_without_change() {
        let mut ui = test_ui();
        let options = test_options();
        let mut value = "Banana".to_string();

        let id = ui.hash_id("Fruit");
        ui.dropdown_open = Some(id);
        ui.dropdown_highlighted = 3;
        ui.focused = Some(id);

        let events = vec![InputEvent::KeyDown {
            code: KeyCode::Escape,
            modifiers: Modifiers::default(),
        }];
        ui.begin_frame(events, 800.0, 600.0, 1.0, 0.0);
        let changed = ui.select("Fruit", &options, &mut value);
        assert!(!changed);
        assert_eq!(value, "Banana");
        assert_eq!(ui.dropdown_open, None);
    }

    #[test]
    fn select_type_ahead_jumps_to_matching_option() {
        let mut ui = test_ui();
        let options = test_options();
        let mut value = "Apple".to_string();

        let id = ui.hash_id("Fruit");
        ui.dropdown_open = Some(id);
        ui.dropdown_highlighted = 0;
        ui.focused = Some(id);

        let events = vec![InputEvent::TextInput(TextInputEvent {
            text: "c".to_string(),
        })];
        ui.begin_frame(events, 800.0, 600.0, 1.0, 0.0);
        ui.select("Fruit", &options, &mut value);
        assert_eq!(ui.dropdown_highlighted, 2); // Cherry
    }

    #[test]
    fn select_dropdown_renders_panel_in_end_frame() {
        let mut ui = test_ui();
        let options = test_options();
        let mut value = "Apple".to_string();

        let id = ui.hash_id("Fruit");
        ui.dropdown_open = Some(id);
        ui.focused = Some(id);
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);
        ui.select("Fruit", &options, &mut value);

        let verts_before = ui.batch.vertices.len();
        ui.end_frame();
        // end_frame should have rendered the panel (background + highlighted item)
        assert!(ui.batch.vertices.len() > verts_before);
    }

    // -----------------------------------------------------------------------
    // Safe area insets
    // -----------------------------------------------------------------------

    /// Non-zero insets must shift the layout origin and reduce the usable
    /// width by the corresponding amounts.
    #[test]
    fn safe_area_insets_offset_layout() {
        let mut ui = test_ui();

        // Typical phone notch + home bar: top=50, right=10, bottom=34, left=20.
        ui.set_safe_area_insets([50.0, 10.0, 34.0, 20.0]);

        // begin_frame rebuilds the layout using the stored insets.
        ui.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);

        // Expected origin: x = 24 (base margin) + 20 (safe left) = 44
        //                  y = 24 (base margin) + 50 (safe top)  = 74
        assert_eq!(
            ui.layout.cursor,
            Vec2::new(44.0, 74.0),
            "layout origin should be offset by safe area insets"
        );

        // Expected usable width: 800 - (24 + 20) - (24 + 10) = 800 - 44 - 34 = 722
        assert_eq!(
            ui.layout.width,
            722.0,
            "layout width should be reduced by left and right insets"
        );
    }

    /// When all insets are zero the layout must be identical to a fresh Ui
    /// with no safe area applied — ensuring backward compatibility on desktop
    /// and SE-style devices.
    #[test]
    fn zero_insets_unchanged() {
        let mut ui_no_insets = test_ui();
        ui_no_insets.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);

        let mut ui_zero_insets = test_ui();
        ui_zero_insets.set_safe_area_insets([0.0; 4]);
        ui_zero_insets.begin_frame(vec![], 800.0, 600.0, 1.0, 0.0);

        assert_eq!(
            ui_no_insets.layout.cursor,
            ui_zero_insets.layout.cursor,
            "zero insets must not change the layout origin"
        );
        assert_eq!(
            ui_no_insets.layout.width,
            ui_zero_insets.layout.width,
            "zero insets must not change the layout width"
        );
    }
}
