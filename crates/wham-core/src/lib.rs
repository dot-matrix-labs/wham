//! `wham-core` — platform-agnostic primitive kernel for the wham GPU-rendered forms library.
//!
//! This crate provides the lowest-level primitives used across all wham crates:
//! geometry types, vertex/index batching, hit testing, theming, input events,
//! and COW state utilities. It has no dependency on browser APIs and can be
//! compiled and tested with `cargo test` on any host platform.

/// Role-independent accessibility tree primitives: [`A11yState`], [`A11yNode<R>`], [`A11yTree<R>`].
pub mod accessibility;
/// Vertex/index buffer builder and draw command types for the GPU renderer.
pub mod batch;
/// Spatial hash grid for mapping pointer positions to widget IDs.
pub mod hit_test;
/// Input event types: pointer, keyboard, IME, clipboard, and wheel events.
pub mod input;
/// History stack used for form undo/redo.
pub mod state;
/// Theme tokens: colours, font scale, high-contrast, and reduced-motion flags.
pub mod theme;
/// Primitive geometry types: [`Rect`](types::Rect), [`Vec2`](types::Vec2), [`Color`](types::Color).
pub mod types;

pub use accessibility::{A11yNode, A11yState, A11yTree};
pub use batch::{Batch, DrawCmd, DirtyTracker, Material, Quad, TextRun, Vertex, WidgetId, WidgetRange};
pub use hit_test::{HitTestEntry, HitTestGrid};
pub use input::{InputEvent, KeyCode, Modifiers, PointerButton, PointerEvent, TextInputEvent};
pub use state::History;
pub use theme::{Theme, ThemeColors};
pub use types::{Color, Rect, Vec2};
