//! `wham-elements` — HTML-spec element layer for the wham GPU-rendered forms library.
//!
//! This crate provides higher-level element abstractions built on top of the
//! `wham-core` primitives. Elements map closely to HTML/ARIA concepts: buttons,
//! links, text nodes, and groups. They carry semantic metadata (roles, labels,
//! states) and keyboard-interaction logic without coupling to any renderer.
//!
//! # No browser APIs
//!
//! This crate has no dependency on `wasm-bindgen`, `web-sys`, or any browser API.
//! It compiles and tests with `cargo test` on any host platform.

pub mod button;
pub mod link;
pub mod text;

pub use button::{Button, ButtonKind, ButtonState};
pub use link::{Link, LinkState};
pub use text::{TextNode, TextVariant};
