use wasm_bindgen::{JsCast, JsValue};
use web_sys::{HtmlCanvasElement, WebGl2RenderingContext as Gl, WebGlBuffer, WebGlProgram, WebGlShader, WebGlTexture};

use ui_core::batch::{Batch, Material, Quad, TextRun};
use ui_core::types::Rect;

use crate::atlas::TextAtlas;

pub struct Renderer {
    gl: Gl,
    program: WebGlProgram,
    vbo: WebGlBuffer,
    ibo: WebGlBuffer,
    atlas: TextAtlas,
    atlas_texture: WebGlTexture,
    width: f32,
    height: f32,
}

impl Renderer {
    pub fn new(canvas: &HtmlCanvasElement, width: f32, height: f32) -> Result<Self, JsValue> {
        let gl: Gl = canvas
            .get_context("webgl2")?
            .ok_or_else(|| JsValue::from_str("WebGL2 not supported"))?
            .dyn_into()?;
        let program = link_program(&gl, VERT_SHADER, FRAG_SHADER)?;
        let vbo = gl.create_buffer().ok_or_else(|| JsValue::from_str("no vbo"))?;
        let ibo = gl.create_buffer().ok_or_else(|| JsValue::from_str("no ibo"))?;
        let atlas_texture = gl.create_texture().ok_or_else(|| JsValue::from_str("no texture"))?;

        gl.use_program(Some(&program));
        gl.enable(Gl::BLEND);
        gl.blend_func(Gl::SRC_ALPHA, Gl::ONE_MINUS_SRC_ALPHA);

        let mut renderer = Self {
            gl,
            program,
            vbo,
            ibo,
            atlas: TextAtlas::new(1024, 1024),
            atlas_texture,
            width,
            height,
        };
        renderer.init_atlas_texture();
        renderer.resize(width, height);
        Ok(renderer)
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
        self.gl.viewport(0, 0, width as i32, height as i32);
    }

    pub fn set_font_bytes(&mut self, bytes: Vec<u8>) {
        self.atlas.set_font_bytes(bytes);
    }

    pub fn render(&mut self, mut batch: Batch, text_runs: Vec<TextRun>) -> Result<(), JsValue> {
        for run in text_runs {
            self.push_text_quads(&mut batch, run);
        }
        let merged = batch;
        self.upload_atlas_if_needed();
        self.draw_batch(&merged)
    }

    fn push_text_quads(&mut self, batch: &mut Batch, run: TextRun) {
        let mut x = run.rect.x;
        let mut y = run.rect.y + run.rect.h * 0.7;
        let font_size = run.font_size;
        let line_height = font_size * 1.4;
        for ch in run.text.chars() {
            if ch == '\n' {
                x = run.rect.x;
                y += line_height;
                continue;
            }
            let glyph = self.atlas.ensure_glyph(ch, font_size);
            let rect = Rect::new(
                x + glyph.bearing.x,
                y - glyph.size.y + glyph.bearing.y,
                glyph.size.x,
                glyph.size.y,
            );
            batch.push_quad(
                Quad {
                    rect,
                    uv: glyph.uv,
                    color: run.color,
                    flags: 1,
                },
                Material::TextAtlas,
                run.clip,
            );
            x += glyph.advance;
        }
    }

    fn init_atlas_texture(&mut self) {
        let gl = &self.gl;
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.atlas_texture));
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MIN_FILTER, Gl::LINEAR as i32);
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MAG_FILTER, Gl::LINEAR as i32);
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_S, Gl::CLAMP_TO_EDGE as i32);
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE as i32);
        let data = self.atlas.pixels();
        let width = 1024;
        let height = 1024;
        gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
            Gl::TEXTURE_2D,
            0,
            Gl::R8 as i32,
            width,
            height,
            0,
            Gl::RED,
            Gl::UNSIGNED_BYTE,
            Some(data),
        )
        .ok();
        self.atlas.mark_clean();
    }

    fn upload_atlas_if_needed(&mut self) {
        if !self.atlas.is_dirty() {
            return;
        }
        let gl = &self.gl;
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.atlas_texture));
        let data = self.atlas.pixels();
        gl.tex_sub_image_2d_with_i32_and_i32_and_u32_and_type_and_opt_u8_array(
            Gl::TEXTURE_2D,
            0,
            0,
            0,
            1024,
            1024,
            Gl::RED,
            Gl::UNSIGNED_BYTE,
            Some(data),
        )
        .ok();
        self.atlas.mark_clean();
    }

    fn draw_batch(&mut self, batch: &Batch) -> Result<(), JsValue> {
        let gl = &self.gl;
        gl.use_program(Some(&self.program));

        let u_resolution = gl.get_uniform_location(&self.program, "u_resolution");
        if let Some(loc) = u_resolution {
            gl.uniform2f(Some(&loc), self.width, self.height);
        }

        let mut vertex_data: Vec<f32> = Vec::with_capacity(batch.vertices.len() * 9);
        for v in &batch.vertices {
            vertex_data.push(v.pos.x);
            vertex_data.push(v.pos.y);
            vertex_data.push(v.uv.x);
            vertex_data.push(v.uv.y);
            vertex_data.push(v.color.r);
            vertex_data.push(v.color.g);
            vertex_data.push(v.color.b);
            vertex_data.push(v.color.a);
            vertex_data.push(v.flags as f32);
        }
        let index_data = batch.indices.clone();

        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.vbo));
        unsafe {
            let vert_array = js_sys::Float32Array::view(&vertex_data);
            gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &vert_array, Gl::DYNAMIC_DRAW);
        }
        gl.bind_buffer(Gl::ELEMENT_ARRAY_BUFFER, Some(&self.ibo));
        unsafe {
            let idx_array = js_sys::Uint32Array::view(&index_data);
            gl.buffer_data_with_array_buffer_view(Gl::ELEMENT_ARRAY_BUFFER, &idx_array, Gl::DYNAMIC_DRAW);
        }

        let stride = 9 * 4;
        let a_pos = gl.get_attrib_location(&self.program, "a_pos") as u32;
        let a_uv = gl.get_attrib_location(&self.program, "a_uv") as u32;
        let a_color = gl.get_attrib_location(&self.program, "a_color") as u32;
        let a_flags = gl.get_attrib_location(&self.program, "a_flags") as u32;

        gl.enable_vertex_attrib_array(a_pos);
        gl.vertex_attrib_pointer_with_i32(a_pos, 2, Gl::FLOAT, false, stride, 0);

        gl.enable_vertex_attrib_array(a_uv);
        gl.vertex_attrib_pointer_with_i32(a_uv, 2, Gl::FLOAT, false, stride, 2 * 4);

        gl.enable_vertex_attrib_array(a_color);
        gl.vertex_attrib_pointer_with_i32(a_color, 4, Gl::FLOAT, false, stride, 4 * 4);

        gl.enable_vertex_attrib_array(a_flags);
        gl.vertex_attrib_pointer_with_i32(a_flags, 1, Gl::FLOAT, false, stride, 8 * 4);

        gl.clear_color(0.97, 0.97, 0.96, 1.0);
        gl.clear(Gl::COLOR_BUFFER_BIT);

        for cmd in &batch.commands {
            match cmd.material {
                Material::TextAtlas => self.bind_text_texture(),
                Material::Solid => self.unbind_text_texture(),
                Material::IconAtlas => self.bind_text_texture(),
            }

            if let Some(clip) = cmd.clip {
                gl.enable(Gl::SCISSOR_TEST);
                gl.scissor(
                    clip.x as i32,
                    (self.height - clip.y - clip.h) as i32,
                    clip.w as i32,
                    clip.h as i32,
                );
            } else {
                gl.disable(Gl::SCISSOR_TEST);
            }

            gl.draw_elements_with_i32(
                Gl::TRIANGLES,
                cmd.count as i32,
                Gl::UNSIGNED_INT,
                (cmd.start * 4) as i32,
            );
        }

        Ok(())
    }

    fn bind_text_texture(&self) {
        let gl = &self.gl;
        gl.active_texture(Gl::TEXTURE0);
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.atlas_texture));
        if let Some(loc) = gl.get_uniform_location(&self.program, "u_use_texture") {
            gl.uniform1i(Some(&loc), 1);
        }
        if let Some(loc) = gl.get_uniform_location(&self.program, "u_atlas") {
            gl.uniform1i(Some(&loc), 0);
        }
    }

    fn unbind_text_texture(&self) {
        let gl = &self.gl;
        gl.bind_texture(Gl::TEXTURE_2D, None);
        if let Some(loc) = gl.get_uniform_location(&self.program, "u_use_texture") {
            gl.uniform1i(Some(&loc), 0);
        }
    }
}

fn compile_shader(gl: &Gl, source: &str, shader_type: u32) -> Result<WebGlShader, JsValue> {
    let shader = gl
        .create_shader(shader_type)
        .ok_or_else(|| JsValue::from_str("unable to create shader"))?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);
    if gl
        .get_shader_parameter(&shader, Gl::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(shader)
    } else {
        Err(JsValue::from_str(
            &gl.get_shader_info_log(&shader).unwrap_or_default(),
        ))
    }
}

fn link_program(gl: &Gl, vert_src: &str, frag_src: &str) -> Result<WebGlProgram, JsValue> {
    let vert = compile_shader(gl, vert_src, Gl::VERTEX_SHADER)?;
    let frag = compile_shader(gl, frag_src, Gl::FRAGMENT_SHADER)?;
    let program = gl
        .create_program()
        .ok_or_else(|| JsValue::from_str("unable to create program"))?;
    gl.attach_shader(&program, &vert);
    gl.attach_shader(&program, &frag);
    gl.link_program(&program);
    if gl
        .get_program_parameter(&program, Gl::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(program)
    } else {
        Err(JsValue::from_str(
            &gl.get_program_info_log(&program).unwrap_or_default(),
        ))
    }
}

const VERT_SHADER: &str = r#"
attribute vec2 a_pos;
attribute vec2 a_uv;
attribute vec4 a_color;
attribute float a_flags;
uniform vec2 u_resolution;
varying vec2 v_uv;
varying vec4 v_color;
varying float v_flags;
void main() {
  vec2 zeroToOne = a_pos / u_resolution;
  vec2 zeroToTwo = zeroToOne * 2.0;
  vec2 clipSpace = zeroToTwo - 1.0;
  gl_Position = vec4(clipSpace.x, -clipSpace.y, 0.0, 1.0);
  v_uv = a_uv;
  v_color = a_color;
  v_flags = a_flags;
}
"#;

const FRAG_SHADER: &str = r#"
precision mediump float;
varying vec2 v_uv;
varying vec4 v_color;
uniform sampler2D u_atlas;
uniform int u_use_texture;
void main() {
  if (u_use_texture == 1) {
    float a = texture2D(u_atlas, v_uv).r;
    gl_FragColor = vec4(v_color.rgb, v_color.a * a);
  } else {
    gl_FragColor = v_color;
  }
}
"#;
