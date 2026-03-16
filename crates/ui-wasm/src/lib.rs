mod atlas;
mod demo;
mod icon_atlas;
mod renderer;
pub mod runtime;

use demo::DemoApp;
use runtime::WasmRuntime;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

#[wasm_bindgen]
pub struct WasmApp {
    runtime: WasmRuntime<DemoApp>,
}

#[wasm_bindgen]
impl WasmApp {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas: HtmlCanvasElement, width: f32, height: f32, scale: f32) -> Result<WasmApp, JsValue> {
        let app = DemoApp::new();
        let runtime = WasmRuntime::new(&canvas, width, height, scale, app)?;
        Ok(Self { runtime })
    }

    pub fn resize(&mut self, width: f32, height: f32, scale: f32) {
        self.runtime.resize(width, height, scale);
    }

    pub fn set_font_bytes(&mut self, bytes: Vec<u8>) {
        self.runtime.set_font_bytes(bytes);
    }

    /// Append a fallback font to the font chain.
    ///
    /// When a glyph is missing from the primary font (and any earlier
    /// fallbacks), the atlas will try rasterizing from this font before
    /// falling back to the Unicode replacement character (U+FFFD).
    pub fn add_fallback_font(&mut self, bytes: Vec<u8>) {
        self.runtime.add_fallback_font(bytes);
    }

    /// Load an icon pack from a sprite sheet PNG (raw RGBA bytes) and a JSON
    /// manifest describing the icon positions within the texture.
    ///
    /// The manifest format is:
    /// ```json
    /// {
    ///   "name": "my-icons",
    ///   "texture_size": [512, 512],
    ///   "icons": [
    ///     { "name": "check", "x": 0, "y": 0, "w": 24, "h": 24 },
    ///     ...
    ///   ]
    /// }
    /// ```
    pub fn load_icon_pack(
        &mut self,
        rgba_pixels: Vec<u8>,
        width: u32,
        height: u32,
        metadata_json: &str,
    ) -> Result<(), JsValue> {
        self.runtime
            .load_icon_pack(rgba_pixels, width, height, metadata_json)
            .map_err(|e| JsValue::from_str(&e))
    }

    pub fn frame(&mut self, timestamp_ms: f64) -> Result<JsValue, JsValue> {
        self.runtime.frame(timestamp_ms)
    }

    pub fn handle_pointer_down(&mut self, x: f32, y: f32, button: u16, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        self.runtime.handle_pointer_down(x, y, button, ctrl, alt, shift, meta);
    }

    pub fn handle_pointer_up(&mut self, x: f32, y: f32, button: u16, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        self.runtime.handle_pointer_up(x, y, button, ctrl, alt, shift, meta);
    }

    pub fn handle_pointer_move(&mut self, x: f32, y: f32, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        self.runtime.handle_pointer_move(x, y, ctrl, alt, shift, meta);
    }

    pub fn handle_wheel(&mut self, x: f32, y: f32, dx: f32, dy: f32, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        self.runtime.handle_wheel(x, y, dx, dy, ctrl, alt, shift, meta);
    }

    pub fn handle_key_down(&mut self, code: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        self.runtime.handle_key_down(code, ctrl, alt, shift, meta);
    }

    pub fn handle_key_up(&mut self, code: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) {
        self.runtime.handle_key_up(code, ctrl, alt, shift, meta);
    }

    pub fn handle_text_input(&mut self, text: String) {
        self.runtime.handle_text_input(text);
    }

    pub fn handle_composition_start(&mut self) {
        self.runtime.handle_composition_start();
    }

    pub fn handle_composition_update(&mut self, text: String) {
        self.runtime.handle_composition_update(text);
    }

    pub fn handle_composition_end(&mut self, text: String) {
        self.runtime.handle_composition_end(text);
    }

    pub fn handle_paste(&mut self, text: String) {
        self.runtime.handle_paste(text);
    }

    pub fn take_clipboard_request(&mut self) -> Option<String> {
        self.runtime.take_clipboard_request()
    }

    /// Notify the renderer that the WebGL context has been lost.
    ///
    /// While the context is lost, `frame()` will skip all GPU work but
    /// continue updating application state so no user data is lost.
    pub fn notify_context_lost(&mut self) {
        self.runtime.notify_context_lost();
    }

    /// Recreate all GPU resources after a WebGL context restoration.
    ///
    /// Must be called from the `webglcontextrestored` event handler.
    /// Returns an error if resource creation fails.
    pub fn reinitialize_renderer(&mut self) -> Result<(), JsValue> {
        self.runtime.reinitialize_renderer()
    }

    /// Update the safe area insets in logical (CSS) pixels.
    ///
    /// Call this on page load, on every `resize` event, and on every
    /// `orientationchange` event. Values correspond to
    /// `env(safe-area-inset-top/right/bottom/left)`. On desktop or
    /// SE-style phones without hardware cutouts all values should be `0`.
    pub fn set_safe_area_insets(&mut self, top: f32, right: f32, bottom: f32, left: f32) {
        self.runtime.set_safe_area_insets(top, right, bottom, left);
    }

    // -----------------------------------------------------------------
    // Theming
    // -----------------------------------------------------------------

    /// Switch to the built-in dark (`dark = true`) or light (`dark = false`) theme.
    pub fn set_theme(&mut self, dark: bool) {
        self.runtime.set_theme(dark);
    }

    /// Apply a fully custom theme via individual RGBA components.
    ///
    /// All channel values should be in `[0.0, 1.0]`.  Accessibility
    /// preferences (reduced_motion, high_contrast, font_scale) stored on the
    /// current theme are preserved.
    #[allow(clippy::too_many_arguments)]
    pub fn set_custom_theme(
        &mut self,
        bg_r: f32, bg_g: f32, bg_b: f32,
        surface_r: f32, surface_g: f32, surface_b: f32,
        text_r: f32, text_g: f32, text_b: f32,
        text_muted_r: f32, text_muted_g: f32, text_muted_b: f32,
        primary_r: f32, primary_g: f32, primary_b: f32,
        error_r: f32, error_g: f32, error_b: f32,
        success_r: f32, success_g: f32, success_b: f32,
        focus_ring_r: f32, focus_ring_g: f32, focus_ring_b: f32, focus_ring_a: f32,
    ) {
        self.runtime.set_custom_theme(
            bg_r, bg_g, bg_b,
            surface_r, surface_g, surface_b,
            text_r, text_g, text_b,
            text_muted_r, text_muted_g, text_muted_b,
            primary_r, primary_g, primary_b,
            error_r, error_g, error_b,
            success_r, success_g, success_b,
            focus_ring_r, focus_ring_g, focus_ring_b, focus_ring_a,
        );
    }

    /// Set focus to the widget with the given ID.
    /// Called from the accessibility mirror when the screen reader moves focus.
    pub fn set_focus(&mut self, id: f64) {
        self.runtime.set_focus(id as u64);
    }

    /// Returns `true` if any widget currently has focus.
    pub fn has_focused_widget(&self) -> bool {
        self.runtime.has_focused_widget()
    }

    /// Returns the kind of the focused widget as a string (e.g. "textinput",
    /// "button"), or `null` if no widget is focused.
    pub fn focused_widget_kind(&self) -> JsValue {
        match self.runtime.focused_widget_kind_str() {
            Some(kind) => JsValue::from_str(kind),
            None => JsValue::NULL,
        }
    }

    /// Returns the focused widget's bounding rect as [x, y, w, h] in canvas
    /// pixels, or `null` if no widget is focused.
    pub fn focused_widget_rect(&self) -> JsValue {
        match self.runtime.focused_widget_rect() {
            Some([x, y, w, h]) => {
                let arr = js_sys::Float32Array::new_with_length(4);
                arr.copy_from(&[x, y, w, h]);
                arr.into()
            }
            None => JsValue::NULL,
        }
    }
}
