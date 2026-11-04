//! zjtaskbar: a taskbar for Zellij's floating panes, drawn from format
//! strings in the same style as zjhints and zjtabs, and sharing their
//! themes, their settings screen and the bar plumbing in `zjkit`.
//!
//! The bar lists the session's floating panes as entries, grouped by tab,
//! with the focused one highlighted and the minimized ones marked. See
//! `entries` for which panes it lists and in what order, `draw` for how it
//! draws them, and `mouse` for what clicking them does. A launcher button
//! at the bar's left end opens new floating panes; see `launcher`. A tray
//! at its right end shows the time, the session's name or the input mode;
//! see `tray` and `clock`.
//!
//! The plugin is this library's `State`, which `main.rs` registers with
//! Zellij. It is a library so that other crates' tests can draw it into the
//! docs images, through `draw_for_image` behind the `images` feature.

use std::collections::BTreeMap;
use std::time::Instant;
use zellij_tile::prelude::*;
use zjkit::bar::{BarCore, BarPlugin, BarSpec, Role};
use zjkit::config;
use zjkit::mouse::{ColumnSpans, PendingPrompt};

mod clock;
mod draw;
mod entries;
mod flags;
mod frame;
mod host;
#[cfg(test)]
mod images;
mod launcher;
mod mouse;
mod settings;
mod theme;
mod tray;

use draw::Clickable;
use entries::{Entry, Minimized, PaneScope, Tab};
use flags::FLAGS;
use host::*;
use tray::TrayItem;

const CONFIG_PANE_SCOPE: &str = "pane_scope";
const CONFIG_ENTRY_FORMAT: &str = "entry_format";
const CONFIG_FOCUSED_ENTRY_FORMAT: &str = "focused_entry_format";
const CONFIG_MINIMIZED_ENTRY_FORMAT: &str = "minimized_entry_format";
const CONFIG_ENTRY_SPACER: &str = "entry_spacer";
const CONFIG_TAB_SEPARATOR: &str = "tab_separator";
const CONFIG_EMPTY_FORMAT: &str = "empty_format";
const CONFIG_MINIMIZED_FORMAT: &str = "minimized_format";
const CONFIG_EXITED_FORMAT: &str = "exited_format";
const CONFIG_PLUGIN_FORMAT: &str = "plugin_format";
const CONFIG_OTHER_TAB_FORMAT: &str = "other_tab_format";
const CONFIG_OVERFLOW_LEFT_FORMAT: &str = "overflow_left_format";
const CONFIG_OVERFLOW_RIGHT_FORMAT: &str = "overflow_right_format";
const CONFIG_OVERFLOW_STR: &str = "overflow_str";
const CONFIG_AUTO_WIDTH: &str = "auto_width";
const CONFIG_MAX_LENGTH: &str = "max_length";
const CONFIG_HIDE_WHEN_NESTED: &str = "hide_when_nested";
const CONFIG_COLLAPSE_WHEN_EMPTY: &str = "collapse_when_empty";
const CONFIG_DIM_WHEN_UNFOCUSED: &str = "dim_when_unfocused";
const CONFIG_DIM_STRENGTH: &str = "dim_strength";
const CONFIG_MOUSE: &str = "mouse";
const CONFIG_MOUSE_WHEEL_REVERSED: &str = "mouse_wheel_reversed";
const CONFIG_MOUSE_CLOSE_CONFIRM: &str = "mouse_close_confirm";
const CONFIG_FRAME_BUTTONS: &str = "frame_buttons";
const CONFIG_FRAME_MINIMIZE_TEXT: &str = "frame_minimize_text";
const CONFIG_FRAME_MAXIMIZE_TEXT: &str = "frame_maximize_text";
const CONFIG_FRAME_RESTORE_TEXT: &str = "frame_restore_text";
const CONFIG_FRAME_CLOSE_TEXT: &str = "frame_close_text";
const CONFIG_SHOW_LAUNCHER: &str = "show_launcher";
const CONFIG_LAUNCHER_FORMAT: &str = "launcher_format";
const CONFIG_LAUNCHER_SYMBOL: &str = "launcher_symbol";
const CONFIG_LAUNCHER_NEW_TERMINAL: &str = "launcher_new_terminal";
const CONFIG_LAUNCHER_ZELLIJ_ACTIONS: &str = "launcher_zellij_actions";
const CONFIG_TRAY: &str = "tray";
const CONFIG_TRAY_SPACER: &str = "tray_spacer";
const CONFIG_CLOCK_FORMAT: &str = "clock_format";
const CONFIG_CLOCK_ITEM_FORMAT: &str = "clock_item_format";
const CONFIG_CLOCK_UTC_OFFSET: &str = "clock_utc_offset";
const CONFIG_SHOW_SESSION: &str = "show_session";
const CONFIG_SESSION_FORMAT: &str = "session_format";

// Every tab's floating panes, as a desktop taskbar lists the windows of
// every workspace: a pane left open in another tab is still one click away,
// rather than out of sight until that tab is visited.
const DEFAULT_PANE_SCOPE: &str = "all";
// One space, so neighboring entries drawn in Zellij's ribbon colors read as
// separate blocks rather than one long ribbon. The same as zjtabs' spacer.
const DEFAULT_ENTRY_SPACER: &str = " ";
// A line between the groups of two tabs, with a space each side, so where
// one tab's panes end and the next tab's begin reads at a glance.
const DEFAULT_TAB_SEPARATOR: &str = " \u{2502} ";
// Words rather than symbols, in the form zjtabs' flags take after Zellij's
// own, so the defaults need no particular font; an `alias_theme` swaps them
// for symbols.
const DEFAULT_MINIMIZED_FORMAT: &str = " (MINIMIZED)";
const DEFAULT_EXITED_FORMAT: &str = " (EXITED)";
const DEFAULT_PLUGIN_FORMAT: &str = " (PLUGIN)";
// The tab's name rather than a word for "elsewhere", as the name is what
// tells the user where the pane is.
const DEFAULT_OTHER_TAB_FORMAT: &str = " ({tab})";
// The same as zjhints and zjtabs, so the bars cut the same way.
const DEFAULT_OVERFLOW_STR: &str = "...";
// On by default: the bar has a pane of its own, and fitting it is what keeps
// the focused entry on screen as the window narrows.
const DEFAULT_AUTO_WIDTH: bool = true;
// No cap of its own; `auto_width` already fits the pane.
const DEFAULT_MAX_LENGTH: usize = 0;
// A nested session gets no taskbar by default, since the host's bars are
// drawn just beside it (see `zjkit::nesting::is_nested`). The same as zjhints
// and zjtabs.
const DEFAULT_HIDE_WHEN_NESTED: bool = true;
// Hand the row back to the panes around it while there is nothing on it,
// rather than leaving a blank one, as with no floating panes open. The same
// as zjhints and zjtabs.
const DEFAULT_COLLAPSE_WHEN_EMPTY: bool = true;
// Fade the bar while this session is not the one receiving input. The same
// as zjhints and zjtabs, so a shared layout's bars dim in step.
const DEFAULT_DIM_WHEN_UNFOCUSED: bool = true;
const DEFAULT_DIM_STRENGTH: f32 = 0.5;
// On by default, as in zjtabs and unlike zjhints: clicking an entry to bring
// its pane up is what a taskbar is for, and a desktop taskbar does it.
const DEFAULT_MOUSE: bool = true;
// The way zjtabs and Zellij's own tab bar turn the wheel: scrolling up goes
// to the next one, to the right.
const DEFAULT_MOUSE_WHEEL_REVERSED: bool = false;
// Ask first: closing a pane ends what runs in it, and a middle click is easy
// to make by accident. The same as zjtabs.
const DEFAULT_MOUSE_CLOSE_CONFIRM: bool = true;
// On by default, as a desktop window has them: a pane the bar lists can be
// put away, filled out or closed from its own frame. They need no mouse
// option, as the clicks land on the frame rather than the bar.
const DEFAULT_FRAME_BUTTONS: bool = true;
// Shapes from the box drawing, geometric and dingbat blocks, which the
// fonts that draw Zellij's frames have, rather than Nerd Font glyphs. Each
// starts with a space, as Zellij puts nothing between a plugin's items; the
// last ends with one too, before the frame's own indications.
const DEFAULT_FRAME_MINIMIZE_TEXT: &str = " [\u{2500}]";
const DEFAULT_FRAME_MAXIMIZE_TEXT: &str = " [\u{25a1}]";
const DEFAULT_FRAME_RESTORE_TEXT: &str = " [\u{2750}]";
const DEFAULT_FRAME_CLOSE_TEXT: &str = " [\u{2715}] ";
// On by default, as a desktop taskbar has its start button: opening a new
// terminal or one of Zellij's own screens is a click away with nothing to
// set up. It needs the mouse, which is on by default too.
const DEFAULT_SHOW_LAUNCHER: bool = true;
// A word rather than a symbol, in the form the flags' defaults take, so the
// button needs no particular font and says what a click opens; an
// `alias_theme` swaps it for a symbol.
const DEFAULT_LAUNCHER_SYMBOL: &str = "Menu";
// Both on, so the menu is useful with nothing configured: a new terminal is
// what a launcher is most often for, and Zellij's own screens are otherwise
// reached only through session mode's keys.
const DEFAULT_LAUNCHER_NEW_TERMINAL: bool = true;
const DEFAULT_LAUNCHER_ZELLIJ_ACTIONS: bool = true;
// Empty, so a bar with nothing set draws nothing extra and asks for no
// permission until an item is chosen. The session's name and the input mode
// repeat what zjtabs and zjhints draw, and a taskbar is most often run
// beside one of them; the clock needs either `date +%z`, and so permission
// to run commands, or a `clock_utc_offset` set by hand.
const DEFAULT_TRAY: &str = "";
// One space, as between the entries, so items drawn in Zellij's ribbon
// colors read as separate blocks.
const DEFAULT_TRAY_SPACER: &str = " ";
// Hours and minutes on a 24-hour clock: the shortest time that reads
// without a date or AM and PM, and one that changes only once a minute, so
// the bar redraws for it no more often than that.
const DEFAULT_CLOCK_FORMAT: &str = "%H:%M";
// Shown, as in zjtabs: listing `session` in the tray is the user asking for
// it. While unset, Zellij's own `hide_session_name` still hides it; see
// `zjkit::mode::session_shown`.
const DEFAULT_SHOW_SESSION: bool = true;

#[derive(Default)]
pub struct State {
    /// What every call into Zellij goes through, so tests can see them. See
    /// `BarHost`.
    host: BarHost,
    /// Whether this instance runs as the bar or the settings screen. Only the
    /// bar uses the fields below.
    role: Role,
    /// What every bar keeps: its ids, its own URL and pane width, whether it
    /// is hidden, and its configuration.
    bar: BarCore,
    /// Which tabs' floating panes the bar lists.
    pane_scope: PaneScope,
    /// The format of every entry neither focused nor minimized, or `None`
    /// for Zellij's own ribbon colors.
    entry_format: Option<String>,
    /// The format of the focused entry, or `None` for Zellij's own ribbon
    /// colors.
    focused_entry_format: Option<String>,
    /// The format of a minimized entry, or `None` for Zellij's own text
    /// colors.
    minimized_entry_format: Option<String>,
    /// Drawn between neighboring entries of one tab, and between an entry
    /// and a marker.
    entry_spacer: String,
    /// Drawn between neighboring entries of two tabs, in place of the
    /// spacer.
    tab_separator: String,
    /// Drawn while the bar lists no panes, or `None` to draw nothing then.
    empty_format: Option<String>,
    /// What each indicator of `{flags}` draws as, in the order of
    /// `flags::FLAGS`. An empty one hides that indicator.
    flag_formats: [String; FLAGS.len()],
    /// The marker for the entries hidden before the ones shown, or `None`
    /// for one in Zellij's own ribbon colors.
    overflow_left_format: Option<String>,
    /// The marker for the entries hidden after the ones shown, or `None` for
    /// one in Zellij's own ribbon colors.
    overflow_right_format: Option<String>,
    /// The end of a lone entry too wide for the bar, which is cut to fit.
    overflow_str: String,
    /// Fit the entries to the bar's own pane.
    auto_width: bool,
    /// A hard cap on the line's width in columns, `0` for none.
    max_length: usize,
    /// Whether East Asian Ambiguous characters occupy two columns. Nerd Font
    /// glyphs are Ambiguous, and most terminals showing them render two.
    wide_ambiguous: bool,
    /// Draw nothing while this session is nested in another, whose own bars
    /// are on screen.
    hide_when_nested: bool,
    /// Hand this plugin's row back to the panes around it whenever there is
    /// nothing to draw on it.
    collapse_when_empty: bool,
    dim_when_unfocused: bool,
    /// How strongly to dim, in `zjkit::dim::color`'s `0.0..=1.0` scale.
    dim_strength: f32,
    /// Whether the bar takes clicks and the wheel. See `mouse`.
    mouse: bool,
    /// Swap what scrolling up and down step to.
    mouse_wheel_reversed: bool,
    /// Ask before closing a pane, from a middle click or the pane menu.
    mouse_close_confirm: bool,
    /// What the buttons on the listed panes' frames show, or `None` while
    /// `frame_buttons` is off. See `frame`.
    frame_buttons: Option<frame::FrameButtonTexts>,
    /// The buttons last placed on each pane's frame, so they are sent again
    /// only when they change.
    frame_items_sent: BTreeMap<PaneId, Vec<FrameItem>>,
    /// Draw the launcher's button, while the mouse is on. See
    /// `State::launcher_shown`.
    show_launcher: bool,
    /// The format of the launcher's button, or `None` for Zellij's own
    /// ribbon colors.
    launcher_format: Option<String>,
    /// What `{symbol}` draws in the launcher's button.
    launcher_symbol: String,
    /// List a new terminal first in the launcher's menu.
    launcher_new_terminal: bool,
    /// List Zellij's own session manager, settings and plugin manager last
    /// in the launcher's menu.
    launcher_zellij_actions: bool,
    /// The user's own entries in the launcher's menu, in order, from the
    /// `launch_<n>_*` keys. See `launcher`.
    launch_entries: Vec<launcher::LaunchEntry>,
    /// The items of the tray, in the order they are drawn. See `tray`.
    tray: Vec<TrayItem>,
    /// Drawn between neighboring items of the tray, and between the tray and
    /// what comes before it.
    tray_spacer: String,
    /// The tray's clock: its options, and what it knows of the time. See
    /// `clock`.
    clock: clock::Clock,
    /// The tray's session item's `show_session`, or `None` while it is
    /// unset, which leaves it to Zellij's `hide_session_name`. See
    /// `State::session_name`.
    show_session: Option<bool>,
    /// The format of the tray's session item, or `None` for Zellij's own
    /// ribbon colors.
    session_format: Option<String>,
    /// The format of the tray's mode item, or `None` for zjkit's built-in
    /// look. The theme's per-mode formats are read from the configuration
    /// as it is drawn; see `zjkit::mode::render_mode_prefix`.
    mode_format: Option<String>,
    /// The panes from the last `PaneUpdate`, which the entries are taken
    /// from.
    panes: PaneManifest,
    /// The tabs from the last `TabUpdate`, in the order they stand.
    tabs: Vec<Tab>,
    /// The panes the bar hid from the screen and keeps listed, so they can be
    /// brought back, each with where it stood. See `entries` for why the bar
    /// keeps them itself.
    minimized: Minimized,
    /// What the bar lists, in the order it draws them, from `panes`, `tabs`,
    /// `minimized` and `pane_scope`. See `refresh_entries`.
    entries: Vec<Entry>,
    /// The last `ModeUpdate`, for the nested-session state, and for the tray's
    /// mode and session name.
    mode_info: ModeInfo,
    /// What each column of the line last drawn stands for, for the mouse.
    click_spans: ColumnSpans<Clickable>,
    /// The pane menu, close confirmation or launcher menu being shown, if
    /// any. See `mouse::Prompted`.
    pending_prompt: PendingPrompt<mouse::Prompted>,
    /// The last left click on the bar, to tell the second click of a double
    /// click from a click of its own. See `mouse::LeftClick`.
    last_left_click: Option<mouse::LeftClick>,
}

impl State {
    /// Read every option from `user_configuration`, with the themes laid
    /// under it, as `load` does and as a live update from the settings screen
    /// does, so a value behaves the same whichever way it arrived. See
    /// `BarCore::configure`.
    ///
    /// A format that is empty counts as unset, and so as Zellij's own look,
    /// except for the spacer, the separator and the flags: there, empty is a
    /// value of its own, no gap or no indicator.
    fn apply_configuration(&mut self, user_configuration: BTreeMap<String, String>) {
        let configuration = self.bar.configure(&BAR, user_configuration);
        let text = |key: &str, default: &str| {
            configuration
                .get(key)
                .cloned()
                .unwrap_or_else(|| default.to_string())
        };
        self.pane_scope = PaneScope::of(&text(CONFIG_PANE_SCOPE, DEFAULT_PANE_SCOPE));
        self.entry_format = config::non_empty_string_option(configuration, CONFIG_ENTRY_FORMAT);
        self.focused_entry_format =
            config::non_empty_string_option(configuration, CONFIG_FOCUSED_ENTRY_FORMAT);
        self.minimized_entry_format =
            config::non_empty_string_option(configuration, CONFIG_MINIMIZED_ENTRY_FORMAT);
        self.entry_spacer = text(CONFIG_ENTRY_SPACER, DEFAULT_ENTRY_SPACER);
        self.tab_separator = text(CONFIG_TAB_SEPARATOR, DEFAULT_TAB_SEPARATOR);
        self.empty_format = config::non_empty_string_option(configuration, CONFIG_EMPTY_FORMAT);
        self.flag_formats = FLAGS.map(|flag| text(flag.option, flag.default));
        self.overflow_left_format =
            config::non_empty_string_option(configuration, CONFIG_OVERFLOW_LEFT_FORMAT);
        self.overflow_right_format =
            config::non_empty_string_option(configuration, CONFIG_OVERFLOW_RIGHT_FORMAT);
        self.overflow_str = text(CONFIG_OVERFLOW_STR, DEFAULT_OVERFLOW_STR);
        self.auto_width = config::bool_option(configuration, CONFIG_AUTO_WIDTH, DEFAULT_AUTO_WIDTH);
        self.max_length =
            config::usize_option(configuration, CONFIG_MAX_LENGTH, DEFAULT_MAX_LENGTH);
        self.wide_ambiguous = config::wide_ambiguous(configuration);
        self.hide_when_nested = config::bool_option(
            configuration,
            CONFIG_HIDE_WHEN_NESTED,
            DEFAULT_HIDE_WHEN_NESTED,
        );
        self.collapse_when_empty = config::bool_option(
            configuration,
            CONFIG_COLLAPSE_WHEN_EMPTY,
            DEFAULT_COLLAPSE_WHEN_EMPTY,
        );
        self.dim_when_unfocused = config::bool_option(
            configuration,
            CONFIG_DIM_WHEN_UNFOCUSED,
            DEFAULT_DIM_WHEN_UNFOCUSED,
        );
        // Clamped: the dim blend overshoots past neutral gray above 1.0, and
        // inverts the blend direction below 0.0.
        self.dim_strength =
            config::fraction_option(configuration, CONFIG_DIM_STRENGTH, DEFAULT_DIM_STRENGTH);
        self.mouse = config::bool_option(configuration, CONFIG_MOUSE, DEFAULT_MOUSE);
        self.mouse_wheel_reversed = config::bool_option(
            configuration,
            CONFIG_MOUSE_WHEEL_REVERSED,
            DEFAULT_MOUSE_WHEEL_REVERSED,
        );
        self.mouse_close_confirm = config::bool_option(
            configuration,
            CONFIG_MOUSE_CLOSE_CONFIRM,
            DEFAULT_MOUSE_CLOSE_CONFIRM,
        );
        self.frame_buttons =
            config::bool_option(configuration, CONFIG_FRAME_BUTTONS, DEFAULT_FRAME_BUTTONS).then(
                || frame::FrameButtonTexts {
                    minimize: text(CONFIG_FRAME_MINIMIZE_TEXT, DEFAULT_FRAME_MINIMIZE_TEXT),
                    maximize: text(CONFIG_FRAME_MAXIMIZE_TEXT, DEFAULT_FRAME_MAXIMIZE_TEXT),
                    restore: text(CONFIG_FRAME_RESTORE_TEXT, DEFAULT_FRAME_RESTORE_TEXT),
                    close: text(CONFIG_FRAME_CLOSE_TEXT, DEFAULT_FRAME_CLOSE_TEXT),
                },
            );
        self.show_launcher =
            config::bool_option(configuration, CONFIG_SHOW_LAUNCHER, DEFAULT_SHOW_LAUNCHER);
        self.launcher_format =
            config::non_empty_string_option(configuration, CONFIG_LAUNCHER_FORMAT);
        self.launcher_symbol = text(CONFIG_LAUNCHER_SYMBOL, DEFAULT_LAUNCHER_SYMBOL);
        self.launcher_new_terminal = config::bool_option(
            configuration,
            CONFIG_LAUNCHER_NEW_TERMINAL,
            DEFAULT_LAUNCHER_NEW_TERMINAL,
        );
        self.launcher_zellij_actions = config::bool_option(
            configuration,
            CONFIG_LAUNCHER_ZELLIJ_ACTIONS,
            DEFAULT_LAUNCHER_ZELLIJ_ACTIONS,
        );
        self.launch_entries = launcher::launch_entries(configuration);
        self.tray = tray::tray_items(&text(CONFIG_TRAY, DEFAULT_TRAY));
        self.tray_spacer = text(CONFIG_TRAY_SPACER, DEFAULT_TRAY_SPACER);
        // What the clock knows of the time is kept: only its options change.
        self.clock.format = text(CONFIG_CLOCK_FORMAT, DEFAULT_CLOCK_FORMAT);
        self.clock.item_format =
            config::non_empty_string_option(configuration, CONFIG_CLOCK_ITEM_FORMAT);
        self.clock.offset_source = clock::OffsetSource::of(
            config::non_empty_string_option(configuration, CONFIG_CLOCK_UTC_OFFSET).as_deref(),
        );
        self.show_session = config::explicit_bool_option(configuration, CONFIG_SHOW_SESSION);
        self.session_format = config::non_empty_string_option(configuration, CONFIG_SESSION_FORMAT);
        self.mode_format =
            config::non_empty_string_option(configuration, zjkit::mode::CONFIG_MODE_FORMAT);
        // `pane_scope` decides which panes are listed.
        self.refresh_entries();
    }

    /// The features the options as read turn on, whose permissions the bar
    /// asks for beyond what it always does. See `host::Feature`.
    fn features(&self) -> Vec<Feature> {
        let mut features = Vec::new();
        if self.mouse {
            features.push(Feature::Mouse);
        }
        if self.launcher_shown() {
            features.push(Feature::Launcher);
        }
        if self.launcher_runs_zellij_actions() {
            features.push(Feature::LauncherZellijAction);
        }
        if self.launcher_runs_commands() {
            features.push(Feature::LauncherCommand);
        }
        if self.clock_runs_command() {
            features.push(Feature::ClockCommand);
        }
        if self.frame_buttons.is_some() && self.mouse_close_confirm {
            features.push(Feature::FrameCloseConfirm);
        }
        features
    }

    /// Take the entries again from what the bar knows of the panes and the
    /// tabs. Returns whether they changed, and so whether the bar should
    /// redraw: an update that changes nothing the bar shows, such as a pane
    /// moving or a tiled pane's title, leaves them as they were.
    fn refresh_entries(&mut self) -> bool {
        let own_pane = self.bar.plugin_id.map(PaneId::Plugin);
        let entries = entries::entries_of(
            &self.panes,
            &self.tabs,
            &self.minimized,
            self.pane_scope,
            own_pane,
        );
        let changed = entries != self.entries;
        self.entries = entries;
        changed
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
        self.mouse
    }

    /// The permissions of the features the options turn on, which `mouse`
    /// alone cannot tell.
    fn permissions(&self, _spec: &BarSpec) -> Vec<PermissionType> {
        bar_permissions(&self.features())
    }

    fn apply_configuration(&mut self, user_configuration: BTreeMap<String, String>) {
        State::apply_configuration(self, user_configuration);
    }
}

/// Each slot of the shared instance is a `State` of its own; only a bar has
/// the bar's state for `zjkit::shared` to read.
impl zjkit::shared::SlotPlugin for State {
    fn bar(&self) -> Option<&BarCore> {
        self.role.is_bar().then_some(&self.bar)
    }

    fn bar_mut(&mut self) -> Option<&mut BarCore> {
        self.role.is_bar().then_some(&mut self.bar)
    }
}

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.role = Role::load(&BAR, &configuration);
        if !self.role.is_bar() {
            return;
        }
        self.apply_configuration(configuration);
        let permissions = BarPlugin::permissions(self, &BAR);
        self.bar
            .load(&self.host, &BAR, &permissions, BarPlugin::mouse(self));
        // Arms the clock's first timer. Its offset waits for the permission.
        self.follow_clock();
    }

    fn update(&mut self, event: Event) -> bool {
        if !self.role.is_bar() {
            return self.role.update(event);
        }
        let handled = self.bar.update(&self.host, &BAR, &event);
        let mut should_render = BarCore::follow(self, &BAR, handled);
        // Which frames carry the bar's buttons follows the panes, the tabs
        // and the permissions. Not a click: one that restores a pane leaves
        // it hidden and no longer minimized until Zellij reports it shown,
        // which would take its buttons off in between.
        let frames_may_change = matches!(
            event,
            Event::PaneUpdate(_) | Event::TabUpdate(_) | Event::PermissionRequestResult(_)
        );
        match event {
            Event::PaneUpdate(manifest) => {
                entries::forget_gone(&mut self.minimized, &manifest);
                self.panes = manifest;
                should_render |= self.refresh_entries();
            }
            Event::TabUpdate(infos) => {
                self.tabs = entries::tabs_of(&infos);
                should_render |= self.refresh_entries();
            }
            // A mode update arrives on every mode change and more, but the
            // bar shows only how nesting hides or dims it, and the mode and
            // the session's name while they are in the tray. The Zellij theme
            // it also carries is `BarCore`'s to follow.
            Event::ModeUpdate(mode_info) => {
                if self.mode_view(&mode_info) != self.mode_view(&self.mode_info) {
                    should_render = true;
                }
                self.mode_info = mode_info;
            }
            // `BarCore` has made the pane unselectable and, on a grant, added
            // the settings entry. The clock's offset may wait on it.
            Event::PermissionRequestResult(status) => self.clock_permission_answered(status),
            Event::Timer(_) => self.clock_timer_fired(),
            Event::RunCommandResult(exit_code, stdout, _, context) => {
                self.answer_clock_command(exit_code, &stdout, &context)
            }
            // What a click does comes back as a `PaneUpdate`, so the mouse
            // never redraws by itself.
            Event::Mouse(mouse) if self.mouse => self.follow_mouse(&mouse, Instant::now()),
            Event::PromptResult(request_id, result) => self.answer_prompt(request_id, &result),
            Event::FrameItemClicked(click) => self.frame_button_clicked(&click),
            _ => {}
        }
        if frames_may_change {
            self.sync_frame_buttons();
        }
        // Whatever came, the clock may have a new option, offset or tick.
        should_render |= self.follow_clock();
        should_render
    }

    fn pipe(&mut self, message: PipeMessage) -> bool {
        if !self.role.is_bar() {
            self.role.pipe(&self.host, &BAR, &message);
            return false;
        }
        match self.bar.pipe(&self.host, &BAR, &message) {
            // A preview from the settings screen may change the tray.
            Some(handled) => {
                let should_render = BarCore::follow(self, &BAR, handled) | self.follow_clock();
                self.sync_frame_buttons();
                should_render
            }
            None if message.name == frame::PULLED_MESSAGE => {
                self.pane_pulled(message.payload.as_deref());
                false
            }
            None => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if !self.role.is_bar() {
            self.role.render(rows, cols);
            return;
        }
        let (output, click_spans) = self.draw_bar();
        self.bar
            .sync_collapsed(&self.host, self.collapse_when_empty, output.is_empty());
        self.click_spans = click_spans;
        print!("{output}");
    }
}

/// The line zjtaskbar draws for the panes of `manifest` in `tabs`, in a bar
/// `columns` wide, with `configuration` as its options and `mode_info` as
/// the last `ModeUpdate` Zellij sent it: the ANSI the plugin prints, drawn
/// by the same code.
///
/// For the docs images. The bar is set up as the plugin is once Zellij has
/// reported its theme, its pane's width, the panes and the tabs, without
/// asking Zellij for anything. A bar that `mode_info` hides, as
/// `hide_when_nested` hides a nested session's, draws an empty line.
///
/// A pane of `manifest` reported hidden, suppressed and not floating, is
/// taken as one the bar minimized itself, so an image can show a minimized
/// entry without the clicks that would make one.
///
/// A clock in the tray shows the settings screen's sample time,
/// `clock::SAMPLE_UNIX_SECONDS`, at `clock_utc_offset` or else at UTC, so an
/// image comes out the same whenever it is drawn.
#[cfg(any(test, feature = "images"))]
pub fn draw_for_image(
    configuration: BTreeMap<String, String>,
    mode_info: ModeInfo,
    tabs: &[TabInfo],
    manifest: PaneManifest,
    columns: usize,
) -> String {
    let mut state = State::default();
    // Set before the options, as `color_theme zellij` draws in its colors.
    state.bar.zellij_palette = Some(mode_info.style.colors);
    state.bar.pane_width = Some(columns);
    state.mode_info = mode_info;
    state.tabs = entries::tabs_of(tabs);
    for pane in manifest.panes.values().flatten() {
        if pane.is_suppressed && !pane.is_floating {
            let pane_id = zjkit::panes::pane_id_of(pane);
            state.minimized.insert(pane_id, Default::default());
        }
    }
    state.panes = manifest;
    // Reads the options and takes the entries from the panes and tabs.
    state.apply_configuration(configuration);
    state.clock.text = state.clock.sample_text();
    state.draw_bar().0
}

#[cfg(test)]
mod tests;
