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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    Other(u32),
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

