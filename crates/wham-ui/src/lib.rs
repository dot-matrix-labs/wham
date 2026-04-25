//! `wham-ui` — rich navigation UI kit built on `wham-core` and `wham-elements`.
//!
//! This crate provides five navigation components as pure-data state machines.
//! They have no dependency on browser APIs, WebGL, or WASM-specific code and
//! can be used and tested on any platform.
//!
//! # Components
//!
//! - [`nav::Navbar`] — top-of-page site navigation bar (ARIA: `banner` + `navigation`)
//! - [`nav::Sidebar`] — collapsible vertical navigation panel (ARIA: `navigation`)
//! - [`nav::Breadcrumb`] — hierarchical path trail (ARIA: `navigation`)
//! - [`nav::Tabs`] — tabbed panel switcher (ARIA: `tablist`/`tab`/`tabpanel`)
//! - [`nav::Pagination`] — page number navigation (ARIA: `navigation`)
//!
//! # No browser APIs
//!
//! This crate must not depend on `wasm-bindgen`, `web-sys`, or any browser
//! API. It depends only on `wham-core` and `wham-elements`.

pub mod nav;
