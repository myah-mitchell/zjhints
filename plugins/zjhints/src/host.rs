//! What the bar asks of Zellij: the host it makes its calls through, and the
//! permissions, events and pipe messages those calls use, gathered into the
//! `BarSpec` zjkit runs the bar by.

use crate::{DEFAULT_PIPE_NAME, LEGACY_PIPE_NAME};
use std::collections::BTreeMap;
use zellij_tile::prelude::*;
use zjkit::bar::BarSpec;
use zjkit::visibility;

/// The host `State` makes its calls through: Zellij in the plugin, and a
/// recorder in tests, so they can assert what the bar asked for. This is the
/// plugin's one `cfg(test)` switch; see `zjkit::host`.
#[cfg(not(test))]
pub(crate) type BarHost = zjkit::host::Zellij;
#[cfg(test)]
pub(crate) type BarHost = zjkit::host::Recorder;

/// The title of the settings screen's floating pane.
pub(crate) const SETTINGS_PANE_TITLE: &str = "zjhints settings";

pub(crate) fn pipe_names_from_config(configuration: &BTreeMap<String, String>) -> Vec<String> {
    match configuration.get("pipe_name") {
        Some(name) => vec![name.clone()],
        None => vec![DEFAULT_PIPE_NAME.to_string(), LEGACY_PIPE_NAME.to_string()],
    }
}

/// The pipe message zjstatus reads the text of its `{pipe_<name>}` widgets
/// from.
pub(crate) const ZJSTATUS_PIPE_MESSAGE: &str = "pipe";

/// What the bar always asks for.
pub(crate) const BAR_PERMISSIONS: [PermissionType; 3] = [
    PermissionType::ReadApplicationState,
    PermissionType::MessageAndLaunchOtherPlugins,
    // Adding the bar's entry to Zellij's settings screen, and with `mouse`,
    // opening Zellij's context menu and the popup of a hover preview.
    PermissionType::ChangeApplicationState,
];

/// What `mouse` needs beyond what the bar always asks for.
pub(crate) const MOUSE_PERMISSIONS: [PermissionType; 2] = [
    // Running a clicked hint's actions.
    PermissionType::RunActionsAsUser,
    // Showing the menus of hints and keys, which are prompts.
    PermissionType::OpenTerminalsOrPlugins,
];

/// Every permission the bar asks for. A grant replaces the plugin's
/// permissions rather than adding to them, both for the running instance and
/// in Zellij's cache, so every request has to carry the whole set.
pub(crate) fn bar_permissions(mouse: bool) -> Vec<PermissionType> {
    let mut permissions = BAR_PERMISSIONS.to_vec();
    if mouse {
        permissions.extend(MOUSE_PERMISSIONS);
    }
    permissions
}

/// The events the bar always subscribes to.
pub(crate) const BAR_EVENTS: [EventType; 8] = [
    EventType::ModeUpdate,
    // Has Zellij send the keybindings once, and again when they are
    // reconfigured, rather than attached to every `ModeUpdate`.
    EventType::InitialKeybinds,
    EventType::SessionUpdate,
    EventType::PaneUpdate,
    EventType::PermissionRequestResult,
    EventType::TabUpdate,
    EventType::NestedSessionModeUpdate,
    EventType::NestedSessionEnded,
];

/// The events `mouse` needs beyond what the bar always subscribes to.
pub(crate) const MOUSE_EVENTS: [EventType; 4] = [
    EventType::Mouse,
    EventType::PromptResult,
    EventType::ActionComplete,
    // Opening a hover preview once the pointer has rested on its hint.
    EventType::Timer,
];

/// The pipe message that opens the settings screen, or brings it up.
pub(crate) const OPEN_SETTINGS_MESSAGE: &str = "zjhints::open_settings";

/// The hide, show and toggle messages, and the question a new bar asks the
/// running ones.
pub(crate) static VISIBILITY: visibility::Names = zjkit::visibility_names!("zjhints");

/// What zjkit needs to know to run the bar. See `zjkit::bar`.
pub(crate) static BAR: BarSpec = BarSpec {
    schema: &crate::settings::SCHEMA,
    visibility: &VISIBILITY,
    popup: Some(&crate::popup::NAMES),
    open_settings_message: OPEN_SETTINGS_MESSAGE,
    settings_pane_title: SETTINGS_PANE_TITLE,
    permissions: bar_permissions,
    events: &BAR_EVENTS,
    mouse_events: &MOUSE_EVENTS,
};
