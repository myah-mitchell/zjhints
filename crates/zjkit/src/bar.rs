//! What every bar plugin does the same way: telling the bar apart from its
//! settings screen and hover-preview popup, asking for permissions, and
//! taking the events and pipe messages that mean the same to every bar.
//!
//! A plugin describes itself once, in a static `BarSpec`, and keeps a `Role`
//! and a `BarCore` in its state:
//!
//! - `load` asks `Role::load` what the instance runs as. The settings screen
//!   and the popup are then loaded already, and the plugin hands them every
//!   later `update`, `pipe` and `render`. A bar reads its options, then calls
//!   `BarCore::load`.
//! - The plugin's own reader of options calls `BarCore::configure` first, and
//!   reads every option from the configuration it returns.
//! - `update` and `pipe` give each event and message to the core before the
//!   plugin looks at it. The core answers with a `Handled`, which says whether
//!   to redraw and, when the options changed, hands back the configuration to
//!   read again: the core knows the options every bar shares, but reading the
//!   plugin's own is the plugin's business. `BarCore::follow` acts on the
//!   answer, through the plugin's `BarPlugin` implementation.
//! - `render` tells Zellij whether the row is worth keeping with
//!   `BarCore::sync_collapsed`.

use crate::host::{settings_requested_by, Host, SETTINGS_KEY};
use crate::panes::{own_pane_url, own_visible_pane_width};
use crate::popup::{self, PreviewPopup};
use crate::settings::live;
use crate::settings::schema::{Configuration, Schema};
use crate::settings::screen::SettingsScreen;
use crate::theme;
use crate::visibility::{self, Request, Visibility};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use zellij_tile::prelude::*;

/// What zjkit needs to know about a plugin to run its bar. Each plugin has
/// one, as a static.
pub struct BarSpec {
    /// The plugin's options, as the settings screen knows them. Its `plugin`
    /// also names the bar's entry on Zellij's settings screen and its log
    /// lines, its `live_message` is the preview the bar follows, and its
    /// `layer` lays the selected themes under the user's options.
    pub schema: &'static Schema,
    /// The bar's hide, show and toggle messages, and the question a new bar
    /// asks the running ones.
    pub visibility: &'static visibility::Names,
    /// The names a bar and its hover-preview popups know each other by, for
    /// a plugin whose bar opens them.
    pub popup: Option<&'static popup::Names>,
    /// The pipe message that opens the settings screen, or brings it up.
    pub open_settings_message: &'static str,
    /// The title of the settings screen's floating pane.
    pub settings_pane_title: &'static str,
    /// Every permission the bar asks for, with `mouse` on or off. A grant
    /// replaces the plugin's permissions rather than adding to them, both for
    /// the running instance and in Zellij's cache, so this is the whole set.
    ///
    /// A plugin with another option that needs a permission gives the set
    /// with that option on here, so `permissions(true)` is everything the bar
    /// may use, and says what its options as read need in
    /// `BarPlugin::permissions`.
    pub permissions: fn(bool) -> Vec<PermissionType>,
    /// The events the bar always subscribes to.
    pub events: &'static [EventType],
    /// The events `mouse` needs beyond `events`.
    pub mouse_events: &'static [EventType],
}

/// What an instance of a bar plugin runs as. The settings screen and the
/// hover-preview popup are started from the bar's own location, so one
/// plugin plays all three, told apart by the configuration it starts with.
#[derive(Default)]
pub enum Role {
    /// The popup of a hover preview. See `popup`.
    Popup(PreviewPopup),
    /// The settings screen. See `settings::screen`.
    Settings(Box<SettingsScreen>),
    /// The bar itself, whose events are the plugin's to handle.
    #[default]
    Bar,
}

impl Role {
    /// The role `configuration` starts an instance in, with a popup or a
    /// settings screen loaded already. A bar has its loading still to do; see
    /// `BarCore::load`.
    ///
    /// The popup and the screen ask for the bar's whole set of permissions:
    /// they run from its location, and so share its cached grant, which a
    /// request replaces.
    pub fn load(spec: &BarSpec, configuration: &Configuration) -> Self {
        let popup = spec
            .popup
            .and_then(|names| PreviewPopup::from_configuration(names, configuration));
        if let Some(popup) = popup {
            popup.load(&(spec.permissions)(true));
            return Self::Popup(popup);
        }
        if configuration.get(SETTINGS_KEY).map(String::as_str) == Some("true") {
            let mut screen = SettingsScreen::new(spec.schema);
            screen.load(configuration);
            return Self::Settings(Box::new(screen));
        }
        Self::Bar
    }

    pub fn is_bar(&self) -> bool {
        matches!(self, Self::Bar)
    }

    /// Hand `event` to the popup or the settings screen. Returns whether to
    /// redraw, which for the bar is always false: its events are the
    /// plugin's.
    pub fn update(&mut self, event: Event) -> bool {
        match self {
            Self::Popup(popup) => popup.update(event),
            Self::Settings(screen) => screen.update(event),
            Self::Bar => false,
        }
    }

    /// Hand `message` to the popup or the settings screen. Neither redraws
    /// for a message.
    pub fn pipe(&self, host: &impl Host, spec: &BarSpec, message: &PipeMessage) {
        match self {
            Self::Popup(popup) => popup.pipe(message),
            // Asked to open by a bar while the screen is open already, and
            // perhaps hidden.
            Self::Settings(_) if message.name == spec.open_settings_message => host.show_self(),
            Self::Settings(_) | Self::Bar => {}
        }
    }

    /// Draw the popup or the settings screen. The bar is the plugin's to
    /// draw.
    pub fn render(&mut self, rows: usize, cols: usize) {
        match self {
            Self::Popup(popup) => popup.render(rows, cols),
            Self::Settings(screen) => screen.render(rows, cols),
            Self::Bar => {}
        }
    }
}

/// What the plugin has left to do once `BarCore` has taken an event or a
/// message.
#[must_use]
#[derive(Debug, PartialEq)]
pub enum Handled {
    /// Nothing the bar shows has changed.
    Nothing,
    /// Something the bar shows has changed: redraw it.
    Redraw,
    /// The bar's options changed, or the themes laid under them did: read
    /// this user configuration again, as on load, then redraw.
    Reconfigure(Configuration),
}

impl Handled {
    fn redraw_if(changed: bool) -> Self {
        if changed {
            Self::Redraw
        } else {
            Self::Nothing
        }
    }
}

/// What `BarCore::follow` needs of a bar plugin's state beyond the
/// `BarCore` it keeps: the host it calls Zellij through, its `mouse`
/// option, the permissions its options need, and its reader of options.
///
/// Reading the options takes the whole state mutably, so a callback could
/// not do it while the core or the host were borrowed from the same state;
/// the helper reaches each part through this instead, one at a time.
pub trait BarPlugin {
    /// What the plugin makes its calls into Zellij through. See
    /// `crate::host`.
    type Host: Host;

    /// The host the plugin keeps, which the core's calls go through too.
    fn host(&self) -> &Self::Host;

    /// The core the plugin keeps.
    fn bar(&self) -> &BarCore;

    /// Whether the plugin's `mouse` option is on, as last read.
    fn mouse(&self) -> bool;

    /// Every permission the bar needs with its options as last read: the
    /// whole set, since a grant replaces what the plugin had. The plugin
    /// passes this to `BarCore::load`, and `BarCore::follow` asks for it
    /// again whenever new options need something the old ones did not.
    ///
    /// By default it is `BarSpec::permissions` for the `mouse` option. A
    /// plugin with another option that needs a permission gives its own.
    fn permissions(&self, spec: &BarSpec) -> Vec<PermissionType> {
        (spec.permissions)(self.mouse())
    }

    /// Read every option from `user_configuration`, as on load. The plugin's
    /// reader calls `BarCore::configure` first.
    fn apply_configuration(&mut self, user_configuration: Configuration);
}

/// The state every bar keeps, and its handling of what means the same to
/// every bar. See the module documentation for how a plugin uses it.
#[derive(Debug, Default)]
pub struct BarCore {
    /// This instance's plugin id, to find its own pane in `PaneUpdate`.
    pub plugin_id: Option<u32>,
    /// The client this instance draws for, to answer only the settings
    /// requests made from that client. See `host::settings_requested_by`.
    pub client_id: Option<ClientId>,
    /// The URL this bar's pane reports, which is the alias name when the bar
    /// was started from an alias. A bar loaded in the background, with
    /// `load_plugins`, has a hidden pane that reports it too. `None` until
    /// the first `PaneUpdate`.
    pub own_url: Option<String>,
    /// The location this bar runs from, as `Host::plugin_location` reports
    /// it, once a preview has needed it. For a bar started from an alias,
    /// this is where the alias leads, while `own_url` is the alias name.
    pub own_location: Option<String>,
    /// The URL each plugin pane of the session reports, by plugin id, from
    /// the last `PaneUpdate`. A preview is taken only from a plugin whose
    /// pane runs this bar's own plugin. See `BarCore::sent_by_own_plugin`.
    pub plugin_pane_urls: BTreeMap<u32, String>,
    /// Hidden by a hide or toggle message, or by the `hidden` option, and
    /// what Zellij was last told about the pane's row.
    pub visibility: Visibility,
    /// Keys whose previews this bar ignores, because its value for them came
    /// from somewhere other than the alias. See `live::apply`.
    pub preview_ignored: BTreeSet<String>,
    /// The options as the user wrote them, before a theme is laid under them.
    /// A preview from the settings screen edits this and layers it again, so
    /// switching theme replaces the old theme's values instead of keeping
    /// them as if the user had set them.
    pub user_configuration: Configuration,
    /// `user_configuration` with the selected themes laid under it. Every
    /// option is read from this, `$alias` colors included.
    pub configuration: Configuration,
    /// The Zellij theme from the last mode update, which `color_theme zellij`
    /// takes its aliases from. `None` until the first one arrives.
    pub zellij_palette: Option<Styling>,
    /// Content width of this bar's own pane, from `PaneUpdate`. `None` while
    /// the bar has no visible pane, as when it is loaded in the background.
    pub pane_width: Option<usize>,
    /// The permissions the bar last asked for, whole, which a grant gives it.
    ///
    /// Asking happens through a shared `BarCore` in `follow`, as the plugin's
    /// host is borrowed from the same state, so the set is kept in a
    /// `RefCell`.
    pub asked_permissions: RefCell<Vec<PermissionType>>,
    /// The permissions the bar holds, from Zellij's last answer: everything
    /// it last asked for after a grant, since a grant replaces what the
    /// plugin had, and nothing after a denial or before the first answer.
    ///
    /// Zellij shows one permission prompt at a time, a later request
    /// replacing an earlier one still unanswered, so an answer is taken as
    /// being for the last request.
    pub granted_permissions: Vec<PermissionType>,
}

impl BarCore {
    /// Take `user_configuration` as the bar's options: lay the selected
    /// themes under it, follow its `hidden` option, and keep both. Returns
    /// the configuration every option is to be read from.
    ///
    /// The plugin's reader of options calls this first, whether the options
    /// came with `load`, a preview or a new Zellij theme, so a value behaves
    /// the same whichever way it arrived.
    pub fn configure(
        &mut self,
        spec: &BarSpec,
        user_configuration: Configuration,
    ) -> &Configuration {
        let configuration = (spec.schema.layer)(&user_configuration, self.zellij_palette.as_ref());
        self.visibility
            .configure(&configuration, &self.configuration);
        self.user_configuration = user_configuration;
        self.configuration = configuration;
        &self.configuration
    }

    /// Start the bar, once the plugin has read its options: learn its ids,
    /// report theme names it does not know, ask for `permissions`, the
    /// plugin's `BarPlugin::permissions`, and subscribe to its events, with
    /// what `mouse` needs when it is on.
    pub fn load(
        &mut self,
        host: &impl Host,
        spec: &BarSpec,
        permissions: &[PermissionType],
        mouse: bool,
    ) {
        let ids = host.plugin_ids();
        self.plugin_id = Some(ids.plugin_id);
        self.client_id = Some(ids.client_id);
        theme::log_unknown_themes(spec.schema.plugin, &self.user_configuration);
        self.request_permission(host, permissions);

        // Deliberately still selectable at this point.
        //
        // Zellij draws the permission prompt in the plugin's own pane, and a
        // pane that is not selectable cannot be focused, so making it
        // unselectable before the prompt is answered leaves the user unable
        // to answer it at all. The pane is a one-line bar, so the prompt has
        // nowhere to go and no way to be reached.
        //
        // `set_selectable(false)` happens once the result arrives instead
        // (see `update`). That is safe for an already-granted plugin too:
        // `request_permission` replies immediately from the permission cache
        // rather than staying silent, so the event always comes.
        host.subscribe(spec.events);
        if mouse {
            host.subscribe(spec.mouse_events);
        }
    }

    /// Ask for `permissions`, the whole set the bar's options now need,
    /// after loading with a smaller one. The pane is made selectable while
    /// asking, so the permission prompt in it can be answered; the answer
    /// makes it unselectable again.
    pub fn ask_for_permissions(&self, host: &impl Host, permissions: &[PermissionType]) {
        host.set_selectable(true);
        self.request_permission(host, permissions);
    }

    /// Ask Zellij for `permissions`, keeping them as the set a grant gives.
    fn request_permission(&self, host: &impl Host, permissions: &[PermissionType]) {
        self.asked_permissions.replace(permissions.to_vec());
        host.request_permission(permissions);
    }

    /// Ask for what `mouse` needs after loading without it: `permissions`,
    /// as `ask_for_permissions` does, and the mouse's events.
    pub fn enable_mouse(&self, host: &impl Host, spec: &BarSpec, permissions: &[PermissionType]) {
        self.ask_for_permissions(host, permissions);
        host.subscribe(spec.mouse_events);
    }

    /// Act on what `update` or `pipe` left `plugin` to do with an event or a
    /// message. Returns whether the bar should redraw.
    ///
    /// A `Reconfigure` is read with the plugin's `apply_configuration`. When
    /// that turns `mouse` on, the bar asks for what the mouse needs with
    /// `enable_mouse`, since it did not when it loaded. When it turns on
    /// another option that needs a permission the old options did not, the
    /// bar asks for the larger set. Turning an option off asks for nothing:
    /// the grant it had covers less too.
    pub fn follow<P: BarPlugin>(plugin: &mut P, spec: &BarSpec, handled: Handled) -> bool {
        match handled {
            Handled::Nothing => false,
            Handled::Redraw => true,
            Handled::Reconfigure(configuration) => {
                let had_mouse = plugin.mouse();
                let had_permissions = plugin.permissions(spec);
                plugin.apply_configuration(configuration);
                let permissions = plugin.permissions(spec);
                if plugin.mouse() && !had_mouse {
                    plugin.bar().enable_mouse(plugin.host(), spec, &permissions);
                } else if permissions
                    .iter()
                    .any(|permission| !had_permissions.contains(permission))
                {
                    plugin
                        .bar()
                        .ask_for_permissions(plugin.host(), &permissions);
                }
                true
            }
        }
    }

    /// Tell Zellij whether this pane's row is worth keeping, when the answer
    /// has changed since the last time it was told. See
    /// `Visibility::next_collapsed`.
    ///
    /// A collapsed pane hands its row to the panes around it but keeps its
    /// place in the layout, so expanding gives back the exact row the layout
    /// asked for. Zellij ignores this for a plugin that has no tiled pane, so
    /// a bar running only as a pipe source for another plugin loses nothing.
    ///
    /// Turning `collapse_when_empty` off expands again on the next render
    /// rather than waiting for the bar to fill up on its own.
    pub fn sync_collapsed(
        &mut self,
        host: &impl Host,
        collapse_when_empty: bool,
        output_is_empty: bool,
    ) {
        let collapsed = self
            .visibility
            .next_collapsed(collapse_when_empty, output_is_empty);
        if let Some(collapsed) = collapsed {
            host.set_collapsed(collapsed);
        }
    }

    /// Open the settings screen, or reach the one already open, editing the
    /// alias this bar was started from if it was. See `Host::open_settings`.
    pub fn open_settings(&self, host: &impl Host, spec: &BarSpec) {
        host.open_settings(
            spec.open_settings_message,
            spec.settings_pane_title,
            self.own_url.as_deref(),
        );
    }

    /// Hide or show the bar, as a message with `alias` for its payload asks.
    /// See `Visibility::change`. Returns whether anything changed.
    pub fn change_visibility(&mut self, request: Request, alias: Option<&str>) -> bool {
        self.visibility
            .change(request, alias, self.own_url.as_deref())
    }

    /// Take what `event` means to every bar. The plugin handles the event
    /// itself afterwards, as a `ModeUpdate` or `PaneUpdate` carries more than
    /// the core reads.
    pub fn update(&mut self, host: &impl Host, spec: &BarSpec, event: &Event) -> Handled {
        match event {
            // `color_theme zellij` follows the Zellij theme, so a new one
            // means laying the themes again.
            Event::ModeUpdate(mode_info) => {
                let palette = mode_info.style.colors;
                if self.zellij_palette == Some(palette) {
                    return Handled::Nothing;
                }
                self.zellij_palette = Some(palette);
                Handled::Reconfigure(self.user_configuration.clone())
            }
            Event::PaneUpdate(manifest) => {
                let pane_width = own_visible_pane_width(manifest, self.plugin_id);
                let changed = pane_width != self.pane_width;
                self.pane_width = pane_width;
                if let Some(url) = own_pane_url(manifest, self.plugin_id) {
                    self.own_url = Some(url);
                }
                self.plugin_pane_urls = plugin_pane_urls(manifest);
                // The first pane update is the earliest this bar knows which
                // alias it runs as, and that it may message other plugins.
                if self.visibility.should_ask() {
                    host.ask_visibility(spec.visibility, self.own_url.as_deref());
                }
                Handled::redraw_if(changed)
            }
            // Answered, or granted from the cache. Either way the prompt is
            // gone and the pane has no further reason to take focus.
            Event::PermissionRequestResult(status) => {
                host.set_selectable(false);
                self.granted_permissions = match status {
                    PermissionStatus::Granted => self.asked_permissions.borrow().clone(),
                    PermissionStatus::Denied => Vec::new(),
                };
                if *status == PermissionStatus::Granted {
                    host.add_settings_entry(spec.schema.plugin);
                }
                Handled::Redraw
            }
            _ => Handled::Nothing,
        }
    }

    /// Take `message` if it means the same to every bar: a request from
    /// Zellij's settings screen, a question or answer about visibility, a
    /// hide, show or toggle, or a preview from the settings screen. `None`
    /// when it is none of these, and so the plugin's own to handle.
    pub fn pipe(
        &mut self,
        host: &impl Host,
        spec: &BarSpec,
        message: &PipeMessage,
    ) -> Option<Handled> {
        let payload = message.payload.as_deref();
        if message.name == OPEN_PLUGIN_SETTINGS_MESSAGE {
            if settings_requested_by(&message.args, self.client_id) {
                self.open_settings(host, spec);
            }
            return Some(Handled::Nothing);
        }
        if message.name == spec.visibility.ask {
            let asker = visibility::asker(
                &message.source,
                payload,
                self.plugin_id,
                self.own_url.as_deref(),
            );
            if let Some(asker) = asker {
                host.answer_visibility(spec.visibility, asker, self.visibility.answer());
            }
            return Some(Handled::Nothing);
        }
        if message.name == spec.visibility.answer {
            let changed = self.visibility.take_answer(&message.source, payload);
            return Some(Handled::redraw_if(changed));
        }
        if let Some(request) = Request::from_message(spec.visibility, &message.name) {
            if self.visibility.ends_a_cli_stream(&message.source, payload) {
                return Some(Handled::Nothing);
            }
            return Some(Handled::redraw_if(self.change_visibility(request, payload)));
        }
        if message.name == spec.schema.live_message {
            return Some(self.take_preview(host, spec, message));
        }
        None
    }

    /// A preview from the settings screen, when it is for this bar and
    /// changes an option this bar follows the alias in.
    ///
    /// A preview can add launcher entries and change what the bar runs, so
    /// it is taken only when `sent_by_own_plugin` says the settings screen
    /// could have sent it. A preview from another plugin, from `zellij pipe`
    /// or from a keybinding changes nothing.
    fn take_preview(&mut self, host: &impl Host, spec: &BarSpec, message: &PipeMessage) -> Handled {
        if !self.sent_by_own_plugin(host, &message.source) {
            return Handled::Nothing;
        }
        let Some((alias, changes)) = live::decode(&message.args) else {
            return Handled::Nothing;
        };
        if !live::is_for(&alias, self.own_url.as_deref()) {
            return Handled::Nothing;
        }
        let mut configuration = self.user_configuration.clone();
        if !live::apply(&mut configuration, &mut self.preview_ignored, &changes) {
            return Handled::Nothing;
        }
        theme::log_unknown_themes(spec.schema.plugin, &configuration);
        Handled::Reconfigure(configuration)
    }

    /// Whether `source` is a plugin whose pane runs this bar's own plugin,
    /// as the settings screen's does.
    ///
    /// A bar opens the screen from the location its alias stands for, so
    /// the screen's pane reports that location as its URL, where the bar's
    /// own pane reports the alias name. The bar learns the location from
    /// `Host::plugin_location` the first time a preview needs it.
    ///
    /// A pane reporting the bar's own URL runs the same plugin too, as a
    /// screen started from the alias would, so it counts as well. A plugin
    /// with no pane in the last `PaneUpdate` counts as another plugin.
    fn sent_by_own_plugin(&mut self, host: &impl Host, source: &PipeSource) -> bool {
        let PipeSource::Plugin(sender) = source else {
            return false;
        };
        let Some(sender_url) = self.plugin_pane_urls.get(sender) else {
            return false;
        };
        if self.own_url.as_ref() == Some(sender_url) {
            return true;
        }
        if self.own_location.is_none() {
            self.own_location = self
                .plugin_id
                .and_then(|plugin_id| host.plugin_location(plugin_id));
        }
        self.own_location.as_ref() == Some(sender_url)
    }
}

/// The URL each plugin pane in `manifest` reports, by plugin id.
fn plugin_pane_urls(manifest: &PaneManifest) -> BTreeMap<u32, String> {
    manifest
        .panes
        .values()
        .flatten()
        .filter(|pane| pane.is_plugin)
        .filter_map(|pane| Some((pane.id, pane.plugin_url.clone()?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{Call, Recorder};
    use crate::settings::schema::{option, Kind};
    use std::collections::{BTreeMap, HashMap};

    /// What `layer` lays under the options when a Zellij theme is known.
    const THEME_COLOR: (&str, &str) = ("color_accent", "#010203");

    static SCHEMA: Schema = Schema {
        plugin: "zjtest",
        groups: &["General"],
        options: &[option(
            "max_length",
            "General",
            Kind::Integer,
            Some("0"),
            "Widest the bar gets",
        )],
        families: &[],
        screen_options: &["settings", "alias"],
        internal_options: &[],
        live_message: "zjtest::configure",
        preview_group: None,
        permissions: || permissions(true),
        // Stands in for a theme taken from the running Zellij theme, so a
        // test can tell whether the themes were laid again.
        layer: |configuration, zellij_palette| {
            let mut layered = configuration.clone();
            if zellij_palette.is_some() {
                let (key, color) = THEME_COLOR;
                layered.entry(key.to_string()).or_insert(color.to_string());
            }
            layered
        },
        sample_values: |_, _| Vec::new(),
        draw_bar: |_, _, _| String::new(),
    };

    static VISIBILITY: visibility::Names = crate::visibility_names!("zjtest");

    static SPEC: BarSpec = BarSpec {
        schema: &SCHEMA,
        visibility: &VISIBILITY,
        popup: None,
        open_settings_message: "zjtest::open_settings",
        settings_pane_title: "zjtest settings",
        permissions,
        events: &[EventType::ModeUpdate, EventType::PaneUpdate],
        mouse_events: &[EventType::Mouse],
    };

    fn permissions(mouse: bool) -> Vec<PermissionType> {
        let mut permissions = vec![PermissionType::ReadApplicationState];
        if mouse {
            permissions.push(PermissionType::RunActionsAsUser);
        }
        permissions
    }

    fn configuration(pairs: &[(&str, &str)]) -> Configuration {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    /// Where the plugin of the alias `zjtest` is, which the settings screen
    /// is opened from.
    const LOCATION: &str = "file:/plugins/zjtest.wasm";

    /// A bar that knows it runs as the alias `zjtest`, with plugin id 3, and
    /// the settings screen, plugin 5, open beside it.
    fn bar_of_alias() -> BarCore {
        BarCore {
            plugin_id: Some(3),
            own_url: Some("zjtest".to_string()),
            plugin_pane_urls: BTreeMap::from([
                (3, "zjtest".to_string()),
                (5, LOCATION.to_string()),
            ]),
            ..Default::default()
        }
    }

    /// A host that reports plugin 3, the bar of `bar_of_alias`, as running
    /// from `LOCATION`.
    fn host_of_alias() -> Recorder {
        let host = Recorder::default();
        host.set_plugin_location(3, LOCATION);
        host
    }

    /// A plugin pane reporting `url`, as a `PaneUpdate` lists it.
    fn plugin_pane(id: u32, url: &str) -> PaneInfo {
        PaneInfo {
            id,
            is_plugin: true,
            plugin_url: Some(url.to_string()),
            ..Default::default()
        }
    }

    fn message(source: PipeSource, name: &str, payload: Option<&str>) -> PipeMessage {
        PipeMessage::new(source, name, &payload.map(str::to_string), &None, false)
    }

    fn from_plugin(id: u32, name: &str, payload: Option<&str>) -> PipeMessage {
        message(PipeSource::Plugin(id), name, payload)
    }

    /// A preview from the settings screen of the alias `alias`, plugin 5,
    /// changing each key from its `was` to its `now`.
    fn preview(alias: &str, changes: &[(&str, Option<&str>, Option<&str>)]) -> PipeMessage {
        preview_from(PipeSource::Plugin(5), alias, changes)
    }

    /// A preview as `preview` makes it, but sent from `source`.
    fn preview_from(
        source: PipeSource,
        alias: &str,
        changes: &[(&str, Option<&str>, Option<&str>)],
    ) -> PipeMessage {
        let changes: Vec<live::Change> = changes
            .iter()
            .map(|(key, was, now)| live::Change {
                key: key.to_string(),
                was: was.map(str::to_string),
                now: now.map(str::to_string),
            })
            .collect();
        PipeMessage::new(
            source,
            SCHEMA.live_message,
            &None,
            &Some(live::encode(alias, &changes)),
            false,
        )
    }

    fn manifest(panes: Vec<PaneInfo>) -> PaneManifest {
        PaneManifest {
            panes: HashMap::from([(0, panes)]),
        }
    }

    #[test]
    fn a_configuration_of_no_other_role_runs_the_bar() {
        assert!(Role::load(&SPEC, &configuration(&[("max_length", "9")])).is_bar());
        // `settings` set to anything but true is an ordinary option.
        assert!(Role::load(&SPEC, &configuration(&[("settings", "false")])).is_bar());
    }

    #[test]
    fn the_settings_screen_comes_up_when_asked_to_open_again() {
        let host = Recorder::default();
        let role = Role::Settings(Box::new(SettingsScreen::new(&SCHEMA)));
        role.pipe(&host, &SPEC, &from_plugin(1, "zjtest::hide", None));
        assert_eq!(host.calls(), Vec::new());
        role.pipe(&host, &SPEC, &from_plugin(1, "zjtest::open_settings", None));
        assert_eq!(host.calls(), vec![Call::ShowSelf]);
    }

    #[test]
    fn loading_asks_for_permissions_and_events_and_leaves_the_pane_selectable() {
        let host = Recorder::default();
        let mut bar = BarCore::default();
        bar.load(&host, &SPEC, &permissions(false), false);
        assert_eq!(bar.plugin_id, Some(Recorder::PLUGIN_ID));
        assert_eq!(bar.client_id, Some(Recorder::CLIENT_ID));
        // The pane stays selectable until the answer, so the prompt in it can
        // be answered.
        assert_eq!(
            host.calls(),
            vec![
                Call::PluginIds,
                Call::RequestPermission(permissions(false)),
                Call::Subscribe(SPEC.events.to_vec()),
            ]
        );
    }

    #[test]
    fn a_grant_gives_the_bar_what_it_last_asked_for_and_a_denial_nothing() {
        let host = Recorder::default();
        let mut bar = BarCore::default();
        bar.load(&host, &SPEC, &permissions(false), false);
        assert_eq!(bar.granted_permissions, Vec::new());
        let granted = Event::PermissionRequestResult(PermissionStatus::Granted);
        let _ = bar.update(&host, &SPEC, &granted);
        assert_eq!(bar.granted_permissions, permissions(false));

        // Asking for more holds only the old grant until the answer.
        bar.enable_mouse(&host, &SPEC, &permissions(true));
        assert_eq!(bar.granted_permissions, permissions(false));
        let _ = bar.update(&host, &SPEC, &granted);
        assert_eq!(bar.granted_permissions, permissions(true));

        let denied = Event::PermissionRequestResult(PermissionStatus::Denied);
        let _ = bar.update(&host, &SPEC, &denied);
        assert_eq!(bar.granted_permissions, Vec::new());
    }

    #[test]
    fn loading_with_mouse_asks_for_what_mouse_needs_too() {
        let host = Recorder::default();
        BarCore::default().load(&host, &SPEC, &permissions(true), true);
        assert_eq!(
            host.calls(),
            vec![
                Call::PluginIds,
                Call::RequestPermission(permissions(true)),
                Call::Subscribe(SPEC.events.to_vec()),
                Call::Subscribe(SPEC.mouse_events.to_vec()),
            ]
        );
    }

    #[test]
    fn turning_mouse_on_later_asks_for_it_from_a_selectable_pane() {
        let host = Recorder::default();
        BarCore::default().enable_mouse(&host, &SPEC, &permissions(true));
        assert_eq!(
            host.calls(),
            vec![
                Call::SetSelectable(true),
                Call::RequestPermission(permissions(true)),
                Call::Subscribe(SPEC.mouse_events.to_vec()),
            ]
        );
    }

    /// A plugin with two options of its own: `mouse`, and `confirm`, which
    /// with `mouse` on needs one more permission, as a plugin that asks
    /// before acting on a click would.
    #[derive(Default)]
    struct Plugin {
        host: Recorder,
        bar: BarCore,
        mouse: bool,
        confirm: bool,
        /// Every configuration read, in order.
        read: Vec<Configuration>,
    }

    impl BarPlugin for Plugin {
        type Host = Recorder;

        fn host(&self) -> &Recorder {
            &self.host
        }

        fn bar(&self) -> &BarCore {
            &self.bar
        }

        fn mouse(&self) -> bool {
            self.mouse
        }

        fn permissions(&self, spec: &BarSpec) -> Vec<PermissionType> {
            let mut permissions = (spec.permissions)(self.mouse);
            if self.mouse && self.confirm {
                permissions.push(CONFIRM_PERMISSION);
            }
            permissions
        }

        fn apply_configuration(&mut self, user_configuration: Configuration) {
            let configuration = self.bar.configure(&SPEC, user_configuration);
            let on = |key: &str| configuration.get(key).map(String::as_str) == Some("true");
            self.mouse = on("mouse");
            self.confirm = on("confirm");
            self.read.push(self.bar.user_configuration.clone());
        }
    }

    /// What the test plugin's `confirm` needs.
    const CONFIRM_PERMISSION: PermissionType = PermissionType::OpenTerminalsOrPlugins;

    #[test]
    fn following_says_whether_to_redraw_and_reads_a_new_configuration() {
        let mut plugin = Plugin::default();
        assert!(!BarCore::follow(&mut plugin, &SPEC, Handled::Nothing));
        assert!(BarCore::follow(&mut plugin, &SPEC, Handled::Redraw));
        assert_eq!(plugin.read, Vec::<Configuration>::new());
        let new = configuration(&[("max_length", "9")]);
        assert!(BarCore::follow(
            &mut plugin,
            &SPEC,
            Handled::Reconfigure(new.clone())
        ));
        assert_eq!(plugin.read, vec![new]);
        assert_eq!(plugin.host.calls(), Vec::new());
    }

    #[test]
    fn following_a_configuration_that_turns_mouse_on_asks_for_it() {
        let mut plugin = Plugin::default();
        let mouse = |value| Handled::Reconfigure(configuration(&[("mouse", value)]));
        assert!(BarCore::follow(&mut plugin, &SPEC, mouse("true")));
        assert_eq!(
            plugin.host.calls(),
            vec![
                Call::SetSelectable(true),
                Call::RequestPermission(permissions(true)),
                Call::Subscribe(SPEC.mouse_events.to_vec()),
            ]
        );
        // Already on, or turned off, it asks nothing.
        for value in ["true", "false"] {
            plugin.host.clear();
            assert!(BarCore::follow(&mut plugin, &SPEC, mouse(value)));
            assert_eq!(plugin.host.calls(), Vec::new(), "{value}");
        }
    }

    #[test]
    fn following_a_configuration_that_needs_another_permission_asks_for_the_larger_set() {
        let mut plugin = Plugin::default();
        let options = |pairs: &[(&str, &str)]| Handled::Reconfigure(configuration(pairs));
        assert!(BarCore::follow(
            &mut plugin,
            &SPEC,
            options(&[("mouse", "true")])
        ));
        plugin.host.clear();
        let confirm_on = options(&[("mouse", "true"), ("confirm", "true")]);
        assert!(BarCore::follow(&mut plugin, &SPEC, confirm_on));
        let mut larger = permissions(true);
        larger.push(CONFIRM_PERMISSION);
        // The mouse's events are subscribed already.
        assert_eq!(
            plugin.host.calls(),
            vec![Call::SetSelectable(true), Call::RequestPermission(larger)]
        );
        // Turning it off again keeps a grant that covers more, and asks
        // nothing.
        plugin.host.clear();
        assert!(BarCore::follow(
            &mut plugin,
            &SPEC,
            options(&[("mouse", "true")])
        ));
        assert_eq!(plugin.host.calls(), Vec::new());
    }

    #[test]
    fn turning_mouse_on_with_another_permission_s_option_asks_for_both_at_once() {
        let mut plugin = Plugin::default();
        let both = Handled::Reconfigure(configuration(&[("mouse", "true"), ("confirm", "true")]));
        assert!(BarCore::follow(&mut plugin, &SPEC, both));
        let mut larger = permissions(true);
        larger.push(CONFIRM_PERMISSION);
        assert_eq!(
            plugin.host.calls(),
            vec![
                Call::SetSelectable(true),
                Call::RequestPermission(larger),
                Call::Subscribe(SPEC.mouse_events.to_vec()),
            ]
        );
    }

    #[test]
    fn the_row_is_collapsed_or_expanded_only_when_that_changes() {
        let host = Recorder::default();
        let mut bar = BarCore::default();
        bar.sync_collapsed(&host, true, false);
        assert_eq!(host.calls(), Vec::new());
        bar.sync_collapsed(&host, true, true);
        bar.sync_collapsed(&host, true, true);
        assert_eq!(host.calls(), vec![Call::SetCollapsed(true)]);
        host.clear();
        bar.sync_collapsed(&host, false, true);
        assert_eq!(host.calls(), vec![Call::SetCollapsed(false)]);
    }

    #[test]
    fn a_permission_answer_makes_the_pane_unselectable_and_a_grant_adds_the_settings_entry() {
        let host = Recorder::default();
        let granted = Event::PermissionRequestResult(PermissionStatus::Granted);
        assert_eq!(
            BarCore::default().update(&host, &SPEC, &granted),
            Handled::Redraw
        );
        assert_eq!(
            host.calls(),
            vec![
                Call::SetSelectable(false),
                Call::AddSettingsEntry("zjtest".to_string()),
            ]
        );

        let host = Recorder::default();
        let denied = Event::PermissionRequestResult(PermissionStatus::Denied);
        assert_eq!(
            BarCore::default().update(&host, &SPEC, &denied),
            Handled::Redraw
        );
        assert_eq!(host.calls(), vec![Call::SetSelectable(false)]);
    }

    #[test]
    fn the_first_pane_update_learns_the_alias_and_asks_once_whether_the_bars_are_hidden() {
        let host = Recorder::default();
        let mut bar = BarCore {
            plugin_id: Some(7),
            ..Default::default()
        };
        let own_pane = PaneInfo {
            id: 7,
            is_plugin: true,
            plugin_url: Some("zjtest-alt".to_string()),
            pane_content_columns: 80,
            ..Default::default()
        };
        let update = Event::PaneUpdate(manifest(vec![own_pane]));
        assert_eq!(bar.update(&host, &SPEC, &update), Handled::Redraw);
        assert_eq!(bar.own_url.as_deref(), Some("zjtest-alt"));
        assert_eq!(bar.pane_width, Some(80));
        // The same report again changes nothing and asks nothing.
        assert_eq!(bar.update(&host, &SPEC, &update), Handled::Nothing);
        assert_eq!(
            host.calls(),
            vec![Call::AskVisibility {
                message: "zjtest::ask_visibility",
                alias: Some("zjtest-alt".to_string()),
            }]
        );
    }

    #[test]
    fn a_new_zellij_theme_hands_back_the_options_to_read_again() {
        let host = Recorder::default();
        let mut bar = BarCore::default();
        let user_configuration = configuration(&[("max_length", "9")]);
        assert_eq!(
            bar.configure(&SPEC, user_configuration.clone()),
            &user_configuration
        );

        let mut mode_info = ModeInfo::default();
        mode_info.style.colors.ribbon_selected.background = PaletteColor::Rgb((1, 2, 3));
        let update = Event::ModeUpdate(mode_info);
        let Handled::Reconfigure(again) = bar.update(&host, &SPEC, &update) else {
            panic!("a new theme should hand back the options");
        };
        assert_eq!(again, user_configuration);
        // Read again, they have the theme laid under them, and the user's own
        // options are left as they were written.
        let (key, color) = THEME_COLOR;
        assert_eq!(
            bar.configure(&SPEC, again).get(key).map(String::as_str),
            Some(color)
        );
        assert_eq!(bar.user_configuration, user_configuration);
        // The same theme again changes nothing.
        assert_eq!(bar.update(&host, &SPEC, &update), Handled::Nothing);
    }

    #[test]
    fn the_hidden_option_hides_the_bar_only_when_it_changes() {
        let mut bar = BarCore::default();
        let _ = bar.configure(&SPEC, configuration(&[("hidden", "true")]));
        assert!(bar.visibility.is_hidden());
        // A message overrides it, and options read again without it changing
        // keep that.
        bar.change_visibility(Request::Show, None);
        let _ = bar.configure(
            &SPEC,
            configuration(&[("hidden", "true"), ("max_length", "9")]),
        );
        assert!(!bar.visibility.is_hidden());
    }

    #[test]
    fn zellij_s_settings_entry_opens_settings_from_the_bar_of_the_client_that_chose_it() {
        let request = |client: &str| {
            let args = BTreeMap::from([(
                OPEN_PLUGIN_SETTINGS_CLIENT_ID_ARG.to_string(),
                client.to_string(),
            )]);
            PipeMessage::new(
                PipeSource::Plugin(1),
                OPEN_PLUGIN_SETTINGS_MESSAGE,
                &None,
                &Some(args),
                false,
            )
        };
        let host = Recorder::default();
        let mut bar = BarCore {
            client_id: Some(2),
            ..bar_of_alias()
        };
        assert_eq!(
            bar.pipe(&host, &SPEC, &request("3")),
            Some(Handled::Nothing)
        );
        assert_eq!(host.calls(), Vec::new());
        assert_eq!(
            bar.pipe(&host, &SPEC, &request("2")),
            Some(Handled::Nothing)
        );
        assert_eq!(
            host.calls(),
            vec![Call::OpenSettings {
                message: "zjtest::open_settings".to_string(),
                pane_title: "zjtest settings".to_string(),
                own_url: Some("zjtest".to_string()),
            }]
        );
    }

    #[test]
    fn a_bar_answers_another_bar_of_its_alias_asking_whether_it_is_hidden() {
        let host = Recorder::default();
        let mut bar = bar_of_alias();
        bar.change_visibility(Request::Hide, None);
        let ask = |source: PipeSource, alias: Option<&str>| {
            message(source, "zjtest::ask_visibility", alias)
        };
        let mut answered = |source, alias| {
            host.clear();
            assert_eq!(
                bar.pipe(&host, &SPEC, &ask(source, alias)),
                Some(Handled::Nothing)
            );
            host.calls()
        };
        let answer = vec![Call::AnswerVisibility {
            message: "zjtest::visibility",
            plugin_id: 5,
            answer: visibility::HIDDEN.to_string(),
        }];
        assert_eq!(answered(PipeSource::Plugin(5), Some("zjtest")), answer);
        // An asker that does not know its alias yet cannot name it.
        assert_eq!(answered(PipeSource::Plugin(5), None), answer);
        assert_eq!(
            answered(PipeSource::Plugin(5), Some("zjtest-nested")),
            Vec::new()
        );
        // The question is broadcast, so the asker hears it too.
        assert_eq!(answered(PipeSource::Plugin(3), None), Vec::new());
        assert_eq!(answered(PipeSource::Keybind, None), Vec::new());
    }

    #[test]
    fn a_new_bar_takes_the_first_answer_only() {
        let host = Recorder::default();
        let mut bar = bar_of_alias();
        let answer = |source, payload| message(source, "zjtest::visibility", Some(payload));
        assert_eq!(
            bar.pipe(&host, &SPEC, &answer(PipeSource::Keybind, "hidden")),
            Some(Handled::Nothing)
        );
        assert_eq!(
            bar.pipe(&host, &SPEC, &answer(PipeSource::Plugin(5), "hidden")),
            Some(Handled::Redraw)
        );
        assert!(bar.visibility.is_hidden());
        assert_eq!(
            bar.pipe(&host, &SPEC, &answer(PipeSource::Plugin(6), "shown")),
            Some(Handled::Nothing)
        );
        assert!(bar.visibility.is_hidden());
    }

    #[test]
    fn a_hide_or_show_before_the_answer_wins_over_it() {
        let host = Recorder::default();
        let mut bar = bar_of_alias();
        let show = from_plugin(5, "zjtest::show", None);
        assert_eq!(bar.pipe(&host, &SPEC, &show), Some(Handled::Nothing));
        let answer = from_plugin(5, "zjtest::visibility", Some("hidden"));
        assert_eq!(bar.pipe(&host, &SPEC, &answer), Some(Handled::Nothing));
        assert!(!bar.visibility.is_hidden());
    }

    #[test]
    fn toggling_hides_then_shows_and_repeats_change_nothing() {
        let host = Recorder::default();
        let mut bar = bar_of_alias();
        let mut send = |name, payload| {
            bar.pipe(&host, &SPEC, &from_plugin(5, name, payload))
                .expect("a visibility message is the core's")
        };
        assert_eq!(send("zjtest::toggle", None), Handled::Redraw);
        assert_eq!(send("zjtest::hide", None), Handled::Nothing);
        assert_eq!(send("zjtest::toggle", Some("")), Handled::Redraw);
        assert_eq!(send("zjtest::show", None), Handled::Nothing);
        assert_eq!(host.calls(), Vec::new());
    }

    #[test]
    fn a_message_naming_an_alias_reaches_only_that_alias_s_bars() {
        let host = Recorder::default();
        let mut bar = bar_of_alias();
        let hide = |alias| from_plugin(5, "zjtest::hide", Some(alias));
        assert_eq!(
            bar.pipe(&host, &SPEC, &hide("zjtest-nested")),
            Some(Handled::Nothing)
        );
        assert!(!bar.visibility.is_hidden());
        assert_eq!(
            bar.pipe(&host, &SPEC, &hide("zjtest")),
            Some(Handled::Redraw)
        );
        // A bar that does not know its alias yet takes them all.
        let mut new_bar = BarCore::default();
        assert_eq!(
            new_bar.pipe(&host, &SPEC, &hide("zjtest-nested")),
            Some(Handled::Redraw)
        );
    }

    #[test]
    fn the_end_of_a_scripted_zellij_pipe_does_not_reach_every_bar() {
        let host = Recorder::default();
        let mut bar = bar_of_alias();
        let cli = |pipe: &str, payload| {
            message(PipeSource::Cli(pipe.to_string()), "zjtest::hide", payload)
        };
        assert_eq!(
            bar.pipe(&host, &SPEC, &cli("pipe-1", Some("zjtest-nested"))),
            Some(Handled::Nothing)
        );
        // The same command ending its stream with no payload.
        assert_eq!(
            bar.pipe(&host, &SPEC, &cli("pipe-1", None)),
            Some(Handled::Nothing)
        );
        assert!(!bar.visibility.is_hidden());
        // A new command with no payload is meant for every bar.
        assert_eq!(
            bar.pipe(&host, &SPEC, &cli("pipe-2", None)),
            Some(Handled::Redraw)
        );
        assert!(bar.visibility.is_hidden());
    }

    #[test]
    fn a_preview_for_this_bar_hands_back_the_changed_options() {
        let host = host_of_alias();
        let mut bar = bar_of_alias();
        let _ = bar.configure(&SPEC, configuration(&[("max_length", "10")]));
        let changes = [("max_length", Some("10"), Some("30"))];
        assert_eq!(
            bar.pipe(&host, &SPEC, &preview("zjtest", &changes)),
            Some(Handled::Reconfigure(configuration(&[("max_length", "30")])))
        );
        // Handing them back is all: the plugin reads them again.
        assert_eq!(
            bar.user_configuration,
            configuration(&[("max_length", "10")])
        );
        assert_eq!(host.calls(), Vec::new());
        // The location the alias stands for is kept for the next preview.
        assert_eq!(bar.own_location.as_deref(), Some(LOCATION));
    }

    #[test]
    fn a_preview_is_taken_from_the_screen_a_pane_update_lists() {
        let host = host_of_alias();
        let mut bar = BarCore {
            plugin_id: Some(3),
            ..Default::default()
        };
        let panes = vec![plugin_pane(3, "zjtest"), plugin_pane(5, LOCATION)];
        let _ = bar.update(&host, &SPEC, &Event::PaneUpdate(manifest(panes)));
        let changes = [("max_length", None, Some("30"))];
        assert_eq!(
            bar.pipe(&host, &SPEC, &preview("zjtest", &changes)),
            Some(Handled::Reconfigure(configuration(&[("max_length", "30")])))
        );

        // Once the screen's pane is closed, a plugin given its id later is
        // not taken for it.
        let panes = vec![plugin_pane(3, "zjtest")];
        let _ = bar.update(&host, &SPEC, &Event::PaneUpdate(manifest(panes)));
        assert_eq!(
            bar.pipe(&host, &SPEC, &preview("zjtest", &changes)),
            Some(Handled::Nothing)
        );
    }

    #[test]
    fn a_preview_from_a_pane_of_the_bar_s_own_alias_is_taken_as_it_is() {
        // A screen started from the alias reports the alias name, as the bar
        // does, so the bar need not ask where the alias leads.
        let host = Recorder::default();
        let mut bar = bar_of_alias();
        bar.plugin_pane_urls.insert(5, "zjtest".to_string());
        let changes = [("max_length", None, Some("30"))];
        assert_eq!(
            bar.pipe(&host, &SPEC, &preview("zjtest", &changes)),
            Some(Handled::Reconfigure(configuration(&[("max_length", "30")])))
        );
        assert_eq!(bar.own_location, None);
    }

    #[test]
    fn a_preview_from_another_plugin_is_refused() {
        let host = host_of_alias();
        let mut bar = bar_of_alias();
        bar.plugin_pane_urls
            .insert(6, "file:/plugins/other.wasm".to_string());
        let changes = [("max_length", None, Some("30"))];
        for sender in [6, 9] {
            // Plugin 9 has no pane at all.
            let message = preview_from(PipeSource::Plugin(sender), "zjtest", &changes);
            assert_eq!(
                bar.pipe(&host, &SPEC, &message),
                Some(Handled::Nothing),
                "plugin {sender}"
            );
        }
    }

    #[test]
    fn a_preview_from_zellij_pipe_or_a_keybinding_is_refused() {
        let host = host_of_alias();
        let mut bar = bar_of_alias();
        let changes = [("max_length", None, Some("30"))];
        for source in [PipeSource::Cli("pipe-1".to_string()), PipeSource::Keybind] {
            let message = preview_from(source.clone(), "zjtest", &changes);
            assert_eq!(
                bar.pipe(&host, &SPEC, &message),
                Some(Handled::Nothing),
                "{source:?}"
            );
        }
    }

    #[test]
    fn a_bar_whose_location_is_unknown_refuses_a_preview_from_another_location() {
        // Zellij could not say where the alias bar runs from, so a pane
        // reporting a location is not known to be the screen.
        let host = Recorder::default();
        let mut bar = bar_of_alias();
        let changes = [("max_length", None, Some("30"))];
        assert_eq!(
            bar.pipe(&host, &SPEC, &preview("zjtest", &changes)),
            Some(Handled::Nothing)
        );
    }

    #[test]
    fn a_preview_for_another_alias_is_ignored() {
        let host = host_of_alias();
        let mut bar = bar_of_alias();
        let changes = [("max_length", None, Some("30"))];
        assert_eq!(
            bar.pipe(&host, &SPEC, &preview("zjtest-alt", &changes)),
            Some(Handled::Nothing)
        );
    }

    #[test]
    fn a_preview_leaves_a_key_the_bar_got_elsewhere_alone() {
        let host = host_of_alias();
        // The layout gave this bar max_length 5; the alias does not set it.
        let mut bar = bar_of_alias();
        let _ = bar.configure(&SPEC, configuration(&[("max_length", "5")]));
        let changes = [("max_length", None, Some("30"))];
        assert_eq!(
            bar.pipe(&host, &SPEC, &preview("zjtest", &changes)),
            Some(Handled::Nothing)
        );
        assert!(bar.preview_ignored.contains("max_length"));
    }

    #[test]
    fn other_messages_are_the_plugin_s_own() {
        let host = Recorder::default();
        let mut bar = bar_of_alias();
        for name in ["zjtest::hover_opened", "zjhints::hide", "pipe"] {
            assert_eq!(
                bar.pipe(&host, &SPEC, &from_plugin(5, name, None)),
                None,
                "{name}"
            );
        }
        assert_eq!(host.calls(), Vec::new());
    }
}
