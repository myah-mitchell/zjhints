//! The zjtaskbar plugin: registers the bar the library draws with Zellij. See
//! `lib.rs` for the bar itself.

use zellij_tile::prelude::*;
use zjtaskbar::State;

register_plugin!(State);
