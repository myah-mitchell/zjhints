//! zjtabs: a tab bar for Zellij, drawn from format strings in the same style
//! as zjhints, and sharing its themes, its settings screen and the bar
//! plumbing in `zjkit`.

use std::collections::BTreeMap;
use zellij_tile::prelude::*;
use zjkit::bar::{BarCore, BarPlugin, Role};

mod host;
mod settings;

use host::*;

#[derive(Default)]
struct State {
    /// What every call into Zellij goes through, so tests can see them. See
    /// `BarHost`.
    host: BarHost,
    /// Whether this instance runs as the bar or the settings screen. Only the
    /// bar uses the fields below.
    role: Role,
    /// What every bar keeps: its ids, its own URL and pane width, whether it
    /// is hidden, and its configuration.
    bar: BarCore,
}

register_plugin!(State);

impl State {
    /// Read every option from `user_configuration`, with the themes laid
    /// under it, as `load` does and as a live update from the settings screen
    /// does, so a value behaves the same whichever way it arrived. See
    /// `BarCore::configure`.
    fn apply_configuration(&mut self, user_configuration: BTreeMap<String, String>) {
        self.bar.configure(&BAR, user_configuration);
    }
}

impl BarPlugin for State {
    type Host = BarHost;

    fn host(&self) -> &BarHost {
        &self.host
    }

    fn bar(&self) -> &BarCore {
        &self.bar
    }

    fn mouse(&self) -> bool {
        // No `mouse` option yet: the bar always loads with the mouse on.
        true
    }

    fn apply_configuration(&mut self, user_configuration: BTreeMap<String, String>) {
        State::apply_configuration(self, user_configuration);
    }
}

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.role = Role::load(&BAR, &configuration);
        if !self.role.is_bar() {
            return;
        }
        self.apply_configuration(configuration);
        self.bar.load(&self.host, &BAR, true);
    }

    fn update(&mut self, event: Event) -> bool {
        if !self.role.is_bar() {
            return self.role.update(event);
        }
        let handled = self.bar.update(&self.host, &BAR, &event);
        BarCore::follow(self, &BAR, handled)
    }

    fn pipe(&mut self, message: PipeMessage) -> bool {
        if !self.role.is_bar() {
            self.role.pipe(&self.host, &BAR, &message);
            return false;
        }
        match self.bar.pipe(&self.host, &BAR, &message) {
            Some(handled) => BarCore::follow(self, &BAR, handled),
            None => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if !self.role.is_bar() {
            self.role.render(rows, cols);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zjkit::host::Call;

    #[test]
    fn loading_the_bar_asks_for_its_permissions_and_events() {
        let mut state = State::default();
        state.load(BTreeMap::new());
        let calls = state.host.calls();
        assert!(calls.contains(&Call::RequestPermission(bar_permissions(true))));
        assert!(calls.contains(&Call::Subscribe(BAR_EVENTS.to_vec())));
    }

    #[test]
    fn asking_for_mouse_keeps_the_permissions_the_bar_always_needs() {
        assert_eq!(bar_permissions(false), BAR_PERMISSIONS.to_vec());
        let permissions = bar_permissions(true);
        for permission in BAR_PERMISSIONS.iter().chain(MOUSE_PERMISSIONS.iter()) {
            assert!(permissions.contains(permission), "{permission:?}");
        }
        assert!(!bar_permissions(false).contains(&PermissionType::RunActionsAsUser));
    }

    #[test]
    fn the_settings_screen_asks_for_the_permissions_of_a_bar_with_mouse() {
        assert_eq!((settings::SCHEMA.permissions)(), bar_permissions(true));
    }
}
