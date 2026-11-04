//! The calls a bar makes into Zellij, behind a trait so tests can see them.
//!
//! A plugin holds one `Host` and makes every call through it. In the plugin
//! that is `Zellij`, which calls the `zellij_tile` functions. In tests it is
//! `Recorder`, which calls nothing and keeps a `Call` for each call instead,
//! so a test can drive the plugin and then assert what it asked Zellij to do.
//!
//! The choice between them is the plugin's to make, with one `cfg(test)`
//! switch of its own: `cfg(test)` is set per crate, so zjkit cannot tell when
//! a plugin's tests are being built. `Recorder` is compiled in every build
//! for the same reason.
//!
//! Where several plugins make the same call in the same way, the trait has
//! one method for the whole of it, such as `open_settings` or
//! `open_preview_popup`, so each plugin passes only its own names. Everything
//! else is one method per Zellij call.
//!
//! A plugin runs as a shared instance, one `P` per slot (see `shared`). The
//! calls about the plugin's own pane, its timers and its permissions are
//! made for the acting slot: `Zellij` hands them to `shared`, which makes
//! them for that slot, and `Recorder` hands them over too, so tests of
//! `shared` see them. With no slot acting, `Zellij` makes them itself.
//!
//! The popup of a hover preview and the settings screen are whole plugins of
//! their own (see `popup` and `settings::screen`). They make their calls
//! about their own pane through `Zellij`, and the rest directly. Their tests
//! drive the parts that make no calls.

use crate::popup;
use crate::shared::{self, Command, PopupRequest};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::SystemTime;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

/// The configuration key that marks an instance as the settings screen.
pub(crate) const SETTINGS_KEY: &str = "settings";
/// The configuration key that pins the settings screen to one alias.
const ALIAS_KEY: &str = "alias";
/// The configuration key Zellij tells shared instances of one location apart
/// by: panes that load the location with the same value, or none, are slots
/// of one instance.
pub const INSTANCE_KEY: &str = "instance";

/// Everything a bar asks of Zellij.
pub trait Host {
    /// The plugin's ids: the acting slot's id as its plugin id, which is its
    /// pane's, and the client the slot acts for when Zellij named one. With
    /// no slot acting, the instance's own.
    fn plugin_ids(&self) -> PluginIds;

    /// The id of the shared instance every slot runs in. A pipe message
    /// from any of its slots names this id as its source.
    fn instance_id(&self) -> u32;

    /// Ask for `permissions`. A grant replaces what the plugin had rather than
    /// adding to it, so this has to be the whole set the plugin needs.
    /// Zellij answers with `Event::PermissionRequestResult`.
    fn request_permission(&self, permissions: &[PermissionType]);

    /// Have Zellij send `events` to `update`.
    fn subscribe(&self, events: &[EventType]);

    /// Whether the plugin's pane can take the focus. A permission prompt is
    /// drawn in the pane and can only be answered while it can.
    fn set_selectable(&self, selectable: bool);

    /// Whether the pane's row goes to the panes around it. See
    /// `visibility::Visibility::next_collapsed`.
    fn set_collapsed(&self, collapsed: bool);

    /// Bring the plugin's pane up, floating it if it was hidden.
    fn show_self(&self);

    /// Close the plugin's pane.
    fn close_self(&self);

    /// Have Zellij send `Event::Timer` after `seconds`.
    fn set_timeout(&self, seconds: f64);

    /// The current time of day, as the system clock has it.
    ///
    /// Not a `Call`: asking the time changes nothing, and a bar asks it on
    /// every draw that shows a clock. A test sets the time a `Recorder`
    /// reports with `Recorder::set_system_time`.
    fn system_time(&self) -> SystemTime;

    /// Run `command`, its program then its arguments, on the machine Zellij
    /// runs on, in the plugin's working directory. Needs the `RunCommands`
    /// permission. Zellij sends `Event::RunCommandResult` once it ends, with
    /// its exit code, output and `context`, so the plugin can tell which of
    /// its commands answered.
    fn run_command(&self, command: &[&str], context: BTreeMap<String, String>);

    /// Send a pipe message called `name` with `payload` to every running
    /// plugin, launching none.
    fn broadcast(&self, name: &str, payload: String);

    /// Run `action` as if the user had pressed its key. `Event::ActionComplete`
    /// carries `context` back once it is done, so the plugin can tell its own
    /// actions apart.
    fn run_action(&self, action: Action, context: BTreeMap<String, String>);

    /// Show `request` in Zellij's prompt popup. Returns the id the answer
    /// comes back with in `Event::PromptResult`.
    fn prompt(&self, request: PromptRequest) -> u64;

    /// Open Zellij's context menu for `target` at `line` and `column` of the
    /// plugin's pane.
    fn open_context_menu(&self, target: ContextMenuTarget, line: usize, column: usize);

    /// Focus the tab at `position`, counted from 1 in the order `TabUpdate`
    /// lists the tabs.
    fn switch_tab_to(&self, position: u32);

    /// Close the tab whose id is `tab_id`, the `TabInfo::tab_id` that stays
    /// the same as tabs move. Closes nothing when no tab has that id.
    fn close_tab_with_id(&self, tab_id: u64);

    /// Focus the terminal pane `terminal_pane_id`, switching to its tab if it
    /// is in another. A pane hidden with `hide_pane_with_id` is shown again:
    /// as a floating pane with `should_float_if_hidden`, otherwise tiled, and
    /// in place of the focused pane with `should_be_in_place_if_hidden`.
    fn focus_terminal_pane(
        &self,
        terminal_pane_id: u32,
        should_float_if_hidden: bool,
        should_be_in_place_if_hidden: bool,
    );

    /// Focus the plugin pane `plugin_pane_id`, as `focus_terminal_pane` does
    /// for a terminal.
    fn focus_plugin_pane(
        &self,
        plugin_pane_id: u32,
        should_float_if_hidden: bool,
        should_be_in_place_if_hidden: bool,
    );

    /// Hide the pane `pane_id` from the screen, keeping it and what runs in
    /// it. Zellij reports it as suppressed until it is shown or focused again.
    fn hide_pane_with_id(&self, pane_id: PaneId);

    /// Show the pane `pane_id` and focus it, as a floating pane if it is
    /// hidden and `should_float_if_hidden`. With a `tab_id`, the
    /// `TabInfo::tab_id` of a tab, a pane hidden with `hide_pane_with_id` is
    /// moved to that tab first and shown there; any other pane is shown in
    /// its own tab, switching to it. Returns an error, showing nothing, when
    /// no tab has that id.
    fn show_pane_with_id(
        &self,
        pane_id: PaneId,
        should_float_if_hidden: bool,
        tab_id: Option<usize>,
    ) -> Result<(), String>;

    /// Toggle the pane `pane_id` between filling its tab and its own size.
    fn toggle_pane_id_fullscreen(&self, pane_id: PaneId);

    /// Float the tiled pane `pane_id`, or tile the floating one. Zellij
    /// places a pane it floats as it would a new one, and leaves the last
    /// tiled pane of a tab tiled.
    fn toggle_pane_embed_or_eject_for_pane_id(&self, pane_id: PaneId);

    /// Place `items` at the right of the top row of the pane `pane_id`'s
    /// frame, in place of the ones this plugin placed there before. No items
    /// takes them off. They stay until the pane closes or the plugin
    /// unloads, and a click on a clickable one comes back as
    /// `Event::FrameItemClicked`.
    fn set_pane_frame_items(&self, pane_id: PaneId, items: Vec<FrameItem>);

    /// Move and resize each floating pane named in
    /// `pane_ids_and_coordinates` to its coordinates. A coordinate left
    /// `None` keeps its value.
    fn change_floating_panes_coordinates(
        &self,
        pane_ids_and_coordinates: Vec<(PaneId, FloatingPaneCoordinates)>,
    );

    /// Close the terminal pane `terminal_pane_id`, ending what runs in it.
    fn close_terminal_pane(&self, terminal_pane_id: u32);

    /// Close the plugin pane `plugin_pane_id`.
    fn close_plugin_pane(&self, plugin_pane_id: u32);

    /// Open a new floating pane running `command`, at `coordinates` or
    /// wherever Zellij finds room. `context` comes back with the events
    /// about the pane. A command with no `cwd` runs in the working directory
    /// of the pane focused when it opens, as a pane Zellij's own keys open
    /// does. A relative `cwd` is taken from this plugin's own working
    /// directory, and Zellij maps a `cwd` under `/host`, `/data`, `/cache` or
    /// `/tmp` to the plugin's own folders, as it maps every path a plugin
    /// gives it.
    fn open_command_pane_floating(
        &self,
        command: CommandToRun,
        coordinates: Option<FloatingPaneCoordinates>,
        context: BTreeMap<String, String>,
    );

    /// Open the plugin at `url`, a URL or an alias, in a new floating pane
    /// with `configuration`, at `coordinates` or wherever Zellij finds room.
    fn open_plugin_pane_floating(
        &self,
        url: &str,
        configuration: BTreeMap<String, String>,
        coordinates: Option<FloatingPaneCoordinates>,
        context: BTreeMap<String, String>,
    );

    /// Open the plugin's settings screen as a floating pane titled
    /// `pane_title`, or reach the one already open, with the pipe message
    /// `message`. `own_url` is the URL of the bar asking: when it is an alias
    /// name, the screen edits that alias. `instance` is the bar's own
    /// `instance` option, so the screen runs as a slot of the bar's shared
    /// instance. See `settings_configuration`.
    ///
    /// The message is addressed to `zellij:OWN_URL`, which stands for this
    /// plugin's own location. A message addressed by URL goes to a running
    /// slot whose configuration matches exactly, and launches one only when
    /// there is none, so a second request finds the screen already open.
    fn open_settings(
        &self,
        message: &str,
        pane_title: &str,
        own_url: Option<&str>,
        instance: Option<&str>,
    );

    /// Add the bar's entry, titled `plugin`, to Zellij's settings screen.
    /// Bars started the same way share one entry, and choosing it reaches all
    /// of them with `OPEN_PLUGIN_SETTINGS_MESSAGE`; see
    /// `settings_requested_by`.
    fn add_settings_entry(&self, plugin: &str);

    /// Open a hover preview listing `entries`: the plugin at `url` again, as
    /// a popup tagged `tag`, at `(line, column)` of the bar's pane and
    /// sized to fit the entries. It does not take the focus, and Zellij keeps
    /// it on screen, above a bar at the bottom. `instance` is the bar's own
    /// `instance` option, so the popup runs as a slot of the bar's shared
    /// instance, as the settings screen does. See `popup`.
    fn open_preview_popup(
        &self,
        names: &popup::Names,
        url: &str,
        tag: &str,
        entries: &[String],
        at: (usize, usize),
        instance: Option<&str>,
    );

    /// Close the hover preview whose slot id is `popup`, which it told the
    /// bar when it opened (see `popup::POPUP_SLOT_ARG`).
    fn close_preview_popup(&self, names: &popup::Names, popup: u32);
}

/// The configuration the settings screen is opened with from a bar running
/// as `own_url`: marked as the screen, pinned to the bar's alias when
/// `own_url` is an alias name rather than a location (a location always has
/// a scheme, as in `file:`), and given the bar's `instance` option, if it has
/// one, so the screen joins the bar's shared instance.
pub fn settings_configuration(
    own_url: Option<&str>,
    instance: Option<&str>,
) -> BTreeMap<String, String> {
    let mut configuration = BTreeMap::from([(SETTINGS_KEY.to_string(), "true".to_string())]);
    if let Some(alias) = own_url.filter(|url| !url.contains(':')) {
        configuration.insert(ALIAS_KEY.to_string(), alias.to_string());
    }
    if let Some(instance) = instance {
        configuration.insert(INSTANCE_KEY.to_string(), instance.to_string());
    }
    configuration
}

/// Whether a request from Zellij's settings screen, with `args`, came from
/// the client `client_id` draws for. Every client's instance of the chosen
/// bar receives it, and only the one the user is looking at should answer.
pub fn settings_requested_by(args: &BTreeMap<String, String>, client_id: Option<ClientId>) -> bool {
    args.get(OPEN_PLUGIN_SETTINGS_CLIENT_ID_ARG)
        .and_then(|id| id.parse::<ClientId>().ok())
        .is_some_and(|id| Some(id) == client_id)
}

/// The popup `Host::open_preview_popup` opens.
fn preview_popup_request(
    names: &popup::Names,
    url: &str,
    tag: &str,
    entries: &[String],
    (line, column): (usize, usize),
    instance: Option<&str>,
) -> PopupRequest {
    let (width, height) = popup::size(entries);
    let mut configuration = popup::configuration(names, tag, entries);
    if let Some(instance) = instance {
        configuration.insert(INSTANCE_KEY.to_string(), instance.to_string());
    }
    PopupRequest {
        url: url.to_string(),
        configuration,
        line,
        column,
        width,
        height,
        options: PopupOptions::default().focused(false),
    }
}

/// The real host: each method calls the matching `zellij_tile` function,
/// or hands the call to `shared` for the acting slot.
#[derive(Clone, Copy, Debug, Default)]
pub struct Zellij;

impl Host for Zellij {
    fn plugin_ids(&self) -> PluginIds {
        let mut ids = get_plugin_ids();
        if let Some(acting) = shared::acting() {
            ids.plugin_id = acting.slot;
            if let Some(client_id) = acting.client {
                ids.client_id = client_id;
            }
        }
        ids
    }

    fn instance_id(&self) -> u32 {
        get_plugin_ids().plugin_id
    }

    fn request_permission(&self, permissions: &[PermissionType]) {
        if !shared::submit(Command::RequestPermission(permissions.to_vec())) {
            request_permission(permissions);
        }
    }

    fn subscribe(&self, events: &[EventType]) {
        subscribe(events);
    }

    fn set_selectable(&self, selectable: bool) {
        if !shared::submit(Command::SetSelectable(selectable)) {
            set_selectable(selectable);
        }
    }

    fn set_collapsed(&self, collapsed: bool) {
        if !shared::submit(Command::SetCollapsed(collapsed)) {
            set_self_collapsed(collapsed);
        }
    }

    fn show_self(&self) {
        if !shared::submit(Command::ShowSelf) {
            show_self(true);
        }
    }

    fn close_self(&self) {
        if !shared::submit(Command::CloseSelf) {
            close_self();
        }
    }

    fn set_timeout(&self, seconds: f64) {
        if !shared::submit(Command::SetTimeout(seconds)) {
            set_timeout(seconds);
        }
    }

    fn system_time(&self) -> SystemTime {
        SystemTime::now()
    }

    fn run_command(&self, command: &[&str], context: BTreeMap<String, String>) {
        run_command(command, shared::tag_context(context));
    }

    fn broadcast(&self, name: &str, payload: String) {
        pipe_message_to_plugin(MessageToPlugin::new(name).with_payload(payload));
    }

    fn run_action(&self, action: Action, context: BTreeMap<String, String>) {
        run_action(action, shared::tag_context(context));
    }

    fn prompt(&self, request: PromptRequest) -> u64 {
        let request_id = prompt(request);
        shared::submit(Command::Prompted(request_id));
        request_id
    }

    fn open_context_menu(&self, target: ContextMenuTarget, line: usize, column: usize) {
        let command = Command::OpenContextMenu {
            target,
            line,
            column,
        };
        if !shared::submit(command) {
            open_context_menu(target, line, column);
        }
    }

    fn switch_tab_to(&self, position: u32) {
        switch_tab_to(position);
    }

    fn close_tab_with_id(&self, tab_id: u64) {
        close_tab_with_id(tab_id);
    }

    fn focus_terminal_pane(
        &self,
        terminal_pane_id: u32,
        should_float_if_hidden: bool,
        should_be_in_place_if_hidden: bool,
    ) {
        focus_terminal_pane(
            terminal_pane_id,
            should_float_if_hidden,
            should_be_in_place_if_hidden,
        );
    }

    fn focus_plugin_pane(
        &self,
        plugin_pane_id: u32,
        should_float_if_hidden: bool,
        should_be_in_place_if_hidden: bool,
    ) {
        focus_plugin_pane(
            plugin_pane_id,
            should_float_if_hidden,
            should_be_in_place_if_hidden,
        );
    }

    fn hide_pane_with_id(&self, pane_id: PaneId) {
        hide_pane_with_id(pane_id);
    }

    fn show_pane_with_id(
        &self,
        pane_id: PaneId,
        should_float_if_hidden: bool,
        tab_id: Option<usize>,
    ) -> Result<(), String> {
        let should_focus_pane = true;
        show_pane_with_id(pane_id, should_float_if_hidden, should_focus_pane, tab_id)
    }

    fn toggle_pane_id_fullscreen(&self, pane_id: PaneId) {
        toggle_pane_id_fullscreen(pane_id);
    }

    fn toggle_pane_embed_or_eject_for_pane_id(&self, pane_id: PaneId) {
        toggle_pane_embed_or_eject_for_pane_id(pane_id);
    }

    fn set_pane_frame_items(&self, pane_id: PaneId, items: Vec<FrameItem>) {
        if !shared::submit(Command::SetPaneFrameItems(pane_id, items.clone())) {
            set_pane_frame_items(pane_id, items);
        }
    }

    fn change_floating_panes_coordinates(
        &self,
        pane_ids_and_coordinates: Vec<(PaneId, FloatingPaneCoordinates)>,
    ) {
        change_floating_panes_coordinates(pane_ids_and_coordinates);
    }

    fn close_terminal_pane(&self, terminal_pane_id: u32) {
        close_terminal_pane(terminal_pane_id);
    }

    fn close_plugin_pane(&self, plugin_pane_id: u32) {
        close_plugin_pane(plugin_pane_id);
    }

    fn open_command_pane_floating(
        &self,
        command: CommandToRun,
        coordinates: Option<FloatingPaneCoordinates>,
        context: BTreeMap<String, String>,
    ) {
        open_command_pane_floating(command, coordinates, context);
    }

    fn open_plugin_pane_floating(
        &self,
        url: &str,
        configuration: BTreeMap<String, String>,
        coordinates: Option<FloatingPaneCoordinates>,
        context: BTreeMap<String, String>,
    ) {
        open_plugin_pane_floating(url, configuration, coordinates, context);
    }

    fn open_settings(
        &self,
        message: &str,
        pane_title: &str,
        own_url: Option<&str>,
        instance: Option<&str>,
    ) {
        pipe_message_to_plugin(
            MessageToPlugin::new(message)
                .with_plugin_url("zellij:OWN_URL")
                .with_plugin_config(settings_configuration(own_url, instance))
                .new_plugin_instance_should_float(true)
                .new_plugin_instance_should_be_focused()
                .new_plugin_instance_should_have_pane_title(pane_title),
        );
    }

    fn add_settings_entry(&self, plugin: &str) {
        register_settings_entry(plugin);
    }

    fn open_preview_popup(
        &self,
        names: &popup::Names,
        url: &str,
        tag: &str,
        entries: &[String],
        at: (usize, usize),
        instance: Option<&str>,
    ) {
        let request = preview_popup_request(names, url, tag, entries, at, instance);
        if !shared::submit(Command::OpenPopup(request.clone())) {
            shared::Effect::OpenPopup(request).perform(None);
        }
    }

    fn close_preview_popup(&self, names: &popup::Names, popup: u32) {
        pipe_message_to_plugin(MessageToPlugin::new(names.close).with_destination_plugin_id(popup));
    }
}

/// One call made through a `Recorder`, with what it was given. Each variant
/// is named after its `Host` method.
///
/// A prompt request or an action is far larger than the other variants carry.
/// Calls are only ever kept by tests, so boxing them would make every test
/// that builds one wordier for no gain worth having.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    PluginIds,
    RequestPermission(Vec<PermissionType>),
    Subscribe(Vec<EventType>),
    SetSelectable(bool),
    SetCollapsed(bool),
    ShowSelf,
    CloseSelf,
    SetTimeout(f64),
    RunCommand {
        command: Vec<String>,
        context: BTreeMap<String, String>,
    },
    Broadcast {
        name: String,
        payload: String,
    },
    RunAction {
        action: Action,
        context: BTreeMap<String, String>,
    },
    /// `request_id` is what the call returned. See `Recorder::prompt`.
    Prompt {
        request_id: u64,
        request: PromptRequest,
    },
    OpenContextMenu {
        target: ContextMenuTarget,
        line: usize,
        column: usize,
    },
    SwitchTabTo(u32),
    CloseTabWithId(u64),
    FocusTerminalPane {
        terminal_pane_id: u32,
        should_float_if_hidden: bool,
        should_be_in_place_if_hidden: bool,
    },
    FocusPluginPane {
        plugin_pane_id: u32,
        should_float_if_hidden: bool,
        should_be_in_place_if_hidden: bool,
    },
    HidePaneWithId(PaneId),
    ShowPaneWithId {
        pane_id: PaneId,
        should_float_if_hidden: bool,
        tab_id: Option<usize>,
    },
    TogglePaneIdFullscreen(PaneId),
    TogglePaneEmbedOrEjectForPaneId(PaneId),
    SetPaneFrameItems(PaneId, Vec<FrameItem>),
    ChangeFloatingPanesCoordinates(Vec<(PaneId, FloatingPaneCoordinates)>),
    CloseTerminalPane(u32),
    ClosePluginPane(u32),
    /// The fields of the `CommandToRun` given, which has no `PartialEq` of
    /// its own to compare by. Its border style is left out, as no plugin
    /// sets one.
    OpenCommandPaneFloating {
        path: PathBuf,
        args: Vec<String>,
        cwd: Option<PathBuf>,
        coordinates: Option<FloatingPaneCoordinates>,
        context: BTreeMap<String, String>,
    },
    OpenPluginPaneFloating {
        url: String,
        configuration: BTreeMap<String, String>,
        coordinates: Option<FloatingPaneCoordinates>,
        context: BTreeMap<String, String>,
    },
    OpenSettings {
        message: String,
        pane_title: String,
        own_url: Option<String>,
        instance: Option<String>,
    },
    AddSettingsEntry(String),
    OpenPreviewPopup {
        url: String,
        tag: String,
        entries: Vec<String>,
        line: usize,
        column: usize,
        instance: Option<String>,
    },
    /// `message` is the `close` name of the `popup::Names` given.
    ClosePreviewPopup {
        message: &'static str,
        popup: u32,
    },
}

/// A host for tests: calls nothing, and keeps a `Call` for each method
/// called, in order, until `clear`.
///
/// `Host` methods take `&self`, as a plugin makes calls from methods that
/// only read its state, so the calls are kept in a `RefCell`.
///
/// While a slot is acting (see `shared`), each call is also kept with that
/// slot's id, which `slot_calls` reports, and the calls about the plugin's
/// own pane, its timers and its permissions are handed to `shared` as
/// `Zellij` hands them.
#[derive(Debug, Default)]
pub struct Recorder {
    calls: RefCell<Vec<Call>>,
    /// Each call with the slot acting when it was made.
    slot_calls: RefCell<Vec<(Option<SlotId>, Call)>>,
    /// How many prompts this recorder has been asked to show.
    prompts: Cell<u64>,
    /// What `system_time` reports: the Unix epoch until a test sets it.
    system_time: Cell<Option<SystemTime>>,
}

impl Recorder {
    /// The plugin id `plugin_ids` reports.
    pub const PLUGIN_ID: u32 = 1;
    /// The client id `plugin_ids` reports.
    pub const CLIENT_ID: ClientId = 1;
    /// The instance id `instance_id` reports.
    pub const INSTANCE_ID: u32 = 100;

    /// Every call made since this recorder was created or last cleared, in
    /// the order they were made.
    pub fn calls(&self) -> Vec<Call> {
        self.calls.borrow().clone()
    }

    /// Every call made since this recorder was created or last cleared, each
    /// with the slot that was acting when it was made, if any.
    pub fn slot_calls(&self) -> Vec<(Option<SlotId>, Call)> {
        self.slot_calls.borrow().clone()
    }

    /// Forget the calls made so far, so a test can assert on what one step
    /// does without the steps before it.
    pub fn clear(&self) {
        self.calls.borrow_mut().clear();
        self.slot_calls.borrow_mut().clear();
    }

    /// Have `system_time` report `time` from now on.
    pub fn set_system_time(&self, time: SystemTime) {
        self.system_time.set(Some(time));
    }

    fn record(&self, call: Call) {
        let slot = shared::acting().map(|acting| acting.slot);
        self.slot_calls.borrow_mut().push((slot, call.clone()));
        self.calls.borrow_mut().push(call);
    }

    /// Keep `call`, and hand `command` to `shared` when a slot is acting.
    fn route(&self, call: Call, command: Command) {
        self.record(call);
        shared::submit(command);
    }
}

impl Host for Recorder {
    /// Reports `PLUGIN_ID` and `CLIENT_ID`, a Zellij process id of 0 and an
    /// empty working directory. While a slot is acting, the slot's id is the
    /// plugin id, and the client it acts for, if any, the client id.
    fn plugin_ids(&self) -> PluginIds {
        self.record(Call::PluginIds);
        let acting = shared::acting();
        PluginIds {
            plugin_id: acting.map_or(Self::PLUGIN_ID, |acting| acting.slot),
            zellij_pid: 0,
            initial_cwd: PathBuf::new(),
            client_id: acting
                .and_then(|acting| acting.client)
                .unwrap_or(Self::CLIENT_ID),
        }
    }

    /// Reports `INSTANCE_ID`. Not a `Call`: asking changes nothing.
    fn instance_id(&self) -> u32 {
        Self::INSTANCE_ID
    }

    fn request_permission(&self, permissions: &[PermissionType]) {
        self.route(
            Call::RequestPermission(permissions.to_vec()),
            Command::RequestPermission(permissions.to_vec()),
        );
    }

    fn subscribe(&self, events: &[EventType]) {
        self.record(Call::Subscribe(events.to_vec()));
    }

    fn set_selectable(&self, selectable: bool) {
        self.route(
            Call::SetSelectable(selectable),
            Command::SetSelectable(selectable),
        );
    }

    fn set_collapsed(&self, collapsed: bool) {
        self.route(
            Call::SetCollapsed(collapsed),
            Command::SetCollapsed(collapsed),
        );
    }

    fn show_self(&self) {
        self.route(Call::ShowSelf, Command::ShowSelf);
    }

    fn close_self(&self) {
        self.route(Call::CloseSelf, Command::CloseSelf);
    }

    fn set_timeout(&self, seconds: f64) {
        self.route(Call::SetTimeout(seconds), Command::SetTimeout(seconds));
    }

    /// The time `set_system_time` set, or the Unix epoch until it is set.
    fn system_time(&self) -> SystemTime {
        self.system_time.get().unwrap_or(SystemTime::UNIX_EPOCH)
    }

    fn run_command(&self, command: &[&str], context: BTreeMap<String, String>) {
        self.record(Call::RunCommand {
            command: command.iter().map(|part| part.to_string()).collect(),
            context: shared::tag_context(context),
        });
    }

    fn broadcast(&self, name: &str, payload: String) {
        self.record(Call::Broadcast {
            name: name.to_string(),
            payload,
        });
    }

    fn run_action(&self, action: Action, context: BTreeMap<String, String>) {
        self.record(Call::RunAction {
            action,
            context: shared::tag_context(context),
        });
    }

    /// Numbers prompts 1, 2, 3 and so on in the order they are shown, as
    /// Zellij does. Clearing the calls does not start the count again, so an
    /// id is never handed out twice.
    fn prompt(&self, request: PromptRequest) -> u64 {
        let request_id = self.prompts.get() + 1;
        self.prompts.set(request_id);
        self.route(
            Call::Prompt {
                request_id,
                request,
            },
            Command::Prompted(request_id),
        );
        request_id
    }

    fn open_context_menu(&self, target: ContextMenuTarget, line: usize, column: usize) {
        self.route(
            Call::OpenContextMenu {
                target,
                line,
                column,
            },
            Command::OpenContextMenu {
                target,
                line,
                column,
            },
        );
    }

    fn switch_tab_to(&self, position: u32) {
        self.record(Call::SwitchTabTo(position));
    }

    fn close_tab_with_id(&self, tab_id: u64) {
        self.record(Call::CloseTabWithId(tab_id));
    }

    fn focus_terminal_pane(
        &self,
        terminal_pane_id: u32,
        should_float_if_hidden: bool,
        should_be_in_place_if_hidden: bool,
    ) {
        self.record(Call::FocusTerminalPane {
            terminal_pane_id,
            should_float_if_hidden,
            should_be_in_place_if_hidden,
        });
    }

    fn focus_plugin_pane(
        &self,
        plugin_pane_id: u32,
        should_float_if_hidden: bool,
        should_be_in_place_if_hidden: bool,
    ) {
        self.record(Call::FocusPluginPane {
            plugin_pane_id,
            should_float_if_hidden,
            should_be_in_place_if_hidden,
        });
    }

    fn hide_pane_with_id(&self, pane_id: PaneId) {
        self.record(Call::HidePaneWithId(pane_id));
    }

    /// Shows the pane every time, as Zellij does for a tab that exists.
    fn show_pane_with_id(
        &self,
        pane_id: PaneId,
        should_float_if_hidden: bool,
        tab_id: Option<usize>,
    ) -> Result<(), String> {
        self.record(Call::ShowPaneWithId {
            pane_id,
            should_float_if_hidden,
            tab_id,
        });
        Ok(())
    }

    fn toggle_pane_id_fullscreen(&self, pane_id: PaneId) {
        self.record(Call::TogglePaneIdFullscreen(pane_id));
    }

    fn toggle_pane_embed_or_eject_for_pane_id(&self, pane_id: PaneId) {
        self.record(Call::TogglePaneEmbedOrEjectForPaneId(pane_id));
    }

    fn set_pane_frame_items(&self, pane_id: PaneId, items: Vec<FrameItem>) {
        self.route(
            Call::SetPaneFrameItems(pane_id, items.clone()),
            Command::SetPaneFrameItems(pane_id, items),
        );
    }

    fn change_floating_panes_coordinates(
        &self,
        pane_ids_and_coordinates: Vec<(PaneId, FloatingPaneCoordinates)>,
    ) {
        self.record(Call::ChangeFloatingPanesCoordinates(
            pane_ids_and_coordinates,
        ));
    }

    fn close_terminal_pane(&self, terminal_pane_id: u32) {
        self.record(Call::CloseTerminalPane(terminal_pane_id));
    }

    fn close_plugin_pane(&self, plugin_pane_id: u32) {
        self.record(Call::ClosePluginPane(plugin_pane_id));
    }

    fn open_command_pane_floating(
        &self,
        command: CommandToRun,
        coordinates: Option<FloatingPaneCoordinates>,
        context: BTreeMap<String, String>,
    ) {
        self.record(Call::OpenCommandPaneFloating {
            path: command.path,
            args: command.args,
            cwd: command.cwd,
            coordinates,
            context,
        });
    }

    fn open_plugin_pane_floating(
        &self,
        url: &str,
        configuration: BTreeMap<String, String>,
        coordinates: Option<FloatingPaneCoordinates>,
        context: BTreeMap<String, String>,
    ) {
        self.record(Call::OpenPluginPaneFloating {
            url: url.to_string(),
            configuration,
            coordinates,
            context,
        });
    }

    fn open_settings(
        &self,
        message: &str,
        pane_title: &str,
        own_url: Option<&str>,
        instance: Option<&str>,
    ) {
        self.record(Call::OpenSettings {
            message: message.to_string(),
            pane_title: pane_title.to_string(),
            own_url: own_url.map(str::to_string),
            instance: instance.map(str::to_string),
        });
    }

    fn add_settings_entry(&self, plugin: &str) {
        self.record(Call::AddSettingsEntry(plugin.to_string()));
    }

    fn open_preview_popup(
        &self,
        names: &popup::Names,
        url: &str,
        tag: &str,
        entries: &[String],
        (line, column): (usize, usize),
        instance: Option<&str>,
    ) {
        self.route(
            Call::OpenPreviewPopup {
                url: url.to_string(),
                tag: tag.to_string(),
                entries: entries.to_vec(),
                line,
                column,
                instance: instance.map(str::to_string),
            },
            Command::OpenPopup(preview_popup_request(
                names,
                url,
                tag,
                entries,
                (line, column),
                instance,
            )),
        );
    }

    fn close_preview_popup(&self, names: &popup::Names, popup: u32) {
        self.record(Call::ClosePreviewPopup {
            message: names.close,
            popup,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_opened_from_an_alias_bar_edit_that_alias() {
        let configuration = settings_configuration(Some("zjtest-alt"), None);
        assert_eq!(
            configuration.get("alias").map(String::as_str),
            Some("zjtest-alt")
        );
        assert_eq!(
            configuration.get("settings").map(String::as_str),
            Some("true")
        );
        // A bar started from a location rather than an alias pins nothing.
        let configuration = settings_configuration(Some("file:/x/zjtest.wasm"), None);
        assert_eq!(configuration.get("alias"), None);
        assert_eq!(settings_configuration(None, None).get("alias"), None);
    }

    #[test]
    fn settings_open_in_the_shared_instance_of_the_bar_that_opened_them() {
        let configuration = settings_configuration(Some("zjtest"), Some("work"));
        assert_eq!(
            configuration.get("instance").map(String::as_str),
            Some("work")
        );
        assert_eq!(
            settings_configuration(Some("zjtest"), None).get("instance"),
            None
        );
    }

    #[test]
    fn a_preview_popup_opens_in_the_shared_instance_of_the_bar_that_opened_it() {
        static NAMES: popup::Names = crate::popup_names!("zjtest");
        let entries = vec!["a".to_string()];
        let request =
            |instance| preview_popup_request(&NAMES, "zjtest", "7-1", &entries, (1, 3), instance);
        assert_eq!(
            request(Some("work")).configuration.get("instance"),
            Some(&"work".to_string())
        );
        // The popup keeps what marks it as one.
        assert_eq!(request(Some("work")).configuration, {
            let mut expected = popup::configuration(&NAMES, "7-1", &entries);
            expected.insert("instance".to_string(), "work".to_string());
            expected
        });
        assert_eq!(request(None).configuration.get("instance"), None);
    }

    #[test]
    fn only_the_client_that_chose_the_settings_entry_opens_settings() {
        let from = |client: &str| {
            BTreeMap::from([(
                OPEN_PLUGIN_SETTINGS_CLIENT_ID_ARG.to_string(),
                client.to_string(),
            )])
        };
        assert!(settings_requested_by(&from("3"), Some(3)));
        assert!(!settings_requested_by(&from("4"), Some(3)));
        assert!(!settings_requested_by(&from("3"), None));
        assert!(!settings_requested_by(&from("three"), Some(3)));
        assert!(!settings_requested_by(&BTreeMap::new(), Some(3)));
    }

    fn question() -> PromptRequest {
        PromptRequest::menu([ChoiceItem::labeled("0", "Sure")])
    }

    #[test]
    fn a_recorder_numbers_prompts_from_one_and_never_reuses_an_id() {
        let host = Recorder::default();
        assert_eq!(host.prompt(question()), 1);
        assert_eq!(host.prompt(question()), 2);
        host.clear();
        assert_eq!(host.prompt(question()), 3);
        assert_eq!(
            host.calls(),
            vec![Call::Prompt {
                request_id: 3,
                request: question(),
            }]
        );
    }
}
