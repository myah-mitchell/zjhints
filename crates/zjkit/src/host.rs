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
//! `ask_visibility`, so each plugin passes only its own names. Everything
//! else is one method per Zellij call.
//!
//! The popup of a hover preview and the settings screen are whole plugins of
//! their own (see `popup` and `settings::screen`), and call Zellij
//! directly. Their tests drive the parts that make no calls.

use crate::{popup, visibility};
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

/// Everything a bar asks of Zellij.
pub trait Host {
    /// This instance's plugin id and the client it draws for.
    fn plugin_ids(&self) -> PluginIds;

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

    /// Have Zellij send `Event::Timer` after `seconds`.
    fn set_timeout(&self, seconds: f64);

    /// The current time of day, as the system clock has it.
    ///
    /// Not a `Call`: asking the time changes nothing, and a bar asks it on
    /// every draw that shows a clock. A test sets the time a `Recorder`
    /// reports with `Recorder::set_system_time`.
    fn system_time(&self) -> SystemTime;

    /// The location plugin `plugin_id` of this session runs from, written
    /// the way a `PaneUpdate` writes the URL of a pane started from a
    /// location: `file:/path/to/plugin.wasm`, say. For a plugin started from
    /// an alias, this is the location the alias stands for, where its own
    /// pane reports the alias name. `None` when Zellij cannot say. Needs the
    /// `ReadApplicationState` permission.
    ///
    /// Not a `Call`: asking changes nothing. A test sets what a `Recorder`
    /// reports with `Recorder::set_plugin_location`.
    fn plugin_location(&self, plugin_id: u32) -> Option<String>;

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

    /// Ask every running bar of the plugin `names` belongs to whether it is
    /// hidden, naming this bar's `alias` when it knows it. See
    /// `visibility::Names::ask`.
    fn ask_visibility(&self, names: &visibility::Names, alias: Option<&str>);

    /// Tell the bar `plugin_id`, which asked, whether this one is hidden.
    /// `answer` is `visibility::Visibility::answer`.
    fn answer_visibility(&self, names: &visibility::Names, plugin_id: u32, answer: &str);

    /// Open the plugin's settings screen as a floating pane titled
    /// `pane_title`, or reach the one already open, with the pipe message
    /// `message`. `own_url` is the URL of the bar asking: when it is an alias
    /// name, the screen edits that alias. See `settings_configuration`.
    ///
    /// The message is addressed to `zellij:OWN_URL`, which stands for this
    /// plugin's own location. A message addressed by URL goes to a running
    /// instance whose configuration matches exactly, and launches one only
    /// when there is none, so a second request finds the screen already open.
    fn open_settings(&self, message: &str, pane_title: &str, own_url: Option<&str>);

    /// Add the bar's entry, titled `plugin`, to Zellij's settings screen.
    /// Bars started the same way share one entry, and choosing it reaches all
    /// of them with `OPEN_PLUGIN_SETTINGS_MESSAGE`; see
    /// `settings_requested_by`.
    fn add_settings_entry(&self, plugin: &str);

    /// Open a hover preview listing `entries`: the plugin at `url` again, as
    /// a popup tagged `tag`, at `line` and `column` of the bar's pane and
    /// sized to fit the entries. It does not take the focus, and Zellij keeps
    /// it on screen, above a bar at the bottom. See `popup`.
    fn open_preview_popup(
        &self,
        names: &popup::Names,
        url: &str,
        tag: &str,
        entries: &[String],
        line: usize,
        column: usize,
    );

    /// Close the hover preview whose plugin id is `popup`.
    fn close_preview_popup(&self, names: &popup::Names, popup: u32);
}

/// The configuration the settings screen is opened with from a bar running
/// as `own_url`: marked as the screen, and pinned to the bar's alias when
/// `own_url` is an alias name rather than a location (a location always has
/// a scheme, as in `file:`).
pub fn settings_configuration(own_url: Option<&str>) -> BTreeMap<String, String> {
    let mut configuration = BTreeMap::from([(SETTINGS_KEY.to_string(), "true".to_string())]);
    if let Some(alias) = own_url.filter(|url| !url.contains(':')) {
        configuration.insert(ALIAS_KEY.to_string(), alias.to_string());
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

/// The real host: each method calls the matching `zellij_tile` function.
#[derive(Clone, Copy, Debug, Default)]
pub struct Zellij;

impl Host for Zellij {
    fn plugin_ids(&self) -> PluginIds {
        get_plugin_ids()
    }

    fn request_permission(&self, permissions: &[PermissionType]) {
        request_permission(permissions);
    }

    fn subscribe(&self, events: &[EventType]) {
        subscribe(events);
    }

    fn set_selectable(&self, selectable: bool) {
        set_selectable(selectable);
    }

    fn set_collapsed(&self, collapsed: bool) {
        set_self_collapsed(collapsed);
    }

    fn show_self(&self) {
        show_self(true);
    }

    fn set_timeout(&self, seconds: f64) {
        set_timeout(seconds);
    }

    fn system_time(&self) -> SystemTime {
        SystemTime::now()
    }

    /// Read from the plugins Zellij lists for this session, which it keeps
    /// by plugin id with the alias already resolved to its location.
    fn plugin_location(&self, plugin_id: u32) -> Option<String> {
        let snapshot = get_session_list().ok()?;
        let mut session = snapshot
            .live_sessions
            .into_iter()
            .find(|session| session.is_current_session)?;
        session
            .plugins
            .remove(&plugin_id)
            .map(|plugin| plugin.location)
    }

    fn run_command(&self, command: &[&str], context: BTreeMap<String, String>) {
        run_command(command, context);
    }

    fn broadcast(&self, name: &str, payload: String) {
        pipe_message_to_plugin(MessageToPlugin::new(name).with_payload(payload));
    }

    fn run_action(&self, action: Action, context: BTreeMap<String, String>) {
        run_action(action, context);
    }

    fn prompt(&self, request: PromptRequest) -> u64 {
        prompt(request)
    }

    fn open_context_menu(&self, target: ContextMenuTarget, line: usize, column: usize) {
        open_context_menu(target, line, column);
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
        set_pane_frame_items(pane_id, items);
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

    fn ask_visibility(&self, names: &visibility::Names, alias: Option<&str>) {
        let mut message = MessageToPlugin::new(names.ask);
        if let Some(alias) = alias {
            message = message.with_payload(alias);
        }
        pipe_message_to_plugin(message);
    }

    fn answer_visibility(&self, names: &visibility::Names, plugin_id: u32, answer: &str) {
        pipe_message_to_plugin(
            MessageToPlugin::new(names.answer)
                .with_destination_plugin_id(plugin_id)
                .with_payload(answer),
        );
    }

    fn open_settings(&self, message: &str, pane_title: &str, own_url: Option<&str>) {
        pipe_message_to_plugin(
            MessageToPlugin::new(message)
                .with_plugin_url("zellij:OWN_URL")
                .with_plugin_config(settings_configuration(own_url))
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
        line: usize,
        column: usize,
    ) {
        let (width, height) = popup::size(entries);
        open_plugin_popup_with_options(
            url,
            popup::configuration(names, tag, entries),
            line,
            column,
            width,
            height,
            PopupOptions::default().focused(false),
        );
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
    /// `message` is the `ask` name of the `visibility::Names` given.
    AskVisibility {
        message: &'static str,
        alias: Option<String>,
    },
    /// `message` is the `answer` name of the `visibility::Names` given.
    AnswerVisibility {
        message: &'static str,
        plugin_id: u32,
        answer: String,
    },
    OpenSettings {
        message: String,
        pane_title: String,
        own_url: Option<String>,
    },
    AddSettingsEntry(String),
    OpenPreviewPopup {
        url: String,
        tag: String,
        entries: Vec<String>,
        line: usize,
        column: usize,
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
#[derive(Debug, Default)]
pub struct Recorder {
    calls: RefCell<Vec<Call>>,
    /// How many prompts this recorder has been asked to show.
    prompts: Cell<u64>,
    /// What `system_time` reports: the Unix epoch until a test sets it.
    system_time: Cell<Option<SystemTime>>,
    /// What `plugin_location` reports, by plugin id: nothing until a test
    /// sets it.
    plugin_locations: RefCell<BTreeMap<u32, String>>,
}

impl Recorder {
    /// The plugin id `plugin_ids` reports.
    pub const PLUGIN_ID: u32 = 1;
    /// The client id `plugin_ids` reports.
    pub const CLIENT_ID: ClientId = 1;

    /// Every call made since this recorder was created or last cleared, in
    /// the order they were made.
    pub fn calls(&self) -> Vec<Call> {
        self.calls.borrow().clone()
    }

    /// Forget the calls made so far, so a test can assert on what one step
    /// does without the steps before it.
    pub fn clear(&self) {
        self.calls.borrow_mut().clear();
    }

    /// Have `system_time` report `time` from now on.
    pub fn set_system_time(&self, time: SystemTime) {
        self.system_time.set(Some(time));
    }

    /// Have `plugin_location` report `location` for plugin `plugin_id` from
    /// now on.
    pub fn set_plugin_location(&self, plugin_id: u32, location: &str) {
        self.plugin_locations
            .borrow_mut()
            .insert(plugin_id, location.to_string());
    }

    fn record(&self, call: Call) {
        self.calls.borrow_mut().push(call);
    }
}

impl Host for Recorder {
    /// Reports `PLUGIN_ID` and `CLIENT_ID`, a Zellij process id of 0 and an
    /// empty working directory.
    fn plugin_ids(&self) -> PluginIds {
        self.record(Call::PluginIds);
        PluginIds {
            plugin_id: Self::PLUGIN_ID,
            zellij_pid: 0,
            initial_cwd: PathBuf::new(),
            client_id: Self::CLIENT_ID,
        }
    }

    fn request_permission(&self, permissions: &[PermissionType]) {
        self.record(Call::RequestPermission(permissions.to_vec()));
    }

    fn subscribe(&self, events: &[EventType]) {
        self.record(Call::Subscribe(events.to_vec()));
    }

    fn set_selectable(&self, selectable: bool) {
        self.record(Call::SetSelectable(selectable));
    }

    fn set_collapsed(&self, collapsed: bool) {
        self.record(Call::SetCollapsed(collapsed));
    }

    fn show_self(&self) {
        self.record(Call::ShowSelf);
    }

    fn set_timeout(&self, seconds: f64) {
        self.record(Call::SetTimeout(seconds));
    }

    /// The time `set_system_time` set, or the Unix epoch until it is set.
    fn system_time(&self) -> SystemTime {
        self.system_time.get().unwrap_or(SystemTime::UNIX_EPOCH)
    }

    /// The location `set_plugin_location` set for `plugin_id`, if any.
    fn plugin_location(&self, plugin_id: u32) -> Option<String> {
        self.plugin_locations.borrow().get(&plugin_id).cloned()
    }

    fn run_command(&self, command: &[&str], context: BTreeMap<String, String>) {
        self.record(Call::RunCommand {
            command: command.iter().map(|part| part.to_string()).collect(),
            context,
        });
    }

    fn broadcast(&self, name: &str, payload: String) {
        self.record(Call::Broadcast {
            name: name.to_string(),
            payload,
        });
    }

    fn run_action(&self, action: Action, context: BTreeMap<String, String>) {
        self.record(Call::RunAction { action, context });
    }

    /// Numbers prompts 1, 2, 3 and so on in the order they are shown, as
    /// Zellij does. Clearing the calls does not start the count again, so an
    /// id is never handed out twice.
    fn prompt(&self, request: PromptRequest) -> u64 {
        let request_id = self.prompts.get() + 1;
        self.prompts.set(request_id);
        self.record(Call::Prompt {
            request_id,
            request,
        });
        request_id
    }

    fn open_context_menu(&self, target: ContextMenuTarget, line: usize, column: usize) {
        self.record(Call::OpenContextMenu {
            target,
            line,
            column,
        });
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
        self.record(Call::SetPaneFrameItems(pane_id, items));
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

    fn ask_visibility(&self, names: &visibility::Names, alias: Option<&str>) {
        self.record(Call::AskVisibility {
            message: names.ask,
            alias: alias.map(str::to_string),
        });
    }

    fn answer_visibility(&self, names: &visibility::Names, plugin_id: u32, answer: &str) {
        self.record(Call::AnswerVisibility {
            message: names.answer,
            plugin_id,
            answer: answer.to_string(),
        });
    }

    fn open_settings(&self, message: &str, pane_title: &str, own_url: Option<&str>) {
        self.record(Call::OpenSettings {
            message: message.to_string(),
            pane_title: pane_title.to_string(),
            own_url: own_url.map(str::to_string),
        });
    }

    fn add_settings_entry(&self, plugin: &str) {
        self.record(Call::AddSettingsEntry(plugin.to_string()));
    }

    fn open_preview_popup(
        &self,
        _names: &popup::Names,
        url: &str,
        tag: &str,
        entries: &[String],
        line: usize,
        column: usize,
    ) {
        self.record(Call::OpenPreviewPopup {
            url: url.to_string(),
            tag: tag.to_string(),
            entries: entries.to_vec(),
            line,
            column,
        });
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
        let configuration = settings_configuration(Some("zjtest-alt"));
        assert_eq!(
            configuration.get("alias").map(String::as_str),
            Some("zjtest-alt")
        );
        assert_eq!(
            configuration.get("settings").map(String::as_str),
            Some("true")
        );
        // A bar started from a location rather than an alias pins nothing.
        let configuration = settings_configuration(Some("file:/x/zjtest.wasm"));
        assert_eq!(configuration.get("alias"), None);
        assert_eq!(settings_configuration(None).get("alias"), None);
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
