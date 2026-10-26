//! The bar's calls into Zellij that tests stub out, and the permissions,
//! events and pipe messages they use.

use crate::{theme, CONFIG_AMBIGUOUS_WIDTH, DEFAULT_PIPE_NAME, LEGACY_PIPE_NAME};
use std::collections::BTreeMap;
use zellij_tile::prelude::*;
use zjkit::visibility;

pub(crate) fn pipe_names_from_config(configuration: &BTreeMap<String, String>) -> Vec<String> {
    match configuration.get("pipe_name") {
        Some(name) => vec![name.clone()],
        None => vec![DEFAULT_PIPE_NAME.to_string(), LEGACY_PIPE_NAME.to_string()],
    }
}

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

/// Ask every running bar whether it is hidden. See `visibility::Names::ask`.
#[cfg(not(test))]
pub(crate) fn ask_visibility(alias: Option<&str>) {
    let mut message = MessageToPlugin::new(VISIBILITY.ask);
    if let Some(alias) = alias {
        message = message.with_payload(alias);
    }
    pipe_message_to_plugin(message);
}

#[cfg(test)]
pub(crate) fn ask_visibility(_alias: Option<&str>) {}

/// Tell bar `plugin_id` whether this one is hidden, as `answer` says.
#[cfg(not(test))]
pub(crate) fn answer_visibility(plugin_id: u32, answer: &str) {
    pipe_message_to_plugin(
        MessageToPlugin::new(VISIBILITY.answer)
            .with_destination_plugin_id(plugin_id)
            .with_payload(answer),
    );
}

#[cfg(test)]
pub(crate) fn answer_visibility(_plugin_id: u32, _answer: &str) {}

/// Log each theme option naming no theme. Called where a configuration
/// arrives, not each time the palette changes and it is applied again.
pub(crate) fn log_unknown_themes(configuration: &BTreeMap<String, String>) {
    for (option, name) in theme::unknown_names(configuration) {
        eprintln!("zjhints: unknown {option} \"{name}\", using \"zellij\"");
    }
}

pub(crate) fn parse_wide_ambiguous(configuration: &BTreeMap<String, String>) -> bool {
    configuration
        .get(CONFIG_AMBIGUOUS_WIDTH)
        .is_some_and(|value| value.trim() == "2")
}

/// Open a hover preview at the bar pane's `line` and `column`, without taking
/// the focus. Zellij keeps it on screen, above a bar at the bottom.
#[cfg(not(test))]
pub(crate) fn open_preview_popup(
    url: &str,
    configuration: BTreeMap<String, String>,
    line: usize,
    column: usize,
    width: usize,
    height: usize,
) {
    open_plugin_popup_with_options(
        url,
        configuration,
        line,
        column,
        width,
        height,
        PopupOptions::default().focused(false),
    );
}

#[cfg(test)]
pub(crate) fn open_preview_popup(
    _url: &str,
    _configuration: BTreeMap<String, String>,
    _line: usize,
    _column: usize,
    _width: usize,
    _height: usize,
) {
}

#[cfg(not(test))]
pub(crate) fn close_preview_popup(popup: u32) {
    pipe_message_to_plugin(
        MessageToPlugin::new(crate::popup::NAMES.close).with_destination_plugin_id(popup),
    );
}

#[cfg(test)]
pub(crate) fn close_preview_popup(_popup: u32) {}

#[cfg(not(test))]
pub(crate) fn start_hover_delay(seconds: f64) {
    set_timeout(seconds);
}

#[cfg(test)]
pub(crate) fn start_hover_delay(_seconds: f64) {}

/// Open the settings screen as a floating pane, or reach the one already
/// open. `zellij:OWN_URL` stands for this plugin's own location, and a
/// message addressed by URL goes to a running instance whose configuration
/// matches exactly, launching one only when there is none.
///
/// `own_url` is the clicked bar's URL. When it is an alias name rather than a
/// location (which always has a scheme, as in `file:`), the screen edits that
/// alias.
pub(crate) fn open_settings(own_url: Option<&str>) {
    pipe_message_to_plugin(
        MessageToPlugin::new(OPEN_SETTINGS_MESSAGE)
            .with_plugin_url("zellij:OWN_URL")
            .with_plugin_config(settings_configuration(own_url))
            .new_plugin_instance_should_float(true)
            .new_plugin_instance_should_be_focused()
            .new_plugin_instance_should_have_pane_title("zjhints settings"),
    );
}

/// Add the bar's entry to Zellij's settings screen. Bars started the same way
/// share one entry, which opens the settings screen from whichever of them
/// Zellij picks.
#[cfg(not(test))]
pub(crate) fn add_settings_entry() {
    register_settings_entry("zjhints");
}

#[cfg(test)]
pub(crate) fn add_settings_entry() {}

/// Whether a request from Zellij's settings screen, with `args`, came from
/// the client `client_id` draws for. Every client's instance of the chosen
/// bar receives it, and only the one the user is looking at should answer.
pub(crate) fn settings_requested_by(
    args: &BTreeMap<String, String>,
    client_id: Option<ClientId>,
) -> bool {
    args.get(OPEN_PLUGIN_SETTINGS_CLIENT_ID_ARG)
        .and_then(|id| id.parse::<ClientId>().ok())
        .is_some_and(|id| Some(id) == client_id)
}

/// The configuration the settings screen is opened with from a bar running
/// as `own_url`. See `open_settings`.
pub(crate) fn settings_configuration(own_url: Option<&str>) -> BTreeMap<String, String> {
    let mut configuration = BTreeMap::from([("settings".to_string(), "true".to_string())]);
    if let Some(alias) = own_url.filter(|url| !url.contains(':')) {
        configuration.insert("alias".to_string(), alias.to_string());
    }
    configuration
}
