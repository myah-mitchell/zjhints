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
pub(crate) const SETTINGS_PANE_TITLE: &str = "zjtaskbar settings";

/// What the bar always asks for.
pub(crate) const BAR_PERMISSIONS: [PermissionType; 3] = [
    // The panes, the tabs and the input mode, from `PaneUpdate`, `TabUpdate`
    // and `ModeUpdate`.
    PermissionType::ReadApplicationState,
    // Adding the bar's entry to Zellij's settings screen, and the buttons on
    // the listed panes' frames. With `mouse` on or from those buttons, also
    // focusing, minimizing, moving and closing the listed panes, and opening
    // Zellij's bar menu. Handing the bar's row back while it is empty
    // needs no permission, as it changes only the bar's own pane.
    PermissionType::ChangeApplicationState,
    // The hide, show and toggle messages between bars, and opening the
    // settings screen, which is this plugin again in a floating pane.
    PermissionType::MessageAndLaunchOtherPlugins,
];

/// A part of the bar that needs a permission beyond `BAR_PERMISSIONS`. The
/// bar asks for a feature's permissions only while its options turn the
/// feature on, and a feature no option turns on is never asked for; see
/// `State::features`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Feature {
    /// Clicking the bar's entries and the bar itself.
    Mouse,
    /// The launcher's menu and the plugins it opens.
    Launcher,
    /// A launcher item that runs one of Zellij's own actions: a new
    /// terminal, or opening the session manager, the settings or the plugin
    /// manager.
    LauncherZellijAction,
    /// A launcher entry that runs a command in a new floating pane.
    LauncherCommand,
    /// The tray's clock, while it has no `clock_utc_offset` to go by and asks
    /// the system for the local time's offset instead.
    ClockCommand,
    /// The close button on a pane's frame, while it asks before closing.
    FrameCloseConfirm,
}

impl Feature {
    /// Every feature, for the settings screen, which asks for what a bar with
    /// every feature on needs.
    pub(crate) const ALL: [Feature; 6] = [
        Feature::Mouse,
        Feature::Launcher,
        Feature::LauncherZellijAction,
        Feature::LauncherCommand,
        Feature::ClockCommand,
        Feature::FrameCloseConfirm,
    ];

    /// What the feature needs beyond what the bar always asks for.
    pub(crate) fn permissions(self) -> &'static [PermissionType] {
        match self {
            Feature::Mouse => &[
                // Renaming a pane, from a double-click or the pane menu,
                // which runs Zellij's own action for it.
                PermissionType::RunActionsAsUser,
                // The pane menu and the close confirmation, which are prompts.
                PermissionType::OpenTerminalsOrPlugins,
            ],
            Feature::Launcher => &[
                // The launcher's menu, which is a prompt, and the plugin
                // entries, which open plugins in floating panes.
                PermissionType::OpenTerminalsOrPlugins,
            ],
            Feature::LauncherZellijAction => &[
                // Zellij's `NewFloatingPane` and `LaunchOrFocusPlugin`, run
                // as its own keys run them; see `zjkit::launch`.
                PermissionType::RunActionsAsUser,
            ],
            Feature::LauncherCommand => &[
                // Opening a pane that runs a command.
                PermissionType::RunCommands,
            ],
            Feature::ClockCommand => &[
                // Running `date` for the local time's offset from UTC, which
                // a plugin cannot read from its sandbox.
                PermissionType::RunCommands,
            ],
            Feature::FrameCloseConfirm => &[
                // The close confirmation, which is a prompt.
                PermissionType::OpenTerminalsOrPlugins,
            ],
        }
    }
}

/// Every permission the bar asks for with `features` on, each once. A grant
/// replaces the plugin's permissions rather than adding to them, both for
/// the running instance and in Zellij's cache, so every request has to carry
/// the whole set.
pub(crate) fn bar_permissions(features: &[Feature]) -> Vec<PermissionType> {
    let mut permissions = BAR_PERMISSIONS.to_vec();
    for permission in features.iter().flat_map(|feature| feature.permissions()) {
        if !permissions.contains(permission) {
            permissions.push(*permission);
        }
    }
    permissions
}

/// The events the bar always subscribes to.
pub(crate) const BAR_EVENTS: [EventType; 8] = [
    // The nested-session state that hides or dims the bar, and the Zellij
    // theme the default look is drawn in.
    EventType::ModeUpdate,
    // The panes the bar lists, and its own pane, for its width and the alias
    // it was started as.
    EventType::PaneUpdate,
    EventType::PermissionRequestResult,
    // Which tab is active, whether its floating panes are shown, and the
    // names of the tabs the panes are in.
    EventType::TabUpdate,
    // The tray's clock: its ticks, and the answer of the command that asks
    // the system for its offset from UTC. Neither needs a permission, and
    // neither arrives unless the clock asks for it.
    EventType::Timer,
    EventType::RunCommandResult,
    // Clicks on the buttons on the listed panes' frames, which go only to
    // the plugin that placed them, and arrive only while it has.
    EventType::FrameItemClicked,
    // The answers to the pane menu, the close confirmation and the
    // launcher's menu. The close button asks with the mouse off too.
    EventType::PromptResult,
];

/// The events the bar subscribes to while `mouse` is on.
pub(crate) const MOUSE_EVENTS: [EventType; 1] = [
    // Clicks and the wheel over the bar.
    EventType::Mouse,
];

/// The pipe message that opens the settings screen, or brings it up.
pub(crate) const OPEN_SETTINGS_MESSAGE: &str = "zjtaskbar::open_settings";

/// The hide, show and toggle messages.
pub(crate) static VISIBILITY: visibility::Names = zjkit::visibility_names!("zjtaskbar");

/// What zjkit needs to know to run the bar. See `zjkit::bar`.
pub(crate) static BAR: BarSpec = BarSpec {
    schema: &crate::settings::SCHEMA,
    visibility: &VISIBILITY,
    // The bar opens no hover previews.
    popup: None,
    open_settings_message: OPEN_SETTINGS_MESSAGE,
    settings_pane_title: SETTINGS_PANE_TITLE,
    // With every feature on, so the set is everything the bar may use. The
    // bar asks for what its options as read need; see `State::permissions`.
    permissions: |_mouse| bar_permissions(&Feature::ALL),
    events: &BAR_EVENTS,
    mouse_events: &MOUSE_EVENTS,
};
