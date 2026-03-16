use serde::Serialize;
use wasm_bindgen::JsValue;
use web_sys::HtmlCanvasElement;

use ui_core::app::FormApp;
use ui_core::form::Form;
use ui_core::icon::IconPack;
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

        let dirty = self.ui.dirty_tracker();
        self.renderer.render_with_dirty(&batch, Some(dirty))?;

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
        // Window resize invalidates every widget's position/size — force a
        // full rebuild on the next frame.
        self.ui.invalidate_all();
    }

    /// Forward a font to the renderer's text atlas.
    pub fn set_font_bytes(&mut self, bytes: Vec<u8>) {
        self.renderer.set_font_bytes(bytes);
    }

    /// Append a fallback font to the renderer's text atlas fallback chain.
    pub fn add_fallback_font(&mut self, bytes: Vec<u8>) {
        self.renderer.add_fallback_font(bytes);
    }

    /// Load an icon pack from raw RGBA pixel data and a JSON manifest.
    pub fn load_icon_pack(
        &mut self,
        rgba_pixels: Vec<u8>,
        width: u32,
        height: u32,
        metadata_json: &str,
    ) -> Result<(), String> {
        let pack = IconPack::from_manifest(metadata_json)?;
        self.ui.set_icon_pack(pack.clone());
        self.renderer
            .icon_atlas_mut()
            .load(rgba_pixels, width, height, pack);
        Ok(())
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
    // Safe area
    // -----------------------------------------------------------------

    /// Update the safe area insets in logical (CSS) pixels.
    ///
    /// Should be called on initial load, on every `resize` event, and on
    /// every `orientationchange` event so that layout always reflects the
    /// current hardware cutout geometry.
    ///
    /// `top`, `right`, `bottom`, `left` correspond to
    /// `env(safe-area-inset-top/right/bottom/left)`.
    pub fn set_safe_area_insets(&mut self, top: f32, right: f32, bottom: f32, left: f32) {
        self.ui.set_safe_area_insets([top, right, bottom, left]);
    }

    // -----------------------------------------------------------------
    // Theming
    // -----------------------------------------------------------------

    /// Switch to the built-in dark (`dark = true`) or light (`dark = false`) theme.
    ///
    /// Call this from JS whenever `prefers-color-scheme` changes or when the
    /// user toggles the theme manually.
    pub fn set_theme(&mut self, dark: bool) {
        let theme = if dark {
            Theme::dark()
        } else {
            Theme::light()
        };
        *self.ui.theme_mut() = theme;
    }

    /// Apply a fully custom theme via individual RGBA color components.
    ///
    /// All channel values should be in `[0.0, 1.0]`.  Accessibility
    /// preferences (reduced_motion, high_contrast, font_scale) stored on
    /// the current theme are preserved so they are not silently overwritten.
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
        use ui_core::types::Color;
        let t = self.ui.theme_mut();
        t.colors.background = Color::rgba(bg_r, bg_g, bg_b, 1.0);
        t.colors.surface = Color::rgba(surface_r, surface_g, surface_b, 1.0);
        t.colors.text = Color::rgba(text_r, text_g, text_b, 1.0);
        t.colors.text_muted = Color::rgba(text_muted_r, text_muted_g, text_muted_b, 1.0);
        t.colors.primary = Color::rgba(primary_r, primary_g, primary_b, 1.0);
        t.colors.error = Color::rgba(error_r, error_g, error_b, 1.0);
        t.colors.success = Color::rgba(success_r, success_g, success_b, 1.0);
        t.colors.focus_ring =
            Color::rgba(focus_ring_r, focus_ring_g, focus_ring_b, focus_ring_a);
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
