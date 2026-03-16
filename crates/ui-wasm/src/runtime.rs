use serde::Serialize;
use wasm_bindgen::JsValue;
use web_sys::HtmlCanvasElement;

use ui_core::app::FormApp;
use ui_core::form::Form;
use ui_core::input::{
    InputEvent, KeyCode, Modifiers, PointerButton, PointerEvent, TextInputEvent,
};
use ui_core::theme::Theme;
use ui_core::types::Vec2;
use ui_core::ui::{Ui, WidgetKind};

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

use crate::atlas::quantize_font_size;
use crate::renderer::{resolve_text_runs, Renderer};

/// A reusable runtime that drives any `FormApp` implementation in the browser.
///
/// `WasmRuntime` owns the `Ui`, `Renderer`, `Form`, event queue, and the
/// application itself. It provides the frame loop, event forwarding,
/// accessibility tree export, and clipboard request plumbing so that
/// `FormApp` implementations only need to define *what* to render.
pub struct WasmRuntime<A: FormApp> {
    app: A,
    ui: Ui,
    renderer: Renderer,
    form: Form,
    events: Vec<InputEvent>,
    width: f32,
    height: f32,
    scale: f32,
    clipboard_request: Option<String>,
}

impl<A: FormApp> WasmRuntime<A> {
    /// Create a new runtime for the given `FormApp`.
    ///
    /// Initializes the `Ui`, `Renderer`, and `Form` (from `app.schema()`).
    pub fn new(
        canvas: &HtmlCanvasElement,
        width: f32,
        height: f32,
        scale: f32,
        app: A,
    ) -> Result<Self, JsValue> {
        let renderer = Renderer::new(canvas, width, height)?;
        let theme = Theme::default_light();
        let ui = Ui::new(width, height, theme);
        let form = Form::new(app.schema());
        Ok(Self {
            app,
            ui,
            renderer,
            form,
            events: Vec::new(),
            width,
            height,
            scale,
            clipboard_request: None,
        })
    }

    /// Run one frame: begin, build UI, end, resolve text, render.
    ///
    /// Returns the accessibility tree as a `JsValue` for the a11y mirror.
    pub fn frame(&mut self, timestamp_ms: f64) -> Result<JsValue, JsValue> {
        let events = std::mem::take(&mut self.events);
        self.ui
            .begin_frame(events, self.width, self.height, self.scale, timestamp_ms);

        self.app.build(&mut self.ui, &mut self.form);

        let a11y = self.ui.end_frame();
        self.clipboard_request = self.ui.take_clipboard_request();
        let mut batch = self.ui.take_batch();

        // Advance the atlas frame counter for LRU tracking, then resolve
        // text runs into vertex quads (rasterization + quad generation)
        // BEFORE the render pass, so the renderer receives a complete batch.
        self.renderer.atlas_mut().begin_frame();
        resolve_text_runs(&mut batch, self.renderer.atlas_mut());

        // Feed actual glyph advance widths back into Ui so that caret
        // placement and click-to-position use real metrics next frame.
        let advances: Rc<RefCell<HashMap<(char, u16), f32>>> =
            Rc::new(RefCell::new(self.renderer.atlas_mut().advance_map()));
        self.ui.set_char_advance(Box::new(move |ch, font_size| {
            let key = (ch, quantize_font_size(font_size));
            advances
                .borrow()
                .get(&key)
                .copied()
                .unwrap_or(font_size * 0.6)
        }));

        self.renderer.render(&batch)?;

        let serializer =
            serde_wasm_bindgen::Serializer::new().serialize_large_number_types_as_bigints(true);
        let a11y_json = a11y.serialize(&serializer).unwrap_or(JsValue::NULL);
        Ok(a11y_json)
    }

    /// Resize the renderer and update stored dimensions.
    pub fn resize(&mut self, width: f32, height: f32, scale: f32) {
        self.width = width;
        self.height = height;
        self.scale = scale;
        self.renderer.resize(width, height);
    }

    /// Forward a font to the renderer's text atlas.
    pub fn set_font_bytes(&mut self, bytes: Vec<u8>) {
        self.renderer.set_font_bytes(bytes);
    }

    /// Append a fallback font to the renderer's text atlas fallback chain.
    pub fn add_fallback_font(&mut self, bytes: Vec<u8>) {
        self.renderer.add_fallback_font(bytes);
    }

    // -----------------------------------------------------------------
    // Event forwarding
    // -----------------------------------------------------------------

    pub fn handle_pointer_down(
        &mut self,
        x: f32,
        y: f32,
        button: u16,
        ctrl: bool,
        alt: bool,
        shift: bool,
        meta: bool,
    ) {
        self.events.push(InputEvent::PointerDown(PointerEvent {
            pos: Vec2::new(x, y),
            button: Some(map_button(button)),
            modifiers: Modifiers {
                ctrl,
                alt,
                shift,
                meta,
            },
        }));
    }

    pub fn handle_pointer_up(
        &mut self,
        x: f32,
        y: f32,
        button: u16,
        ctrl: bool,
        alt: bool,
        shift: bool,
        meta: bool,
    ) {
        self.events.push(InputEvent::PointerUp(PointerEvent {
            pos: Vec2::new(x, y),
            button: Some(map_button(button)),
            modifiers: Modifiers {
                ctrl,
                alt,
                shift,
                meta,
            },
        }));
    }

    pub fn handle_pointer_move(
        &mut self,
        x: f32,
        y: f32,
        ctrl: bool,
        alt: bool,
        shift: bool,
        meta: bool,
    ) {
        self.events.push(InputEvent::PointerMove(PointerEvent {
            pos: Vec2::new(x, y),
            button: None,
            modifiers: Modifiers {
                ctrl,
                alt,
                shift,
                meta,
            },
        }));
    }

    pub fn handle_wheel(
        &mut self,
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
        ctrl: bool,
        alt: bool,
        shift: bool,
        meta: bool,
    ) {
        self.events.push(InputEvent::PointerWheel {
            pos: Vec2::new(x, y),
            delta: Vec2::new(dx, dy),
            modifiers: Modifiers {
                ctrl,
                alt,
                shift,
                meta,
            },
        });
    }

    pub fn handle_key_down(
        &mut self,
        code: &str,
        ctrl: bool,
        alt: bool,
        shift: bool,
        meta: bool,
    ) {
        self.events.push(InputEvent::KeyDown {
            code: KeyCode::from_code_str(code),
            modifiers: Modifiers {
                ctrl,
                alt,
                shift,
                meta,
            },
        });
    }

    pub fn handle_key_up(
        &mut self,
        code: &str,
        ctrl: bool,
        alt: bool,
        shift: bool,
        meta: bool,
    ) {
        self.events.push(InputEvent::KeyUp {
            code: KeyCode::from_code_str(code),
            modifiers: Modifiers {
                ctrl,
                alt,
                shift,
                meta,
            },
        });
    }

    pub fn handle_text_input(&mut self, text: String) {
        self.events
            .push(InputEvent::TextInput(TextInputEvent { text }));
    }

    pub fn handle_composition_start(&mut self) {
        self.events.push(InputEvent::CompositionStart);
    }

    pub fn handle_composition_update(&mut self, text: String) {
        self.events.push(InputEvent::CompositionUpdate(text));
    }

    pub fn handle_composition_end(&mut self, text: String) {
        self.events.push(InputEvent::CompositionEnd(text));
    }

    pub fn handle_paste(&mut self, text: String) {
        self.events.push(InputEvent::Paste(text));
    }

    // -----------------------------------------------------------------
    // Focus & accessibility queries
    // -----------------------------------------------------------------

    /// Set focus to the widget with the given ID.
    pub fn set_focus(&mut self, id: u64) {
        self.ui.set_focus_by_id(id);
    }

    /// Returns `true` if any widget currently has focus.
    pub fn has_focused_widget(&self) -> bool {
        self.ui.focused_id().is_some()
    }

    /// Returns the kind of the focused widget as a string, or `None`.
    pub fn focused_widget_kind_str(&self) -> Option<&'static str> {
        self.ui.focused_widget_kind().map(|k| match k {
            WidgetKind::Label => "label",
            WidgetKind::Button => "button",
            WidgetKind::Checkbox => "checkbox",
            WidgetKind::Radio => "radio",
            WidgetKind::TextInput => "textinput",
            WidgetKind::Select => "select",
            WidgetKind::Group => "group",
            _ => "unknown",
        })
    }

    /// Returns the bounding rect [x, y, w, h] of the focused widget, or `None`.
    pub fn focused_widget_rect(&self) -> Option<[f32; 4]> {
        self.ui
            .focused_widget_rect()
            .map(|r| [r.x, r.y, r.w, r.h])
    }

    /// Take the clipboard request, if any.
    pub fn take_clipboard_request(&mut self) -> Option<String> {
        self.clipboard_request.take()
    }

    // -----------------------------------------------------------------
    // Context loss
    // -----------------------------------------------------------------

    /// Notify the renderer that the WebGL context has been lost.
    pub fn notify_context_lost(&mut self) {
        self.renderer.notify_context_lost();
    }

    /// Recreate all GPU resources after a WebGL context restoration.
    pub fn reinitialize_renderer(&mut self) -> Result<(), JsValue> {
        self.renderer.reinitialize()
    }
}

fn map_button(button: u16) -> PointerButton {
    match button {
        0 => PointerButton::Left,
        1 => PointerButton::Middle,
        2 => PointerButton::Right,
        other => PointerButton::Other(other),
    }
}
