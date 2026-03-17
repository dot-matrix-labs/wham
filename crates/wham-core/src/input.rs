use crate::types::Vec2;

#[derive(Clone, Copy, Debug, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Left,
    Middle,
    Right,
    Other(u16),
}

#[derive(Clone, Copy, Debug)]
pub struct PointerEvent {
    pub pos: Vec2,
    pub button: Option<PointerButton>,
    pub modifiers: Modifiers,
}

/// Physical key identifiers matching the Web KeyboardEvent.code specification.
///
/// Named variants cover the keys used by the UI framework for shortcuts and
/// navigation.  Any unrecognised physical key is represented as
/// `Other(String)` carrying the raw `KeyboardEvent.code` value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyCode {
    Backspace,
    Delete,
    Enter,
    Escape,
    Tab,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    A,
    C,
    V,
    X,
    Z,
    Y,
    Other(String),
}

impl KeyCode {
    /// Parse a `KeyboardEvent.code` string into a `KeyCode`.
    pub fn from_code_str(code: &str) -> Self {
        match code {
            "Backspace" => KeyCode::Backspace,
            "Tab" => KeyCode::Tab,
            "Enter" | "NumpadEnter" => KeyCode::Enter,
            "Escape" => KeyCode::Escape,
            "Insert" => KeyCode::Insert,
            "Delete" => KeyCode::Delete,
            "ArrowLeft" => KeyCode::ArrowLeft,
            "ArrowUp" => KeyCode::ArrowUp,
            "ArrowRight" => KeyCode::ArrowRight,
            "ArrowDown" => KeyCode::ArrowDown,
            "Home" => KeyCode::Home,
            "End" => KeyCode::End,
            "PageUp" => KeyCode::PageUp,
            "PageDown" => KeyCode::PageDown,
            "KeyA" => KeyCode::A,
            "KeyC" => KeyCode::C,
            "KeyV" => KeyCode::V,
            "KeyX" => KeyCode::X,
            "KeyZ" => KeyCode::Z,
            "KeyY" => KeyCode::Y,
            other => KeyCode::Other(other.to_string()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TextInputEvent {
    pub text: String,
}

#[derive(Clone, Debug)]
pub enum InputEvent {
    PointerDown(PointerEvent),
    PointerUp(PointerEvent),
    PointerMove(PointerEvent),
    PointerWheel { pos: Vec2, delta: Vec2, modifiers: Modifiers },
    KeyDown { code: KeyCode, modifiers: Modifiers },
    KeyUp { code: KeyCode, modifiers: Modifiers },
    TextInput(TextInputEvent),
    CompositionStart,
    CompositionUpdate(String),
    CompositionEnd(String),
    Paste(String),
}

