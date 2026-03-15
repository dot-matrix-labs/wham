use crate::types::{Color, Rect, Vec2};

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
}

#[derive(Default, Debug, Clone)]
pub struct Batch {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub commands: Vec<DrawCmd>,
    pub text_runs: Vec<TextRun>,
}

impl Batch {
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.commands.clear();
        self.text_runs.clear();
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
}
