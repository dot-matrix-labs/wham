use std::collections::{HashMap, HashSet};

use crate::types::{Color, Rect, Vec2};

/// Stable widget identifier — a hash of the full ID-stack path.
pub type WidgetId = u64;

#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub pos: Vec2,
    pub uv: Vec2,
    pub color: Color,
    pub flags: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Quad {
    pub rect: Rect,
    pub uv: Rect,
    pub color: Color,
    pub flags: u32,
}

#[derive(Clone, Debug)]
pub struct DrawCmd {
    pub start: u32,
    pub count: u32,
    pub material: Material,
    pub clip: Option<Rect>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Material {
    Solid,
    TextAtlas,
    IconAtlas,
}

/// The vertex + index range that a single widget occupies in the batch buffers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WidgetRange {
    /// Inclusive start index into `Batch::vertices`.
    pub vertex_start: usize,
    /// Exclusive end index into `Batch::vertices`.
    pub vertex_end: usize,
    /// Inclusive start index into `Batch::indices`.
    pub index_start: usize,
    /// Exclusive end index into `Batch::indices`.
    pub index_end: usize,
}

/// Per-frame dirty tracking.
///
/// The tracker distinguishes between two modes:
///
/// - **Fully dirty**: the entire frame must be rebuilt (first frame, window
///   resize, theme change, context restore). `is_fully_dirty()` returns
///   `true`.
/// - **Partially dirty**: only widgets whose inputs changed need their quads
///   regenerated. `dirty_set` contains their IDs.
#[derive(Debug)]
pub struct DirtyTracker {
    /// When `true`, all widgets must be rebuilt regardless of `dirty_set`.
    fully_dirty: bool,
    /// IDs of widgets whose inputs changed this frame.
    dirty_set: HashSet<WidgetId>,
    /// The vertex/index range each widget occupied in the *previous* frame's
    /// batch. Used so clean widgets can have their data copied verbatim.
    prev_ranges: HashMap<WidgetId, WidgetRange>,
}

impl Default for DirtyTracker {
    fn default() -> Self {
        Self {
            // First frame is always fully dirty.
            fully_dirty: true,
            dirty_set: HashSet::new(),
            prev_ranges: HashMap::new(),
        }
    }
}

impl DirtyTracker {
    /// Returns `true` when the entire frame must be rebuilt.
    #[inline]
    pub fn is_fully_dirty(&self) -> bool {
        self.fully_dirty
    }

    /// Mark the whole frame as dirty (e.g. resize, theme change).
    pub fn mark_fully_dirty(&mut self) {
        self.fully_dirty = true;
        self.dirty_set.clear();
    }

    /// Mark a single widget as needing a quad rebuild this frame.
    pub fn mark_dirty(&mut self, id: WidgetId) {
        self.dirty_set.insert(id);
    }

    /// Returns `true` if `id` must have its quads regenerated this frame.
    /// Always returns `true` when the tracker is fully dirty.
    #[inline]
    pub fn is_dirty(&self, id: WidgetId) -> bool {
        self.fully_dirty || self.dirty_set.contains(&id)
    }

    /// Returns the previous-frame range for `id`, or `None` if unavailable
    /// (first frame, widget just appeared, etc.).
    #[inline]
    pub fn prev_range(&self, id: WidgetId) -> Option<&WidgetRange> {
        self.prev_ranges.get(&id)
    }

    /// Called at the **end** of each frame to:
    /// 1. Persist the current frame's widget ranges as the "previous" ranges
    ///    for the next frame.
    /// 2. Reset the dirty set and fully-dirty flag.
    pub fn end_frame(&mut self, current_ranges: &HashMap<WidgetId, WidgetRange>) {
        self.prev_ranges.clone_from(current_ranges);
        self.fully_dirty = false;
        self.dirty_set.clear();
    }

    /// Returns the number of widgets currently in the dirty set.
    #[cfg(test)]
    pub fn dirty_count(&self) -> usize {
        self.dirty_set.len()
    }
}

/// The `Batch` is the output of one frame's widget traversal. It is built
/// by calling `push_quad` (and friends) for each visible widget and is
/// consumed by the renderer each frame.
///
/// The batch works alongside a [`DirtyTracker`] to skip regenerating quads for
/// widgets whose visual inputs have not changed. On a partial-dirty frame the
/// caller should:
///
/// 1. Call [`Batch::begin_widget`] to announce the start of a widget's quads.
/// 2. Check [`DirtyTracker::is_dirty`]; if clean, call
///    [`Batch::reuse_widget`] instead of emitting new quads.
/// 3. After emitting all quads for a dirty widget, call
///    [`Batch::end_widget`] to record the range.
/// 4. After the full frame, call [`DirtyTracker::end_frame`] with
///    `batch.widget_ranges()`.
#[derive(Default, Debug, Clone)]
pub struct Batch {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub commands: Vec<DrawCmd>,
    pub text_runs: Vec<TextRun>,
    /// Per-widget vertex/index ranges accumulated this frame.
    widget_ranges: HashMap<WidgetId, WidgetRange>,
    /// The vertex/index cursor at which the current widget began.
    current_widget_start: Option<(WidgetId, usize, usize)>,
}

impl Batch {
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.commands.clear();
        self.text_runs.clear();
        self.widget_ranges.clear();
        self.current_widget_start = None;
    }

    /// Returns the per-widget ranges recorded during this frame.
    pub fn widget_ranges(&self) -> &HashMap<WidgetId, WidgetRange> {
        &self.widget_ranges
    }

    /// Signal the start of a widget's quad emission.
    ///
    /// Must be paired with [`end_widget`]. Nested calls are not supported;
    /// each widget must begin/end before the next begins.
    pub fn begin_widget(&mut self, id: WidgetId) {
        self.current_widget_start = Some((id, self.vertices.len(), self.indices.len()));
    }

    /// Signal the end of a widget's quad emission and record its range.
    pub fn end_widget(&mut self) {
        if let Some((id, v_start, i_start)) = self.current_widget_start.take() {
            self.widget_ranges.insert(
                id,
                WidgetRange {
                    vertex_start: v_start,
                    vertex_end: self.vertices.len(),
                    index_start: i_start,
                    index_end: self.indices.len(),
                },
            );
        }
    }

    /// Copy vertex and index data from a *previous* frame's batch for a widget
    /// whose inputs have not changed.
    ///
    /// `prev_vertices` and `prev_indices` are slices of the previous frame's
    /// raw buffers, and `range` is the `WidgetRange` recorded for that widget
    /// last frame.
    ///
    /// The index values stored in `prev_indices[range.index_start..range.index_end]`
    /// are rebased so that they reference the *new* vertex positions in the
    /// current batch.
    pub fn reuse_widget(
        &mut self,
        id: WidgetId,
        prev_vertices: &[Vertex],
        prev_indices: &[u32],
        range: &WidgetRange,
    ) {
        let new_vertex_start = self.vertices.len();
        let new_index_start = self.indices.len();

        // Copy vertices verbatim.
        self.vertices
            .extend_from_slice(&prev_vertices[range.vertex_start..range.vertex_end]);

        // Copy indices, rebasing to the new vertex position.
        let rebase = new_vertex_start as u32 - range.vertex_start as u32;
        for &idx in &prev_indices[range.index_start..range.index_end] {
            self.indices.push(idx + rebase);
        }

        // Re-record the range for next frame.
        self.widget_ranges.insert(
            id,
            WidgetRange {
                vertex_start: new_vertex_start,
                vertex_end: self.vertices.len(),
                index_start: new_index_start,
                index_end: self.indices.len(),
            },
        );
    }

    pub fn push_quad(&mut self, quad: Quad, material: Material, clip: Option<Rect>) {
        let base = self.vertices.len() as u32;
        let rect = quad.rect;
        let uv = quad.uv;
        let color = quad.color;
        let flags = quad.flags;

        self.vertices.push(Vertex {
            pos: Vec2::new(rect.x, rect.y),
            uv: Vec2::new(uv.x, uv.y),
            color,
            flags,
        });
        self.vertices.push(Vertex {
            pos: Vec2::new(rect.x + rect.w, rect.y),
            uv: Vec2::new(uv.x + uv.w, uv.y),
            color,
            flags,
        });
        self.vertices.push(Vertex {
            pos: Vec2::new(rect.x + rect.w, rect.y + rect.h),
            uv: Vec2::new(uv.x + uv.w, uv.y + uv.h),
            color,
            flags,
        });
        self.vertices.push(Vertex {
            pos: Vec2::new(rect.x, rect.y + rect.h),
            uv: Vec2::new(uv.x, uv.y + uv.h),
            color,
            flags,
        });

        self.indices.extend_from_slice(&[
            base,
            base + 1,
            base + 2,
            base,
            base + 2,
            base + 3,
        ]);

        let count = 6;
        if let Some(last) = self.commands.last_mut() {
            if last.material == material && last.clip == clip {
                last.count += count;
                return;
            }
        }

        self.commands.push(DrawCmd {
            start: (self.indices.len() as u32) - count,
            count,
            material,
            clip,
        });
    }
}

#[derive(Clone, Debug)]
pub struct TextRun {
    pub rect: Rect,
    pub text: String,
    pub color: Color,
    pub font_size: f32,
    pub clip: Option<Rect>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_quad(x: f32, y: f32, w: f32, h: f32) -> Quad {
        Quad {
            rect: Rect::new(x, y, w, h),
            uv: Rect::new(0.0, 0.0, 1.0, 1.0),
            color: Color::rgba(1.0, 1.0, 1.0, 1.0),
            flags: 0,
        }
    }

    #[test]
    fn push_quad_generates_four_vertices_and_six_indices() {
        let mut batch = Batch::default();
        batch.push_quad(solid_quad(10.0, 20.0, 100.0, 50.0), Material::Solid, None);
        assert_eq!(batch.vertices.len(), 4);
        assert_eq!(batch.indices.len(), 6);
        assert_eq!(batch.commands.len(), 1);
        assert_eq!(batch.commands[0].count, 6);
    }

    #[test]
    fn push_quad_vertices_match_rect_corners() {
        let mut batch = Batch::default();
        batch.push_quad(solid_quad(10.0, 20.0, 100.0, 50.0), Material::Solid, None);

        // Top-left
        assert_eq!(batch.vertices[0].pos.x, 10.0);
        assert_eq!(batch.vertices[0].pos.y, 20.0);
        // Top-right
        assert_eq!(batch.vertices[1].pos.x, 110.0);
        assert_eq!(batch.vertices[1].pos.y, 20.0);
        // Bottom-right
        assert_eq!(batch.vertices[2].pos.x, 110.0);
        assert_eq!(batch.vertices[2].pos.y, 70.0);
        // Bottom-left
        assert_eq!(batch.vertices[3].pos.x, 10.0);
        assert_eq!(batch.vertices[3].pos.y, 70.0);
    }

    #[test]
    fn push_quad_indices_form_two_triangles() {
        let mut batch = Batch::default();
        batch.push_quad(solid_quad(0.0, 0.0, 1.0, 1.0), Material::Solid, None);
        assert_eq!(batch.indices, vec![0, 1, 2, 0, 2, 3]);
    }

    #[test]
    fn second_quad_indices_offset_correctly() {
        let mut batch = Batch::default();
        batch.push_quad(solid_quad(0.0, 0.0, 1.0, 1.0), Material::Solid, None);
        batch.push_quad(solid_quad(2.0, 0.0, 1.0, 1.0), Material::Solid, None);
        assert_eq!(batch.vertices.len(), 8);
        assert_eq!(batch.indices.len(), 12);
        // Second quad indices should start at 4
        assert_eq!(batch.indices[6..], [4, 5, 6, 4, 6, 7]);
    }

    #[test]
    fn same_material_batches_into_one_command() {
        let mut batch = Batch::default();
        batch.push_quad(solid_quad(0.0, 0.0, 1.0, 1.0), Material::Solid, None);
        batch.push_quad(solid_quad(2.0, 0.0, 1.0, 1.0), Material::Solid, None);
        assert_eq!(batch.commands.len(), 1);
        assert_eq!(batch.commands[0].count, 12);
    }

    #[test]
    fn different_material_creates_new_command() {
        let mut batch = Batch::default();
        batch.push_quad(solid_quad(0.0, 0.0, 1.0, 1.0), Material::Solid, None);
        batch.push_quad(solid_quad(2.0, 0.0, 1.0, 1.0), Material::TextAtlas, None);
        assert_eq!(batch.commands.len(), 2);
        assert_eq!(batch.commands[0].count, 6);
        assert_eq!(batch.commands[0].material, Material::Solid);
        assert_eq!(batch.commands[1].count, 6);
        assert_eq!(batch.commands[1].material, Material::TextAtlas);
    }

    #[test]
    fn different_clip_creates_new_command() {
        let mut batch = Batch::default();
        let clip = Some(Rect::new(0.0, 0.0, 100.0, 100.0));
        batch.push_quad(solid_quad(0.0, 0.0, 1.0, 1.0), Material::Solid, None);
        batch.push_quad(solid_quad(2.0, 0.0, 1.0, 1.0), Material::Solid, clip);
        assert_eq!(batch.commands.len(), 2);
    }

    #[test]
    fn clear_resets_all_buffers() {
        let mut batch = Batch::default();
        batch.push_quad(solid_quad(0.0, 0.0, 1.0, 1.0), Material::Solid, None);
        batch.text_runs.push(TextRun {
            rect: Rect::new(0.0, 0.0, 100.0, 20.0),
            text: "hello".into(),
            color: Color::rgba(1.0, 1.0, 1.0, 1.0),
            font_size: 16.0,
            clip: None,
        });
        batch.clear();
        assert!(batch.vertices.is_empty());
        assert!(batch.indices.is_empty());
        assert!(batch.commands.is_empty());
        assert!(batch.text_runs.is_empty());
        assert!(batch.widget_ranges.is_empty());
    }

    #[test]
    fn uv_coordinates_propagated() {
        let mut batch = Batch::default();
        let quad = Quad {
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            uv: Rect::new(0.25, 0.25, 0.5, 0.5),
            color: Color::rgba(1.0, 1.0, 1.0, 1.0),
            flags: 0,
        };
        batch.push_quad(quad, Material::TextAtlas, None);
        // Top-left UV
        assert_eq!(batch.vertices[0].uv.x, 0.25);
        assert_eq!(batch.vertices[0].uv.y, 0.25);
        // Bottom-right UV
        assert_eq!(batch.vertices[2].uv.x, 0.75);
        assert_eq!(batch.vertices[2].uv.y, 0.75);
    }

    #[test]
    fn flags_propagated_to_all_vertices() {
        let mut batch = Batch::default();
        let quad = Quad {
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            uv: Rect::new(0.0, 0.0, 1.0, 1.0),
            color: Color::rgba(1.0, 1.0, 1.0, 1.0),
            flags: 42,
        };
        batch.push_quad(quad, Material::Solid, None);
        for v in &batch.vertices {
            assert_eq!(v.flags, 42);
        }
    }

    #[test]
    fn material_switching_back_creates_new_command() {
        let mut batch = Batch::default();
        batch.push_quad(solid_quad(0.0, 0.0, 1.0, 1.0), Material::Solid, None);
        batch.push_quad(solid_quad(1.0, 0.0, 1.0, 1.0), Material::TextAtlas, None);
        batch.push_quad(solid_quad(2.0, 0.0, 1.0, 1.0), Material::Solid, None);
        assert_eq!(batch.commands.len(), 3);
    }

    #[test]
    fn icon_atlas_material_creates_separate_command() {
        let mut batch = Batch::default();
        batch.push_quad(solid_quad(0.0, 0.0, 1.0, 1.0), Material::Solid, None);
        batch.push_quad(solid_quad(1.0, 0.0, 1.0, 1.0), Material::IconAtlas, None);
        assert_eq!(batch.commands.len(), 2);
        assert_eq!(batch.commands[1].material, Material::IconAtlas);
    }

    #[test]
    fn icon_atlas_quads_batch_together() {
        let mut batch = Batch::default();
        batch.push_quad(solid_quad(0.0, 0.0, 1.0, 1.0), Material::IconAtlas, None);
        batch.push_quad(solid_quad(1.0, 0.0, 1.0, 1.0), Material::IconAtlas, None);
        assert_eq!(batch.commands.len(), 1);
        assert_eq!(batch.commands[0].count, 12);
        assert_eq!(batch.commands[0].material, Material::IconAtlas);
    }

    // -----------------------------------------------------------------
    // Dirty-region tracking tests
    // -----------------------------------------------------------------

    #[test]
    fn dirty_tracker_starts_fully_dirty() {
        let tracker = DirtyTracker::default();
        assert!(tracker.is_fully_dirty());
        assert!(tracker.is_dirty(42));
    }

    #[test]
    fn dirty_tracker_partial_after_first_end_frame() {
        let mut tracker = DirtyTracker::default();
        tracker.end_frame(&HashMap::new());
        assert!(!tracker.is_fully_dirty());
    }

    #[test]
    fn mark_dirty_sets_widget_as_dirty() {
        let mut tracker = DirtyTracker::default();
        tracker.end_frame(&HashMap::new()); // clear full-dirty flag
        assert!(!tracker.is_dirty(1));
        tracker.mark_dirty(1);
        assert!(tracker.is_dirty(1));
        assert!(!tracker.is_dirty(2));
    }

    #[test]
    fn mark_fully_dirty_overrides_partial() {
        let mut tracker = DirtyTracker::default();
        tracker.end_frame(&HashMap::new());
        tracker.mark_fully_dirty();
        assert!(tracker.is_fully_dirty());
        assert!(tracker.is_dirty(999));
    }

    #[test]
    fn end_frame_clears_dirty_set() {
        let mut tracker = DirtyTracker::default();
        tracker.end_frame(&HashMap::new()); // clear initial full dirty
        tracker.mark_dirty(5);
        assert_eq!(tracker.dirty_count(), 1);
        tracker.end_frame(&HashMap::new());
        assert_eq!(tracker.dirty_count(), 0);
    }

    #[test]
    fn begin_end_widget_records_range() {
        let mut batch = Batch::default();
        batch.begin_widget(1);
        batch.push_quad(solid_quad(0.0, 0.0, 10.0, 10.0), Material::Solid, None);
        batch.end_widget();

        let ranges = batch.widget_ranges();
        assert!(ranges.contains_key(&1));
        let r = &ranges[&1];
        assert_eq!(r.vertex_start, 0);
        assert_eq!(r.vertex_end, 4);
        assert_eq!(r.index_start, 0);
        assert_eq!(r.index_end, 6);
    }

    #[test]
    fn begin_end_widget_two_widgets_adjacent_ranges() {
        let mut batch = Batch::default();
        batch.begin_widget(1);
        batch.push_quad(solid_quad(0.0, 0.0, 10.0, 10.0), Material::Solid, None);
        batch.end_widget();

        batch.begin_widget(2);
        batch.push_quad(solid_quad(20.0, 0.0, 10.0, 10.0), Material::Solid, None);
        batch.end_widget();

        let ranges = batch.widget_ranges();
        let r1 = &ranges[&1];
        let r2 = &ranges[&2];
        assert_eq!(r1.vertex_end, r2.vertex_start);
        assert_eq!(r1.index_end, r2.index_start);
    }

    #[test]
    fn reuse_widget_copies_geometry_correctly() {
        // Frame 1: build a batch with widget 1.
        let mut frame1 = Batch::default();
        frame1.begin_widget(1);
        frame1.push_quad(solid_quad(5.0, 5.0, 20.0, 20.0), Material::Solid, None);
        frame1.end_widget();

        let range = frame1.widget_ranges()[&1].clone();

        // Frame 2: reuse widget 1 without regenerating quads.
        let mut frame2 = Batch::default();
        frame2.reuse_widget(1, &frame1.vertices, &frame1.indices, &range);

        // Geometry should be identical.
        assert_eq!(frame2.vertices.len(), frame1.vertices.len());
        assert_eq!(frame2.indices.len(), frame1.indices.len());
        for (a, b) in frame2.vertices.iter().zip(frame1.vertices.iter()) {
            assert_eq!(a.pos.x, b.pos.x);
            assert_eq!(a.pos.y, b.pos.y);
        }
    }

    #[test]
    fn reuse_widget_rebases_indices() {
        // Frame 1: two widgets.
        let mut frame1 = Batch::default();
        frame1.begin_widget(1);
        frame1.push_quad(solid_quad(0.0, 0.0, 10.0, 10.0), Material::Solid, None);
        frame1.end_widget();
        frame1.begin_widget(2);
        frame1.push_quad(solid_quad(20.0, 0.0, 10.0, 10.0), Material::Solid, None);
        frame1.end_widget();

        let range2 = frame1.widget_ranges()[&2].clone();

        // Frame 2: first add a different widget (shifts vertex buffer),
        // then reuse widget 2.
        let mut frame2 = Batch::default();
        frame2.begin_widget(99);
        frame2.push_quad(solid_quad(0.0, 0.0, 1.0, 1.0), Material::Solid, None);
        frame2.end_widget();

        // frame2 now has 4 vertices; reuse widget 2 (which in frame1 started at vertex 4).
        frame2.reuse_widget(2, &frame1.vertices, &frame1.indices, &range2);

        // Indices for the reused widget must start at 4 (after the 4 new verts).
        let reused_indices = &frame2.indices[6..]; // first 6 are widget 99's
        assert_eq!(reused_indices[0], 4);
        assert_eq!(reused_indices[1], 5);
        assert_eq!(reused_indices[2], 6);
        assert_eq!(reused_indices[3], 4);
        assert_eq!(reused_indices[4], 6);
        assert_eq!(reused_indices[5], 7);
    }

    #[test]
    fn unchanged_widget_does_not_regenerate_quads_over_many_frames() {
        // Simulate 1000 frames where widget 1 is clean and widget 2 changes
        // every frame. Verify that widget 1's vertex data stays consistent.
        let mut tracker = DirtyTracker::default();
        let mut prev_batch = Batch::default();

        // Frame 0 (fully dirty): build both widgets.
        prev_batch.begin_widget(1);
        prev_batch.push_quad(solid_quad(0.0, 0.0, 50.0, 30.0), Material::Solid, None);
        prev_batch.end_widget();
        prev_batch.begin_widget(2);
        prev_batch.push_quad(solid_quad(0.0, 40.0, 50.0, 30.0), Material::Solid, None);
        prev_batch.end_widget();
        tracker.end_frame(prev_batch.widget_ranges());

        for frame in 1..=1000 {
            // Widget 2 is dirty every frame; widget 1 is never dirty.
            tracker.mark_dirty(2);

            let mut cur_batch = Batch::default();

            // Widget 1 — reuse.
            {
                let range = tracker.prev_range(1).cloned().unwrap();
                cur_batch.reuse_widget(1, &prev_batch.vertices, &prev_batch.indices, &range);
            }

            // Widget 2 — regenerate with a slightly different position each frame.
            {
                cur_batch.begin_widget(2);
                cur_batch.push_quad(
                    solid_quad(0.0, 40.0 + frame as f32, 50.0, 30.0),
                    Material::Solid,
                    None,
                );
                cur_batch.end_widget();
            }

            tracker.end_frame(cur_batch.widget_ranges());

            // Widget 1's vertex data must remain at (0,0) in the new batch.
            let r1 = &cur_batch.widget_ranges()[&1];
            assert_eq!(cur_batch.vertices[r1.vertex_start].pos.x, 0.0);
            assert_eq!(cur_batch.vertices[r1.vertex_start].pos.y, 0.0);

            // Widget 2's y-position must reflect this frame's value.
            let r2 = &cur_batch.widget_ranges()[&2];
            assert_eq!(cur_batch.vertices[r2.vertex_start].pos.y, 40.0 + frame as f32);

            prev_batch = cur_batch;
        }
    }

    #[test]
    fn dirty_propagation_on_first_frame() {
        // On the very first frame everything is fully dirty — even widgets
        // not explicitly marked dirty must return is_dirty == true.
        let tracker = DirtyTracker::default();
        for id in [0u64, 1, 42, u64::MAX] {
            assert!(tracker.is_dirty(id), "expected id {} to be dirty on first frame", id);
        }
    }
}
