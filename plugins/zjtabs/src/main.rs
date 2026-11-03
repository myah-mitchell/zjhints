//! zjtabs: a tab bar for Zellij, drawn from format strings in the same style
//! as zjhints, and sharing its themes, its settings screen and the bar
//! plumbing in `zjkit`.

use std::collections::BTreeMap;
use zellij_tile::prelude::*;
use zjkit::bar::{BarCore, BarPlugin, BarSpec, Role};
use zjkit::config;
use zjkit::mode::CONFIG_MODE_FORMAT;
use zjkit::mouse::ColumnSpans;

mod flags;
mod host;
#[cfg(test)]
mod images;
mod mouse;
mod settings;
mod tabs;
mod theme;

use flags::FLAGS;
use host::*;
use tabs::{Clickable, ModeView, Tab};

const CONFIG_TAB_FORMAT: &str = "tab_format";
const CONFIG_ACTIVE_TAB_FORMAT: &str = "active_tab_format";
const CONFIG_TAB_SPACER: &str = "tab_spacer";
const CONFIG_FULLSCREEN_FORMAT: &str = "fullscreen_format";
const CONFIG_SYNC_FORMAT: &str = "sync_format";
const CONFIG_FLOATING_FORMAT: &str = "floating_format";
const CONFIG_BELL_FORMAT: &str = "bell_format";
const CONFIG_OVERFLOW_LEFT_FORMAT: &str = "overflow_left_format";
const CONFIG_OVERFLOW_RIGHT_FORMAT: &str = "overflow_right_format";
const CONFIG_INDEX_FROM_ZERO: &str = "index_from_zero";
const CONFIG_RENAME_PLACEHOLDER: &str = "rename_placeholder";
const CONFIG_OVERFLOW_STR: &str = "overflow_str";
const CONFIG_AUTO_WIDTH: &str = "auto_width";
const CONFIG_MAX_LENGTH: &str = "max_length";
const CONFIG_HIDE_WHEN_NESTED: &str = "hide_when_nested";
const CONFIG_COLLAPSE_WHEN_EMPTY: &str = "collapse_when_empty";
const CONFIG_DIM_WHEN_UNFOCUSED: &str = "dim_when_unfocused";
const CONFIG_DIM_STRENGTH: &str = "dim_strength";
const CONFIG_SHOW_MODE: &str = "show_mode";
const CONFIG_MOUSE: &str = "mouse";
const CONFIG_MOUSE_WHEEL_REVERSED: &str = "mouse_wheel_reversed";
const CONFIG_MOUSE_CLOSE_CONFIRM: &str = "mouse_close_confirm";

// One space, so neighboring tabs drawn in Zellij's ribbon colors read as
// separate blocks rather than one long ribbon.
const DEFAULT_TAB_SPACER: &str = " ";
// Zellij's own wording for the two states its built-in tab bar shows after a
// tab's name, and the same form for floating panes, which it does not show.
// Words rather than symbols, so the default needs no particular font; an
// `alias_theme` swaps them for symbols.
const DEFAULT_FULLSCREEN_FORMAT: &str = " (FULLSCREEN)";
const DEFAULT_SYNC_FORMAT: &str = " (SYNC)";
const DEFAULT_FLOATING_FORMAT: &str = " (FLOATING)";
// The dot Zellij's own tab bar marks a tab that needs the user with.
const DEFAULT_BELL_FORMAT: &str = " \u{25cf}";
// Off by default: Zellij numbers tabs from 1 everywhere else, from the
// `GoToTab` keybindings to the names it gives new tabs.
const DEFAULT_INDEX_FROM_ZERO: bool = false;
// What Zellij's own tab bar shows while a tab's new name is still empty, so
// the tab being renamed does not shrink to nothing.
const DEFAULT_RENAME_PLACEHOLDER: &str = "Enter name...";
// The same as zjhints, so the two bars cut the same way.
const DEFAULT_OVERFLOW_STR: &str = "...";
// On by default: the bar has a pane of its own, and fitting it is what keeps
// the active tab on screen as the window narrows.
const DEFAULT_AUTO_WIDTH: bool = true;
// No cap of its own; `auto_width` already fits the pane.
const DEFAULT_MAX_LENGTH: usize = 0;
// A nested session gets no tab bar by default, since the host's is drawn just
// beside it (see `zjkit::nesting::is_nested`). The same as zjhints.
const DEFAULT_HIDE_WHEN_NESTED: bool = true;
// Hand the row back to the panes around it while there is nothing on it,
// rather than leaving a blank one. The same as zjhints.
const DEFAULT_COLLAPSE_WHEN_EMPTY: bool = true;
// Fade the bar while this session is not the one receiving input. The same
// as zjhints, so a shared layout's bars dim in step.
const DEFAULT_DIM_WHEN_UNFOCUSED: bool = true;
const DEFAULT_DIM_STRENGTH: f32 = 0.5;
// On by default, unlike zjhints: zjhints' own `show_mode` is off by default,
// so with both bars at their defaults the mode shows once, on the tab bar.
const DEFAULT_SHOW_MODE: bool = true;
// On by default, unlike zjhints: clicking a tab to go to it is what a tab bar
// is expected to do, and Zellij's own tab bar does it.
const DEFAULT_MOUSE: bool = true;
// Off by default, so the wheel does what it does in Zellij's own tab bar,
// which goes to the next tab on scroll up and the previous one on scroll
// down. Turning it on swaps the two for anyone who expects the wheel to
// follow the tabs' order left to right the other way around.
const DEFAULT_MOUSE_WHEEL_REVERSED: bool = false;
// On by default: closing a tab takes every pane in it with it, and the
// processes running in them, and a middle click is easy to make by accident,
// such as by pressing the wheel to scroll.
const DEFAULT_MOUSE_CLOSE_CONFIRM: bool = true;

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
    /// The format of every tab but the active one, or `None` for Zellij's own
    /// ribbon colors.
    tab_format: Option<String>,
    /// The format of the active tab, or `None` for Zellij's own ribbon
    /// colors.
    active_tab_format: Option<String>,
    /// Drawn between neighboring tabs, and between a tab and a marker.
    tab_spacer: String,
    /// What each indicator of `{flags}` draws as, in the order of
    /// `flags::FLAGS`. An empty one hides that indicator.
    flag_formats: [String; FLAGS.len()],
    /// The marker for the tabs hidden before the ones shown, or `None` for
    /// one in Zellij's own ribbon colors.
    overflow_left_format: Option<String>,
    /// The marker for the tabs hidden after the ones shown, or `None` for one
    /// in Zellij's own ribbon colors.
    overflow_right_format: Option<String>,
    /// Number tabs from 0 in `{index}`, rather than from 1.
    index_from_zero: bool,
    /// The name drawn for the tab being renamed while its new name is empty.
    rename_placeholder: String,
    /// The end of a lone tab too wide for the bar, which is cut to fit.
    overflow_str: String,
    /// Fit the tabs to the bar's own pane.
    auto_width: bool,
    /// A hard cap on the line's width in columns, `0` for none.
    max_length: usize,
    /// Whether East Asian Ambiguous characters occupy two columns. Nerd Font
    /// glyphs are Ambiguous, and most terminals showing them render two.
    wide_ambiguous: bool,
    /// Draw nothing while this session is nested in another, whose own tab
    /// bar is on screen.
    hide_when_nested: bool,
    /// Hand this plugin's row back to the panes around it whenever there is
    /// nothing to draw on it.
    collapse_when_empty: bool,
    dim_when_unfocused: bool,
    /// How strongly to dim, in `zjkit::dim::color`'s `0.0..=1.0` scale.
    dim_strength: f32,
    /// Draw the current input mode at the start of the line, before the tabs,
    /// so the bar can show the mode with no zjhints or zjstatus beside it.
    /// See `zjkit::mode::render_mode_prefix`.
    show_mode: bool,
    /// The mode prefix's format string for every mode a `mode_format_<mode>`
    /// does not cover, or `None` for a block in the colors of Zellij's
    /// selected ribbon.
    mode_format: Option<String>,
    /// Switch, rename, close and open the menu of tabs with the mouse.
    mouse: bool,
    /// Swap the wheel's two directions: scroll up goes to the previous tab
    /// and scroll down to the next, rather than the other way around.
    mouse_wheel_reversed: bool,
    /// Ask before a middle click closes a tab, rather than closing it at once.
    mouse_close_confirm: bool,
    /// The close confirmation being shown, if any. See `mouse::PendingClose`.
    pending_close: Option<mouse::PendingClose>,
    /// The tabs from the last `TabUpdate`, in the order they stand.
    tabs: Vec<Tab>,
    /// The input mode and nested-session state from the last `ModeUpdate`.
    mode_info: ModeInfo,
    /// Which columns of the line last drawn stand for which tab, marker or
    /// the mode prefix.
    click_spans: ColumnSpans<Clickable>,
}

register_plugin!(State);

impl State {
    /// Read every option from `user_configuration`, with the themes laid
    /// under it, as `load` does and as a live update from the settings screen
    /// does, so a value behaves the same whichever way it arrived. See
    /// `BarCore::configure`.
    ///
    /// A format that is empty counts as unset, and so as Zellij's own look,
    /// except for `tab_spacer` and the flags: there, empty is a value of its
    /// own, no gap between tabs or no indicator.
    fn apply_configuration(&mut self, user_configuration: BTreeMap<String, String>) {
        let configuration = self.bar.configure(&BAR, user_configuration);
        let text = |key: &str, default: &str| {
            configuration
                .get(key)
                .cloned()
                .unwrap_or_else(|| default.to_string())
        };
        self.tab_format = config::non_empty_string_option(configuration, CONFIG_TAB_FORMAT);
        self.active_tab_format =
            config::non_empty_string_option(configuration, CONFIG_ACTIVE_TAB_FORMAT);
        self.tab_spacer = text(CONFIG_TAB_SPACER, DEFAULT_TAB_SPACER);
        self.flag_formats = FLAGS.map(|flag| text(flag.option, flag.default));
        self.overflow_left_format =
            config::non_empty_string_option(configuration, CONFIG_OVERFLOW_LEFT_FORMAT);
        self.overflow_right_format =
            config::non_empty_string_option(configuration, CONFIG_OVERFLOW_RIGHT_FORMAT);
        self.index_from_zero = config::bool_option(
            configuration,
            CONFIG_INDEX_FROM_ZERO,
            DEFAULT_INDEX_FROM_ZERO,
        );
        self.rename_placeholder = text(CONFIG_RENAME_PLACEHOLDER, DEFAULT_RENAME_PLACEHOLDER);
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
        self.show_mode = config::bool_option(configuration, CONFIG_SHOW_MODE, DEFAULT_SHOW_MODE);
        self.mode_format = config::non_empty_string_option(configuration, CONFIG_MODE_FORMAT);
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

    /// With `mouse_close_confirm` as well as `mouse`, which `BarSpec` alone
    /// cannot tell.
    fn permissions(&self, _spec: &BarSpec) -> Vec<PermissionType> {
        bar_permissions(self.mouse, self.mouse_close_confirm)
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
        let permissions = BarPlugin::permissions(self, &BAR);
        self.bar.load(&self.host, &BAR, &permissions, self.mouse);
    }

    fn update(&mut self, event: Event) -> bool {
        if !self.role.is_bar() {
            return self.role.update(event);
        }
        let handled = self.bar.update(&self.host, &BAR, &event);
        let mut should_render = BarCore::follow(self, &BAR, handled);
        match event {
            Event::TabUpdate(infos) => {
                let tabs = tabs::tabs_of(&infos);
                if tabs != self.tabs {
                    self.tabs = tabs;
                    should_render = true;
                }
            }
            // A mode update arrives on every mode change and more, but the
            // bar shows only whether a tab is being renamed, how nesting
            // hides or dims it, and the mode itself while `show_mode` is on.
            // The Zellij theme it also carries is `BarCore`'s to follow.
            Event::ModeUpdate(mode_info) => {
                let view = |mode_info: &ModeInfo| ModeView::of(mode_info, self.show_mode);
                if view(&mode_info) != view(&self.mode_info) {
                    should_render = true;
                }
                self.mode_info = mode_info;
            }
            // Turning `mouse` off cannot take back the subscription, so the
            // events keep coming and are left alone here.
            Event::Mouse(mouse) if self.mouse => self.follow_mouse(&mouse),
            // Taken whatever `mouse` and `mouse_close_confirm` are now: the
            // user was asked while they were on, and answered.
            Event::PromptResult(request_id, result) => self.answer_close(request_id, &result),
            _ => {}
        }
        should_render
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
            return;
        }
        let (output, click_spans) = self.draw_bar();
        self.bar
            .sync_collapsed(&self.host, self.collapse_when_empty, output.is_empty());
        print!("{output}");
        self.click_spans = click_spans;
    }
}

#[cfg(test)]
mod tests;
