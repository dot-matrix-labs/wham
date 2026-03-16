use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    HtmlCanvasElement, WebGl2RenderingContext as Gl, WebGlBuffer, WebGlProgram, WebGlShader,
    WebGlTexture, WebGlUniformLocation,
};

use ui_core::batch::{Batch, DirtyTracker, Material, Quad, TextRun};
use ui_core::types::Rect;

use crate::atlas::TextAtlas;
use crate::icon_atlas::IconAtlas;

pub struct Renderer {
    gl: Gl,
    program: WebGlProgram,
    vbo: WebGlBuffer,
    ibo: WebGlBuffer,
    atlas: TextAtlas,
    /// One GPU texture per atlas page.
    atlas_textures: Vec<WebGlTexture>,
    icon_atlas: IconAtlas,
    icon_texture: WebGlTexture,
    width: f32,
    height: f32,
    context_valid: bool,

    // -----------------------------------------------------------------------
    // Cached uniform and attribute locations.
    //
    // Uniform/attrib locations are stable for the lifetime of a linked
    // program.  They MUST be queried once at init time (or after context
    // restoration via `reinitialize`) and stored here.
    //
    // NEVER call gl.get_uniform_location() or gl.get_attrib_location() on
    // the hot (per-frame) render path — those are string lookups into the
    // driver's symbol table and have measurable cost.
    //
    // When adding a new uniform or attribute to a shader, add its cached
    // location field here and populate it in `cache_locations()`.
    // -----------------------------------------------------------------------

    /// `u_resolution` — canvas size in pixels; set once per frame.
    uloc_resolution: Option<WebGlUniformLocation>,
    /// `u_material` — selects solid / text-atlas / icon-atlas mode.
    uloc_material: Option<WebGlUniformLocation>,
    /// `u_text_atlas` — sampler for the glyph atlas (texture unit 0).
    uloc_text_atlas: Option<WebGlUniformLocation>,
    /// `u_icon_atlas` — sampler for the icon atlas (texture unit 1).
    uloc_icon_atlas: Option<WebGlUniformLocation>,

    /// `a_pos` attribute index.
    aloc_pos: u32,
    /// `a_uv` attribute index.
    aloc_uv: u32,
    /// `a_color` attribute index.
    aloc_color: u32,
    /// `a_flags` attribute index.
    aloc_flags: u32,

    /// Number of floats currently allocated in the VBO on the GPU.
    /// Used to decide whether `bufferSubData` is safe (i.e. the new data
    /// fits within the already-allocated GPU buffer) or a full
    /// `bufferData` reallocation is needed.
    vbo_capacity_floats: usize,
    /// Number of u32 indices currently allocated in the IBO on the GPU.
    ibo_capacity_indices: usize,

    // -----------------------------------------------------------------------
    // Instanced rendering resources.
    //
    // Repeated solid primitives (e.g. a column of buttons or text inputs that
    // share the same size) are drawn with a single `drawElementsInstanced`
    // call.  A unit quad (one 1×1 quad at the origin) is uploaded once;
    // per-instance attributes carry the rect and color offset for each
    // primitive.
    //
    // Minimum number of quads in a DrawCmd before the instanced path is
    // chosen over the regular per-vertex path.
    // -----------------------------------------------------------------------

    /// Shader program for instanced solid-quad drawing.
    inst_program: WebGlProgram,
    /// Per-instance attribute buffer: [x, y, w, h, r, g, b, a] × N instances.
    inst_attr_vbo: WebGlBuffer,
    /// Unit quad vertex buffer (four corners of a 0..1 × 0..1 quad).
    unit_quad_vbo: WebGlBuffer,
    /// Unit quad index buffer (two triangles: 0,1,2 / 0,2,3).
    unit_quad_ibo: WebGlBuffer,

    /// Cached `u_resolution` location in the instanced program.
    inst_uloc_resolution: Option<WebGlUniformLocation>,
    /// Cached `a_local_pos` location in the instanced program.
    inst_aloc_local_pos: u32,
    /// Cached `a_inst_rect` location in the instanced program.
    inst_aloc_inst_rect: u32,
    /// Cached `a_inst_color` location in the instanced program.
    inst_aloc_inst_color: u32,
}

impl Renderer {
    pub fn new(canvas: &HtmlCanvasElement, width: f32, height: f32) -> Result<Self, JsValue> {
        let gl: Gl = canvas
            .get_context("webgl2")?
            .ok_or_else(|| JsValue::from_str("WebGL2 not supported"))?
            .dyn_into()?;

        let (program, vbo, ibo) = create_gpu_resources(&gl)?;
        let (inst_program, inst_attr_vbo, unit_quad_vbo, unit_quad_ibo) =
            create_instanced_resources(&gl)?;

        gl.use_program(Some(&program));
        gl.enable(Gl::BLEND);
        gl.blend_func(Gl::SRC_ALPHA, Gl::ONE_MINUS_SRC_ALPHA);

        // Create the initial texture for page 0.
        let tex = create_atlas_texture(&gl)?;
        let atlas_textures = vec![tex];
        let icon_texture = gl.create_texture().ok_or_else(|| JsValue::from_str("no icon texture"))?;

        let mut renderer = Self {
            gl,
            program,
            vbo,
            ibo,
            atlas: TextAtlas::new(1024, 1024),
            atlas_textures,
            icon_atlas: IconAtlas::new(),
            icon_texture,
            width,
            height,
            context_valid: true,
            // Populated immediately below by `cache_locations()`.
            uloc_resolution: None,
            uloc_material: None,
            uloc_text_atlas: None,
            uloc_icon_atlas: None,
            aloc_pos: 0,
            aloc_uv: 0,
            aloc_color: 0,
            aloc_flags: 0,
            vbo_capacity_floats: 0,
            ibo_capacity_indices: 0,
            inst_program,
            inst_attr_vbo,
            unit_quad_vbo,
            unit_quad_ibo,
            inst_uloc_resolution: None,
            inst_aloc_local_pos: 0,
            inst_aloc_inst_rect: 0,
            inst_aloc_inst_color: 0,
        };
        renderer.cache_locations();
        renderer.init_atlas_textures();
        renderer.resize(width, height);
        Ok(renderer)
    }

    /// Returns `true` if the WebGL context is currently valid for rendering.
    pub fn is_context_valid(&self) -> bool {
        self.context_valid
    }

    /// Mark the context as lost. Called from JS when the `webglcontextlost`
    /// event fires. While the context is lost, `render()` is a no-op.
    pub fn notify_context_lost(&mut self) {
        self.context_valid = false;
    }

    /// Recreate all GPU resources after a WebGL context restoration.
    ///
    /// The GL context object itself survives context loss (the browser resets
    /// its internal state but the JS/Rust wrapper remains valid), so we only
    /// need to recreate shaders, programs, buffers, textures, and re-upload
    /// the glyph atlas.
    pub fn reinitialize(&mut self) -> Result<(), JsValue> {
        let (program, vbo, ibo) = create_gpu_resources(&self.gl)?;
        let (inst_program, inst_attr_vbo, unit_quad_vbo, unit_quad_ibo) =
            create_instanced_resources(&self.gl)?;

        self.program = program;
        self.vbo = vbo;
        self.ibo = ibo;
        self.inst_program = inst_program;
        self.inst_attr_vbo = inst_attr_vbo;
        self.unit_quad_vbo = unit_quad_vbo;
        self.unit_quad_ibo = unit_quad_ibo;

        // Recreate textures for all atlas pages.
        self.atlas_textures.clear();
        for _ in 0..self.atlas.page_count() {
            let tex = create_atlas_texture(&self.gl)?;
            self.atlas_textures.push(tex);
        }
        self.icon_texture = self.gl.create_texture().ok_or_else(|| JsValue::from_str("no icon texture"))?;

        self.gl.use_program(Some(&self.program));
        self.gl.enable(Gl::BLEND);
        self.gl.blend_func(Gl::SRC_ALPHA, Gl::ONE_MINUS_SRC_ALPHA);

        // Re-cache locations for the freshly linked program.
        self.cache_locations();

        // The atlas pixel data in CPU memory is still valid; mark it dirty so
        // the full texture is re-uploaded on the next frame.
        self.atlas.invalidate_gpu_cache();
        self.icon_atlas.invalidate_gpu_cache();
        self.init_atlas_textures();
        self.resize(self.width, self.height);

        // New GPU buffers have no allocated capacity yet.
        self.vbo_capacity_floats = 0;
        self.ibo_capacity_indices = 0;

        self.context_valid = true;
        Ok(())
    }

    /// Query and store all uniform/attribute locations for the current program.
    ///
    /// Must be called once after every `link_program` (at construction and
    /// after context restoration). All per-frame code must use the cached
    /// values stored on `self` — never call `get_uniform_location` or
    /// `get_attrib_location` on the hot path.
    fn cache_locations(&mut self) {
        let gl = &self.gl;
        let prog = &self.program;

        self.uloc_resolution = gl.get_uniform_location(prog, "u_resolution");
        self.uloc_material   = gl.get_uniform_location(prog, "u_material");
        self.uloc_text_atlas = gl.get_uniform_location(prog, "u_text_atlas");
        self.uloc_icon_atlas = gl.get_uniform_location(prog, "u_icon_atlas");

        self.aloc_pos   = gl.get_attrib_location(prog, "a_pos")   as u32;
        self.aloc_uv    = gl.get_attrib_location(prog, "a_uv")    as u32;
        self.aloc_color = gl.get_attrib_location(prog, "a_color") as u32;
        self.aloc_flags = gl.get_attrib_location(prog, "a_flags") as u32;

        // Instanced program locations.
        let inst_prog = &self.inst_program;
        self.inst_uloc_resolution = gl.get_uniform_location(inst_prog, "u_resolution");
        self.inst_aloc_local_pos  = gl.get_attrib_location(inst_prog, "a_local_pos")  as u32;
        self.inst_aloc_inst_rect  = gl.get_attrib_location(inst_prog, "a_inst_rect")  as u32;
        self.inst_aloc_inst_color = gl.get_attrib_location(inst_prog, "a_inst_color") as u32;
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
        self.gl.viewport(0, 0, width as i32, height as i32);
    }

    pub fn set_font_bytes(&mut self, bytes: Vec<u8>) {
        self.atlas.set_font_bytes(bytes);
        // Font changed — reset to one texture for the single page.
        self.atlas_textures.clear();
        if let Ok(tex) = create_atlas_texture(&self.gl) {
            self.atlas_textures.push(tex);
        }
        self.init_atlas_textures();
    }

    pub fn add_fallback_font(&mut self, bytes: Vec<u8>) {
        self.atlas.add_fallback_font(bytes);
    }

    /// Returns a mutable reference to the text atlas so that callers can
    /// pre-rasterize glyphs during the layout pass (before rendering).
    pub fn atlas_mut(&mut self) -> &mut TextAtlas {
        &mut self.atlas
    }

    /// Returns a mutable reference to the icon atlas for loading icon packs.
    pub fn icon_atlas_mut(&mut self) -> &mut IconAtlas {
        &mut self.icon_atlas
    }

    /// Returns a reference to the icon atlas (for reading the icon pack).
    pub fn icon_atlas(&self) -> &IconAtlas {
        &self.icon_atlas
    }

    /// Render a fully-resolved batch. All text runs must have already been
    /// converted to quads (via [`resolve_text_runs`]) before calling this
    /// method — the renderer only performs GPU upload and draw dispatch.
    pub fn render(&mut self, batch: &Batch) -> Result<(), JsValue> {
        self.render_with_dirty(batch, None)
    }

    /// Render a fully-resolved batch with optional dirty-region tracking.
    ///
    /// When `dirty` is `Some(&tracker)` and `tracker.is_fully_dirty()` is
    /// `false`, only the vertex ranges belonging to dirty widgets are
    /// re-uploaded via `gl.bufferSubData()`.  This avoids saturating the
    /// PCIe / UMA bus for frames where only a handful of widgets changed.
    ///
    /// When `dirty` is `None` or the tracker reports a full-dirty frame, the
    /// entire vertex and index buffers are re-uploaded with `gl.bufferData()`.
    pub fn render_with_dirty(
        &mut self,
        batch: &Batch,
        dirty: Option<&DirtyTracker>,
    ) -> Result<(), JsValue> {
        if !self.context_valid {
            return Ok(());
        }
        self.sync_atlas_textures()?;
        self.upload_atlas_if_needed();
        self.upload_icon_atlas_if_needed();
        self.draw_batch_with_dirty(batch, dirty)
    }

    /// Ensure we have GPU textures for all atlas pages.
    fn sync_atlas_textures(&mut self) -> Result<(), JsValue> {
        while self.atlas_textures.len() < self.atlas.page_count() {
            let tex = create_atlas_texture(&self.gl)?;
            let page_idx = self.atlas_textures.len();
            let (pw, ph) = self.atlas.page_dimensions();

            // Initialize the new texture with the page's pixel data.
            self.gl.bind_texture(Gl::TEXTURE_2D, Some(&tex));
            self.gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MIN_FILTER, Gl::LINEAR as i32);
            self.gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MAG_FILTER, Gl::LINEAR as i32);
            self.gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_S, Gl::CLAMP_TO_EDGE as i32);
            self.gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE as i32);
            let data = self.atlas.page_pixels(page_idx);
            self.gl
                .tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                    Gl::TEXTURE_2D,
                    0,
                    Gl::R8 as i32,
                    pw as i32,
                    ph as i32,
                    0,
                    Gl::RED,
                    Gl::UNSIGNED_BYTE,
                    Some(data),
                )
                .ok();

            self.atlas_textures.push(tex);
        }
        Ok(())
    }

    /// Initialize all atlas textures (used at creation and after context restore).
    fn init_atlas_textures(&mut self) {
        let gl = &self.gl;
        let (pw, ph) = self.atlas.page_dimensions();
        for (i, tex) in self.atlas_textures.iter().enumerate() {
            gl.bind_texture(Gl::TEXTURE_2D, Some(tex));
            gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MIN_FILTER, Gl::LINEAR as i32);
            gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MAG_FILTER, Gl::LINEAR as i32);
            gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_S, Gl::CLAMP_TO_EDGE as i32);
            gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE as i32);
            let data = self.atlas.page_pixels(i);
            gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                Gl::TEXTURE_2D,
                0,
                Gl::R8 as i32,
                pw as i32,
                ph as i32,
                0,
                Gl::RED,
                Gl::UNSIGNED_BYTE,
                Some(data),
            )
            .ok();
        }
        self.atlas.mark_clean();
    }

    fn upload_atlas_if_needed(&mut self) {
        if !self.atlas.is_dirty() {
            return;
        }
        let gl = &self.gl;
        let (pw, ph) = self.atlas.page_dimensions();
        for (i, data) in self.atlas.dirty_pages() {
            if let Some(tex) = self.atlas_textures.get(i) {
                gl.bind_texture(Gl::TEXTURE_2D, Some(tex));
                gl.tex_sub_image_2d_with_i32_and_i32_and_u32_and_type_and_opt_u8_array(
                    Gl::TEXTURE_2D,
                    0,
                    0,
                    0,
                    pw as i32,
                    ph as i32,
                    Gl::RED,
                    Gl::UNSIGNED_BYTE,
                    Some(data),
                )
                .ok();
            }
        }
        self.atlas.mark_clean();
    }

    fn upload_icon_atlas_if_needed(&mut self) {
        if !self.icon_atlas.is_dirty() || !self.icon_atlas.is_loaded() {
            return;
        }
        let gl = &self.gl;
        gl.active_texture(Gl::TEXTURE1);
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.icon_texture));
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MIN_FILTER, Gl::LINEAR as i32);
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MAG_FILTER, Gl::LINEAR as i32);
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_S, Gl::CLAMP_TO_EDGE as i32);
        gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE as i32);
        let data = self.icon_atlas.pixels();
        let width = self.icon_atlas.width() as i32;
        let height = self.icon_atlas.height() as i32;
        gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
            Gl::TEXTURE_2D,
            0,
            Gl::RGBA as i32,
            width,
            height,
            0,
            Gl::RGBA,
            Gl::UNSIGNED_BYTE,
            Some(data),
        )
        .ok();
        gl.active_texture(Gl::TEXTURE0);
        self.icon_atlas.mark_clean();
    }

    fn bind_icon_texture(&self) {
        let gl = &self.gl;
        gl.active_texture(Gl::TEXTURE1);
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.icon_texture));
        gl.uniform1i(self.uloc_material.as_ref(), 2);
        gl.uniform1i(self.uloc_icon_atlas.as_ref(), 1);
    }

    fn draw_batch_with_dirty(
        &mut self,
        batch: &Batch,
        dirty: Option<&DirtyTracker>,
    ) -> Result<(), JsValue> {
        let gl = &self.gl;
        gl.use_program(Some(&self.program));

        gl.uniform2f(self.uloc_resolution.as_ref(), self.width, self.height);

        // --- Vertex buffer pack ---
        // Each Vertex is serialised as 9 floats: pos(2) uv(2) color(4) flags(1).
        const FLOATS_PER_VERTEX: usize = 9;
        let total_floats = batch.vertices.len() * FLOATS_PER_VERTEX;
        let total_indices = batch.indices.len();

        // Determine whether we can use bufferSubData (partial update) or must
        // use bufferData (full reallocation).
        //
        // Conditions for partial update:
        // 1. dirty tracker is present and reports a partial-dirty frame.
        // 2. The new data fits within the GPU buffer already allocated (i.e.
        //    the vertex/index counts did not grow since last frame).
        let use_partial = dirty
            .map(|t| !t.is_fully_dirty())
            .unwrap_or(false)
            && total_floats <= self.vbo_capacity_floats
            && total_indices <= self.ibo_capacity_indices;

        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.vbo));
        gl.bind_buffer(Gl::ELEMENT_ARRAY_BUFFER, Some(&self.ibo));

        if use_partial {
            // Partial update: only upload vertex ranges for dirty widgets.
            // The index buffer always needs a full re-upload because the
            // `reuse_widget` path rebases indices — every widget's indices
            // change position even if its vertex data is identical.
            //
            // Vertex data for clean widgets is already correct in the GPU
            // buffer from the previous frame (same byte offsets because the
            // batch was built by copying clean widget data first).

            let tracker = dirty.unwrap();

            // Build a packed f32 array for the entire vertex buffer (same as
            // the full path) — this is needed to identify changed sub-ranges.
            let mut vertex_data: Vec<f32> = Vec::with_capacity(total_floats);
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

            // Upload only the vertex sub-ranges that belong to dirty widgets.
            for (id, range) in batch.widget_ranges() {
                if tracker.is_dirty(*id) {
                    let float_start = range.vertex_start * FLOATS_PER_VERTEX;
                    let float_end = range.vertex_end * FLOATS_PER_VERTEX;
                    let byte_offset = (float_start * 4) as i32;
                    unsafe {
                        let sub = js_sys::Float32Array::view(&vertex_data[float_start..float_end]);
                        gl.buffer_sub_data_with_i32_and_array_buffer_view(
                            Gl::ARRAY_BUFFER,
                            byte_offset,
                            &sub,
                        );
                    }
                }
            }

            // Index buffer: always full-upload (indices are rebased each frame).
            unsafe {
                let idx_array = js_sys::Uint32Array::view(&batch.indices);
                gl.buffer_sub_data_with_i32_and_array_buffer_view(
                    Gl::ELEMENT_ARRAY_BUFFER,
                    0,
                    &idx_array,
                );
            }
        } else {
            // Full upload: pack all vertices and re-allocate GPU buffers.
            let mut vertex_data: Vec<f32> = Vec::with_capacity(total_floats);
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

            unsafe {
                let vert_array = js_sys::Float32Array::view(&vertex_data);
                gl.buffer_data_with_array_buffer_view(
                    Gl::ARRAY_BUFFER,
                    &vert_array,
                    Gl::DYNAMIC_DRAW,
                );
            }
            unsafe {
                let idx_array = js_sys::Uint32Array::view(&batch.indices);
                gl.buffer_data_with_array_buffer_view(
                    Gl::ELEMENT_ARRAY_BUFFER,
                    &idx_array,
                    Gl::DYNAMIC_DRAW,
                );
            }

            // Record new GPU buffer capacities.
            self.vbo_capacity_floats = total_floats;
            self.ibo_capacity_indices = total_indices;
        }

        {
            // Scoped borrow of self.gl so it's dropped before the command
            // loop, which may call self.draw_instanced_solid (a &mut self
            // method).
            let gl = &self.gl;
            let stride = 9 * 4;
            gl.enable_vertex_attrib_array(self.aloc_pos);
            gl.vertex_attrib_pointer_with_i32(self.aloc_pos, 2, Gl::FLOAT, false, stride, 0);

            gl.enable_vertex_attrib_array(self.aloc_uv);
            gl.vertex_attrib_pointer_with_i32(self.aloc_uv, 2, Gl::FLOAT, false, stride, 2 * 4);

            gl.enable_vertex_attrib_array(self.aloc_color);
            gl.vertex_attrib_pointer_with_i32(self.aloc_color, 4, Gl::FLOAT, false, stride, 4 * 4);

            gl.enable_vertex_attrib_array(self.aloc_flags);
            gl.vertex_attrib_pointer_with_i32(self.aloc_flags, 1, Gl::FLOAT, false, stride, 8 * 4);

            gl.clear_color(0.97, 0.97, 0.96, 1.0);
            gl.clear(Gl::COLOR_BUFFER_BIT);
        }

        // Minimum number of quads (each quad = 6 indices) in a solid DrawCmd
        // before switching to the instanced path.
        const INSTANCED_THRESHOLD_QUADS: u32 = 4;

        // Pre-collect commands to avoid borrow issues inside the loop.
        let commands: Vec<_> = batch.commands.iter().cloned().collect();
        let vertices = batch.vertices.clone();
        let indices = batch.indices.clone();

        for cmd in &commands {
            // Solid commands with enough quads and no clip use instanced drawing.
            if cmd.material == Material::Solid
                && cmd.clip.is_none()
                && cmd.count >= INSTANCED_THRESHOLD_QUADS * 6
            {
                self.draw_instanced_solid(cmd, &vertices, &indices);
                // Restore main program and main VBO/IBO bindings after instanced draw.
                let gl = &self.gl;
                gl.use_program(Some(&self.program));
                gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.vbo));
                gl.bind_buffer(Gl::ELEMENT_ARRAY_BUFFER, Some(&self.ibo));
                continue;
            }

            {
                let gl = &self.gl;
                match cmd.material {
                    Material::TextAtlas => {}
                    Material::IconAtlas => {}
                    _ => {}
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
            }
            match cmd.material {
                Material::TextAtlas => self.bind_text_texture(0),
                Material::IconAtlas => self.bind_icon_texture(),
                Material::Solid => self.unbind_text_texture(),
                _ => self.unbind_text_texture(),
            }
            self.gl.draw_elements_with_i32(
                Gl::TRIANGLES,
                cmd.count as i32,
                Gl::UNSIGNED_INT,
                (cmd.start * 4) as i32,
            );
        }

        // Ensure scissor test is off after the loop.
        self.gl.disable(Gl::SCISSOR_TEST);

        Ok(())
    }

    /// Draw a solid DrawCmd using WebGL2 instanced rendering.
    ///
    /// Each group of 6 indices in the command maps to one quad.  The quad's
    /// position, size, and color are extracted from the vertex buffer and
    /// uploaded as per-instance attributes.  A unit quad (0..1 × 0..1) is
    /// then drawn `N` times — once per instance — with the instanced shader
    /// transforming each unit quad into the correct screen-space rect.
    ///
    /// This reduces the number of draw calls to 1 regardless of how many
    /// quads the command contains, and eliminates the per-vertex attribute
    /// fetches for the duplicated corner positions.
    fn draw_instanced_solid(
        &mut self,
        cmd: &ui_core::batch::DrawCmd,
        vertices: &[ui_core::batch::Vertex],
        indices: &[u32],
    ) {
        // Decode per-quad rects + colors from the interleaved vertex buffer.
        // Each quad occupies 4 consecutive vertices (top-left, top-right,
        // bottom-right, bottom-left).  We reconstruct rect from TL + BR.
        let num_quads = (cmd.count / 6) as usize;
        let mut inst_data: Vec<f32> = Vec::with_capacity(num_quads * 8);

        // Walk the index buffer in groups of 6.
        let idx_start = cmd.start as usize;
        for q in 0..num_quads {
            let base_idx = idx_start + q * 6;
            if base_idx + 5 >= indices.len() {
                break;
            }
            // Indices for this quad: [v0, v1, v2, v0, v2, v3]
            let i0 = indices[base_idx] as usize;
            let i2 = indices[base_idx + 2] as usize;
            if i0 >= vertices.len() || i2 >= vertices.len() {
                break;
            }
            let tl = &vertices[i0]; // top-left
            let br = &vertices[i2]; // bottom-right
            let x = tl.pos.x;
            let y = tl.pos.y;
            let w = br.pos.x - tl.pos.x;
            let h = br.pos.y - tl.pos.y;
            inst_data.push(x);
            inst_data.push(y);
            inst_data.push(w);
            inst_data.push(h);
            inst_data.push(tl.color.r);
            inst_data.push(tl.color.g);
            inst_data.push(tl.color.b);
            inst_data.push(tl.color.a);
        }

        let actual_quads = inst_data.len() / 8;
        if actual_quads == 0 {
            return;
        }

        let gl = &self.gl;
        gl.disable(Gl::SCISSOR_TEST);

        // Upload per-instance data.
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.inst_attr_vbo));
        unsafe {
            let arr = js_sys::Float32Array::view(&inst_data);
            gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &arr, Gl::DYNAMIC_DRAW);
        }

        // Switch to the instanced program.
        gl.use_program(Some(&self.inst_program));
        gl.uniform2f(self.inst_uloc_resolution.as_ref(), self.width, self.height);

        // Bind unit quad geometry (vertex positions).
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.unit_quad_vbo));
        gl.enable_vertex_attrib_array(self.inst_aloc_local_pos);
        gl.vertex_attrib_pointer_with_i32(
            self.inst_aloc_local_pos,
            2,         // components (x, y)
            Gl::FLOAT,
            false,
            2 * 4,     // stride: 2 floats
            0,
        );
        gl.vertex_attrib_divisor(self.inst_aloc_local_pos, 0); // per-vertex

        // Bind per-instance attribute buffer.
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.inst_attr_vbo));
        let inst_stride = 8 * 4; // 8 floats × 4 bytes

        // a_inst_rect: x, y, w, h (4 floats at offset 0).
        gl.enable_vertex_attrib_array(self.inst_aloc_inst_rect);
        gl.vertex_attrib_pointer_with_i32(
            self.inst_aloc_inst_rect,
            4,
            Gl::FLOAT,
            false,
            inst_stride,
            0,
        );
        gl.vertex_attrib_divisor(self.inst_aloc_inst_rect, 1); // per-instance

        // a_inst_color: r, g, b, a (4 floats at offset 16).
        gl.enable_vertex_attrib_array(self.inst_aloc_inst_color);
        gl.vertex_attrib_pointer_with_i32(
            self.inst_aloc_inst_color,
            4,
            Gl::FLOAT,
            false,
            inst_stride,
            4 * 4,
        );
        gl.vertex_attrib_divisor(self.inst_aloc_inst_color, 1); // per-instance

        // Bind unit quad index buffer and draw.
        gl.bind_buffer(Gl::ELEMENT_ARRAY_BUFFER, Some(&self.unit_quad_ibo));
        gl.draw_elements_instanced_with_i32(
            Gl::TRIANGLES,
            6,                    // indices per instance (unit quad)
            Gl::UNSIGNED_SHORT,   // unit_quad_ibo uses u16
            0,
            actual_quads as i32,
        );

        // Reset divisors and restore the main program so subsequent
        // non-instanced draw calls work correctly.
        gl.vertex_attrib_divisor(self.inst_aloc_inst_rect, 0);
        gl.vertex_attrib_divisor(self.inst_aloc_inst_color, 0);
        gl.use_program(Some(&self.program));
    }

    fn bind_text_texture(&self, page_idx: usize) {
        let gl = &self.gl;
        gl.active_texture(Gl::TEXTURE0);
        if let Some(tex) = self.atlas_textures.get(page_idx) {
            gl.bind_texture(Gl::TEXTURE_2D, Some(tex));
        }
        gl.uniform1i(self.uloc_material.as_ref(), 1);
        gl.uniform1i(self.uloc_text_atlas.as_ref(), 0);
    }

    fn unbind_text_texture(&self) {
        let gl = &self.gl;
        gl.active_texture(Gl::TEXTURE0);
        gl.bind_texture(Gl::TEXTURE_2D, None);
        gl.uniform1i(self.uloc_material.as_ref(), 0);
    }
}

/// Convert text runs into vertex quads, rasterizing any missing glyphs into
/// the atlas. This should be called **after** the layout pass and **before**
/// [`Renderer::render`] so that the renderer receives an immutable, complete
/// batch and the atlas texture upload happens only once per frame.
pub fn resolve_text_runs(batch: &mut Batch, atlas: &mut TextAtlas) {
    // Take the text runs out of the batch to avoid borrow conflicts.
    let text_runs: Vec<TextRun> = batch.text_runs.drain(..).collect();

    // First pass: ensure all glyphs are cached (rasterization).
    for run in &text_runs {
        atlas.ensure_glyphs_cached(&run.text, run.font_size);
    }

    // Second pass: emit quads using the now-populated atlas.
    for run in &text_runs {
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
            // Glyph is guaranteed to be cached from the first pass.
            let glyph = atlas.get_cached_glyph(ch, font_size).cloned().unwrap_or_else(|| {
                // Fallback: rasterize on demand (should not happen).
                atlas.ensure_glyph(ch, font_size)
            });
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
}

/// Create GPU resources (shader program, VBO, IBO) — but not atlas textures,
/// since those are managed separately per page.
///
/// This is called both at initial construction and after context restoration.
fn create_gpu_resources(gl: &Gl) -> Result<(WebGlProgram, WebGlBuffer, WebGlBuffer), JsValue> {
    let program = link_program(gl, VERT_SHADER, FRAG_SHADER)?;
    let vbo = gl.create_buffer().ok_or_else(|| JsValue::from_str("no vbo"))?;
    let ibo = gl.create_buffer().ok_or_else(|| JsValue::from_str("no ibo"))?;
    Ok((program, vbo, ibo))
}

/// Create GPU resources for the instanced solid-quad rendering path.
///
/// Returns (inst_program, inst_attr_vbo, unit_quad_vbo, unit_quad_ibo).
/// The unit quad buffers are uploaded immediately (static geometry).
fn create_instanced_resources(
    gl: &Gl,
) -> Result<(WebGlProgram, WebGlBuffer, WebGlBuffer, WebGlBuffer), JsValue> {
    let inst_program = link_program(gl, INST_VERT_SHADER, INST_FRAG_SHADER)?;

    // Per-instance attribute buffer — initially empty, grown each frame.
    let inst_attr_vbo = gl.create_buffer().ok_or_else(|| JsValue::from_str("no inst_attr_vbo"))?;

    // Unit quad: four corners of a [0,1]×[0,1] quad.
    // Layout: x, y (2 floats per vertex, 4 vertices = 8 floats total).
    let unit_quad_vbo = gl.create_buffer().ok_or_else(|| JsValue::from_str("no unit_quad_vbo"))?;
    gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&unit_quad_vbo));
    let unit_vertices: [f32; 8] = [
        0.0, 0.0, // top-left
        1.0, 0.0, // top-right
        1.0, 1.0, // bottom-right
        0.0, 1.0, // bottom-left
    ];
    unsafe {
        let arr = js_sys::Float32Array::view(&unit_vertices);
        gl.buffer_data_with_array_buffer_view(Gl::ARRAY_BUFFER, &arr, Gl::STATIC_DRAW);
    }

    // Unit quad indices: two triangles (0,1,2) and (0,2,3).
    let unit_quad_ibo = gl.create_buffer().ok_or_else(|| JsValue::from_str("no unit_quad_ibo"))?;
    gl.bind_buffer(Gl::ELEMENT_ARRAY_BUFFER, Some(&unit_quad_ibo));
    let unit_indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
    unsafe {
        let arr = js_sys::Uint16Array::view(&unit_indices);
        gl.buffer_data_with_array_buffer_view(Gl::ELEMENT_ARRAY_BUFFER, &arr, Gl::STATIC_DRAW);
    }

    Ok((inst_program, inst_attr_vbo, unit_quad_vbo, unit_quad_ibo))
}

/// Create a single atlas texture with standard parameters.
fn create_atlas_texture(gl: &Gl) -> Result<WebGlTexture, JsValue> {
    gl.create_texture().ok_or_else(|| JsValue::from_str("no texture"))
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

const VERT_SHADER: &str = r#"#version 300 es
in vec2 a_pos;
in vec2 a_uv;
in vec4 a_color;
in float a_flags;
uniform vec2 u_resolution;
out vec2 v_uv;
out vec4 v_color;
out float v_flags;
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

const FRAG_SHADER: &str = r#"#version 300 es
precision mediump float;
in vec2 v_uv;
in vec4 v_color;
uniform sampler2D u_text_atlas;
uniform sampler2D u_icon_atlas;
uniform int u_material;
out vec4 fragColor;
void main() {
  if (u_material == 1) {
    float a = texture(u_text_atlas, v_uv).r;
    fragColor = vec4(v_color.rgb, v_color.a * a);
  } else if (u_material == 2) {
    vec4 tex = texture(u_icon_atlas, v_uv);
    fragColor = tex * v_color;
  } else {
    fragColor = v_color;
  }
}
"#;

/// Instanced vertex shader for solid quad rendering.
///
/// Per-vertex: `a_local_pos` — normalized position within the unit quad [0,1].
/// Per-instance: `a_inst_rect` — (x, y, w, h) in pixels; `a_inst_color` — RGBA.
///
/// The vertex shader scales the unit quad to the target rect, then converts
/// pixel coordinates to clip space exactly as the main shader does.
const INST_VERT_SHADER: &str = r#"#version 300 es
in vec2 a_local_pos;
in vec4 a_inst_rect;
in vec4 a_inst_color;
uniform vec2 u_resolution;
out vec4 v_color;
void main() {
  // Scale unit quad to the target pixel rect.
  vec2 pixel_pos = a_inst_rect.xy + a_local_pos * a_inst_rect.zw;
  vec2 clip = (pixel_pos / u_resolution) * 2.0 - 1.0;
  gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);
  v_color = a_inst_color;
}
"#;

/// Instanced fragment shader — outputs the per-instance solid color.
const INST_FRAG_SHADER: &str = r#"#version 300 es
precision mediump float;
in vec4 v_color;
out vec4 fragColor;
void main() {
  fragColor = v_color;
}
"#;
