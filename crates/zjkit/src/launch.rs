//! Opening panes and Zellij's own plugins the way its default keybindings
//! do.
//!
//! Zellij's session mode binds `w`, `c` and `p` to its session manager,
//! its settings screen and its plugin manager. Each binding runs
//! `LaunchOrFocusPlugin` on an alias from the `plugins` block of Zellij's
//! default config, floating and brought to the tab the user is on. Its pane
//! mode binds `a` to a new floating terminal. A bar that offers the same on
//! a click runs the same action, so a click behaves exactly as the key does.
//!
//! The actions are run with `Host::run_action`, which needs
//! `PermissionType::RunActionsAsUser`.

use std::collections::BTreeMap;
use zellij_tile::prelude::actions::Action;
use zellij_utils::input::layout::{PluginAlias, RunPluginOrAlias};

/// The alias Zellij's default config gives its session manager, as
/// `location="zellij:session-manager"`. Bound to `Ctrl o w`.
pub const SESSION_MANAGER_ALIAS: &str = "session-manager";

/// The alias Zellij's default config gives its settings screen, as
/// `location="zellij:configuration"`. Bound to `Ctrl o c`.
pub const CONFIGURATION_ALIAS: &str = "configuration";

/// The alias Zellij's default config gives its plugin manager, as
/// `location="zellij:plugin-manager"`. Bound to `Ctrl o p`.
pub const PLUGIN_MANAGER_ALIAS: &str = "plugin-manager";

/// What Zellij's default bindings run for the plugin aliased `alias`:
/// `LaunchOrFocusPlugin "<alias>" { floating true; move_to_focused_tab true; }`.
///
/// Zellij focuses the plugin if it is open, bringing it to the tab the user
/// is on from any other, and otherwise opens it there in a floating pane,
/// focused. It is named by its alias, as the bindings name it, so a user
/// whose config points the alias at another plugin gets that one here too.
pub fn launch_or_focus_floating(alias: &str) -> Action {
    Action::LaunchOrFocusPlugin {
        // An empty configuration, as the binding's block holds only options
        // of the action itself, which Zellij takes out of the configuration.
        plugin: RunPluginOrAlias::Alias(PluginAlias::new(alias, &Some(BTreeMap::new()), None)),
        should_float: true,
        move_to_focused_tab: true,
        should_open_in_place: false,
        close_replaced_pane: false,
        skip_cache: false,
        tab_id: None,
    }
}

/// What Zellij's default `Ctrl p a` binding runs: `NewFloatingPane`, a new
/// floating terminal running the user's default shell, focused.
///
/// Zellij starts the shell in the working directory of the pane focused
/// when the action runs: a terminal's current directory, or the directory a
/// plugin pane was started in. That is the directory the user is working
/// in. Zellij's plugin call for a new floating terminal,
/// `open_terminal_floating`, cannot give it as exactly: it takes the
/// directory from the plugin, which would have to ask for the focused
/// pane's first and gets none for a plugin pane, and Zellij maps a directory
/// under `/tmp` and a few other folders to the plugin's own.
pub fn new_floating_terminal() -> Action {
    Action::NewFloatingPane {
        command: None,
        pane_name: None,
        coordinates: None,
        near_current_pane: false,
        no_focus: false,
        tab_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_tile::prelude::{BareKey, InputMode, KeyWithModifier};
    use zellij_utils::input::config::Config;

    #[test]
    fn each_plugin_opens_as_zellij_s_own_session_mode_key_opens_it() {
        let config = Config::from_default_assets().expect("Zellij ships a default config");
        for (key, alias, location) in [
            ('w', SESSION_MANAGER_ALIAS, "zellij:session-manager"),
            ('c', CONFIGURATION_ALIAS, "zellij:configuration"),
            ('p', PLUGIN_MANAGER_ALIAS, "zellij:plugin-manager"),
        ] {
            let actions = config
                .keybinds
                .get_actions_for_key_in_mode(
                    &InputMode::Session,
                    &KeyWithModifier::new(BareKey::Char(key)),
                )
                .unwrap_or_else(|| panic!("Zellij binds {key} in session mode"));
            assert_eq!(
                actions.first(),
                Some(&launch_or_focus_floating(alias)),
                "{key}"
            );
            // The alias it names is Zellij's own plugin.
            assert_eq!(
                config.plugins.aliases[alias].location.display(),
                location,
                "{alias}"
            );
        }
    }

    #[test]
    fn a_new_terminal_opens_as_zellij_s_own_pane_mode_key_opens_one() {
        let config = Config::from_default_assets().expect("Zellij ships a default config");
        let actions = config
            .keybinds
            .get_actions_for_key_in_mode(
                &InputMode::Pane,
                &KeyWithModifier::new(BareKey::Char('a')),
            )
            .expect("Zellij binds a in pane mode");
        assert_eq!(actions.first(), Some(&new_floating_terminal()));
    }
}
