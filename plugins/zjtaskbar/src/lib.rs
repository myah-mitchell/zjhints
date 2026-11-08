//! zjtaskbar: a taskbar for Zellij's floating panes, drawn from format
//! strings in the same style as zjhints and zjtabs, and sharing their
//! themes, their settings screen and the bar plumbing in `zjkit`.
//!
//! The bar lists the session's floating panes as entries, grouped by tab,
//! with the focused one highlighted and the minimized ones marked. See
//! `entries` for which panes it lists and in what order, `draw` for how it
//! draws them, and `mouse` for what clicking them does.
//!
//! The plugin is this library's `State`, which `main.rs` registers with
//! Zellij.

use std::cell::{Ref, RefMut};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;
use zellij_tile::prelude::*;
use zjkit::bar::{BarCore, BarPlugin, BarSpec, Role};
use zjkit::config;
use zjkit::mouse::{ColumnSpans, PendingPrompt};

mod draw;
mod entries;
mod flags;
mod host;
mod instance;
mod mouse;
mod settings;
mod theme;

use draw::Clickable;
use entries::{Entry, Minimized, PaneScope, Tab};
use flags::FLAGS;
use host::*;
use instance::Instance;

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
// None: `tab_separator` already sets each tab's panes apart, so naming the
// tab on every entry outside the active one only takes room from the
// titles. An alias theme gives it a symbol, before the tab's name.
const DEFAULT_OTHER_TAB_FORMAT: &str = "";
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

#[derive(Default)]
pub struct State {
    /// What every call into Zellij goes through, so tests can see them. See
    /// `BarHost`.
    host: BarHost,
    /// What every slot of the instance shares: the minimized panes and where
    /// the bars are. See `instance`.
    instance: Instance,
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
    /// The panes from the last `PaneUpdate`, which the entries are taken
    /// from.
    panes: PaneManifest,
    /// The tabs from the last `TabUpdate`, in the order they stand.
    tabs: Vec<Tab>,
    /// What the bar lists, in the order it draws them, from `panes`, `tabs`,
    /// the instance's minimized panes and `pane_scope`. See
    /// `refresh_entries`.
    entries: Vec<Entry>,
    /// The last `ModeUpdate`, for the nested-session state.
    mode_info: ModeInfo,
    /// What each column of the line last drawn stands for, for the mouse.
    click_spans: ColumnSpans<Clickable>,
    /// The pane menu or close confirmation being shown, if any. See `mouse::Prompted`.
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
        features
    }

    /// Take the entries again from what the bar knows of the panes and the
    /// tabs. Returns whether they changed, and so whether the bar should
    /// redraw: an update that changes nothing the bar shows, such as a pane
    /// moving or a tiled pane's title, leaves them as they were.
    fn refresh_entries(&mut self) -> bool {
        let entries = entries::entries_of(
            &self.panes,
            &self.tabs,
            &self.minimized(),
            self.pane_scope,
            &self.bar_ids(),
        );
        let changed = entries != self.entries;
        self.entries = entries;
        changed
    }

    /// The panes minimized from any bar of the instance, each with where it
    /// stood, so they can be brought back. See `entries` for why the bars
    /// keep them themselves.
    fn minimized(&self) -> Ref<'_, Minimized> {
        Ref::map(self.instance.get(), |common| &common.minimized)
    }

    /// The same, to change.
    fn minimized_mut(&self) -> RefMut<'_, Minimized> {
        RefMut::map(self.instance.get_mut(), |common| &mut common.minimized)
    }

    /// The plugin ids of the instance's bars, which are their panes' ids:
    /// this one's and every other slot's running the bar.
    fn bar_ids(&self) -> BTreeSet<u32> {
        let mut bars = self.instance.get().bars.clone();
        bars.extend(self.bar.plugin_id);
        bars
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
        if let Some(id) = self.bar.plugin_id {
            self.instance.get_mut().bars.insert(id);
        }
    }

    fn update(&mut self, event: Event) -> bool {
        if !self.role.is_bar() {
            return self.role.update(event);
        }
        // The slot is going, as `zjkit::shared` says with this, so the other
        // bars no longer leave anything to it.
        if let Event::BeforeClose = event {
            if let Some(id) = self.bar.plugin_id {
                self.instance.get_mut().bars.remove(&id);
            }
            return false;
        }
        let handled = self.bar.update(&self.host, &BAR, &event);
        let mut should_render = BarCore::follow(self, &BAR, handled);
        match event {
            Event::PaneUpdate(manifest) => {
                entries::forget_gone(&mut self.minimized_mut(), &manifest);
                self.panes = manifest;
                should_render |= self.refresh_entries();
            }
            Event::TabUpdate(infos) => {
                self.tabs = entries::tabs_of(&infos);
                should_render |= self.refresh_entries();
            }
            // A mode update arrives on every mode change and more, but the
            // bar shows only how nesting hides or dims it. The Zellij theme it
            // also carries is `BarCore`'s to follow.
            Event::ModeUpdate(mode_info) => {
                if self.mode_view(&mode_info) != self.mode_view(&self.mode_info) {
                    should_render = true;
                }
                self.mode_info = mode_info;
            }
            // What a click does comes back as a `PaneUpdate`, so the mouse
            // never redraws by itself.
            Event::Mouse(mouse) if self.mouse => self.follow_mouse(&mouse, Instant::now()),
            Event::PromptResult(request_id, result) => self.answer_prompt(request_id, &result),
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
        // The size Zellij is about to draw the pane at, which a pane update
        // may not have reported yet. See `BarCore::edge`.
        self.bar.render_cols = Some(cols);
        let (output, click_spans) = self.draw_bar();
        self.bar
            .sync_collapsed(&self.host, self.collapse_when_empty, output.is_empty());
        self.click_spans = click_spans;
        print!("{output}");
    }
}

#[cfg(test)]
mod tests;
