//! Navigation components for the wham UI kit.
//!
//! All five components are pure-data state machines. They do not depend on any
//! renderer, browser API, or async runtime. Each component exposes:
//!
//! - A builder API for constructing the initial state.
//! - A `handle_key` method that accepts a [`wham_core::input::InputEvent`] and
//!   returns a component-specific event enum.
//!
//! # Components
//!
//! | Component | ARIA landmark |
//! |-----------|--------------|
//! | [`Navbar`] | `banner` + `navigation` |
//! | [`Sidebar`] | `navigation` |
//! | [`Breadcrumb`] | `navigation` (`aria-label="Breadcrumb"`) |
//! | [`Tabs`] | `tablist` / `tab` / `tabpanel` |
//! | [`Pagination`] | `navigation` (`aria-label="Pagination"`) |

pub mod breadcrumb;
pub mod navbar;
pub mod pagination;
pub mod sidebar;
pub mod tabs;

pub use breadcrumb::{Breadcrumb, BreadcrumbEvent, BreadcrumbItem};
pub use navbar::{NavLink, Navbar, NavbarEvent};
pub use pagination::{Pagination, PaginationEvent, PaginationFocus};
pub use sidebar::{Sidebar, SidebarEvent, SidebarFocus, SidebarItem, SidebarSection};
pub use tabs::{TabItem, TabOrientation, Tabs, TabsEvent};
