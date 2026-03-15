mod atlas;
mod demo;
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
