//! What the bar asks of Zellij: the host it makes its calls through, and the
//! permissions, events and pipe messages those calls use, gathered into the
//! `BarSpec` zjkit runs the bar by.

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
pub(crate) const SETTINGS_PANE_TITLE: &str = "zjtabs settings";

/// What the bar always asks for.
pub(crate) const BAR_PERMISSIONS: [PermissionType; 3] = [
    // The tabs, the input mode and the bar's own pane, from `TabUpdate`,
    // `ModeUpdate` and `PaneUpdate`.
    PermissionType::ReadApplicationState,
    // Adding the bar's entry to Zellij's settings screen, and with `mouse`,
    // switching tabs on a click or the wheel, closing a tab on a middle click
    // and opening Zellij's context menu.
    PermissionType::ChangeApplicationState,
    // Opening the settings screen, a floating pane of the bar's own
    // instance, and the live previews it sends the bars.
    PermissionType::MessageAndLaunchOtherPlugins,
];

/// What `mouse` needs beyond what the bar always asks for.
pub(crate) const MOUSE_PERMISSIONS: [PermissionType; 1] = [
    // Renaming a tab on a double-click, and opening Zellij's session manager
    // from a click on the session name, each of which runs Zellij's own
    // action for it.
    PermissionType::RunActionsAsUser,
];

/// What `mouse_close_confirm` needs beyond what `mouse` needs. Asked for
/// only while both are on, since without the mouse there is no middle click
/// to confirm.
pub(crate) const CLOSE_CONFIRM_PERMISSIONS: [PermissionType; 1] = [
    // Showing the close confirmation, which is a prompt.
    PermissionType::OpenTerminalsOrPlugins,
];

/// Every permission the bar asks for, with `mouse` and `mouse_close_confirm`
/// on or off. A grant replaces the plugin's permissions rather than adding
/// to them, both for the running instance and in Zellij's cache, so every
/// request has to carry the whole set. The settings screen asks for the set
/// with both on, since it shares the bar's cached grant and is how they get
/// turned on.
pub(crate) fn bar_permissions(mouse: bool, mouse_close_confirm: bool) -> Vec<PermissionType> {
    let mut permissions = BAR_PERMISSIONS.to_vec();
    if mouse {
        permissions.extend(MOUSE_PERMISSIONS);
        if mouse_close_confirm {
            permissions.extend(CLOSE_CONFIRM_PERMISSIONS);
        }
    }
    permissions
}

/// The events the bar always subscribes to.
pub(crate) const BAR_EVENTS: [EventType; 4] = [
    // The input mode, for rename-tab mode and the nested-session state, and
    // the Zellij theme the default look is drawn in.
    EventType::ModeUpdate,
    // The bar's own pane, for its width and the alias it was started as.
    EventType::PaneUpdate,
    EventType::PermissionRequestResult,
    EventType::TabUpdate,
];

/// The events `mouse` needs beyond what the bar always subscribes to.
pub(crate) const MOUSE_EVENTS: [EventType; 2] = [
    EventType::Mouse,
    // The answer to the close confirmation of `mouse_close_confirm`.
    // Subscribed with the mouse even while that option is off, so turning it
    // on later needs no new subscription.
    EventType::PromptResult,
];

/// The pipe message that opens the settings screen, or brings it up.
pub(crate) const OPEN_SETTINGS_MESSAGE: &str = "zjtabs::open_settings";

/// The hide, show and toggle messages.
pub(crate) static VISIBILITY: visibility::Names = zjkit::visibility_names!("zjtabs");

/// What zjkit needs to know to run the bar. See `zjkit::bar`.
pub(crate) static BAR: BarSpec = BarSpec {
    schema: &crate::settings::SCHEMA,
    visibility: &VISIBILITY,
    // The bar opens no hover previews.
    popup: None,
    open_settings_message: OPEN_SETTINGS_MESSAGE,
    settings_pane_title: SETTINGS_PANE_TITLE,
    // With `mouse_close_confirm` on, so the set is everything the bar may
    // use. The bar asks for what its options as read need; see
    // `State::permissions`.
    permissions: |mouse| bar_permissions(mouse, true),
    events: &BAR_EVENTS,
    mouse_events: &MOUSE_EVENTS,
};
