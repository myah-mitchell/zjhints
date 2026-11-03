//! The zjtabs plugin: registers the bar the library draws with Zellij. See
//! `lib.rs` for the bar itself.

use zellij_tile::prelude::*;
use zjtabs::State;

register_plugin!(State);
