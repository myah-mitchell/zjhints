//! Shared building blocks for the zj family of Zellij plugins.
//!
//! Each plugin in this workspace is its own full-width bar. What they have in
//! common lives here, so that one `color_theme` setting gives every bar the
//! same look and the screen that edits their options is written once.
pub mod ansi;
pub mod bar;
pub mod config;
pub mod dim;
pub mod fit;
pub mod format;
pub mod host;
pub mod mouse;
pub mod nesting;
pub mod panes;
pub mod popup;
pub mod settings;
pub mod theme;
pub mod visibility;
