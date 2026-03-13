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
pub enum Material {
    Solid,
    TextAtlas,
    IconAtlas,
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
