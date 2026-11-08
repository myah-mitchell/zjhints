//! The zjtaskbar plugin: registers the bar the library draws with Zellij as a
//! shared plugin, with a `State` for each slot. See `lib.rs` for the bar
//! itself and `zjkit::shared` for the slots.

use zellij_tile::prelude::*;
use zjtaskbar::State;

register_shared_plugin!(zjkit::shared::Shared<State>);
