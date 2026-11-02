use ansi_term::ANSIStrings;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;
use zjkit::ansi::{calculate_visible_length, truncate_ansi_string, ANSI_RESET};
use zjkit::bar::{BarCore, Handled, Role};
use zjkit::config;
use zjkit::fit::{Group, Precedence};
use zjkit::format;
use zjkit::host::Host;
use zjkit::mouse;
use zjkit::nesting;
use zjkit::panes::{focus_by_tab, pane_id_of, terminal_width, ActiveTab, TabFocus};

mod hints;
mod host;
#[cfg(test)]
mod images;
mod keypress;
mod keys;
mod mode_prefix;
mod popup;
mod settings;
mod theme;

use mouse::{
    ActionQueue, Choice, ClickMap, Cut, Item, LineSegment, MenuEntry, MouseOutcome, Preview,
    PreviewMemory, PreviewStep, SegmentContent,
};

use hints::*;
use host::*;
use keys::*;
use mode_prefix::*;

#[derive(Default)]
struct State {
    /// What every call into Zellij goes through, so tests can see them. See
    /// `BarHost`.
    host: BarHost,
    /// Whether this instance runs as the bar, the settings screen or the
    /// popup of a hover preview. Only the bar uses the fields below.
    role: Role,
    /// What every bar keeps: its ids, its own URL and pane width, whether it
    /// is hidden, and its configuration. A bar loaded to feed zjstatus has a
    /// hidden pane that reports the bar's own URL too.
    bar: BarCore,
    initialized: bool,
    /// The zjstatus pipe names each render is published on. See
    /// [`pipe_names_from_config`].
    pipe_names: Vec<String>,
    mode_info: ModeInfo,
    base_mode_is_locked: bool,
    /// Whether Zellij delivers this session's keybindings through
    /// `InitialKeybinds`. Once it does, every `ModeUpdate` arrives with an
    /// empty table and the one already held has to be carried over. A Zellij
    /// too old to know the event keeps attaching them to `ModeUpdate`, and
    /// this stays false.
    keybinds_arrive_separately: bool,
    max_length: usize,
    auto_width: bool,
    reserve_columns: usize,
    /// Whether East Asian Ambiguous characters occupy two columns. Nerd Font
    /// glyphs are Ambiguous, and most terminals showing them render two.
    wide_ambiguous: bool,
    /// Terminal width in columns, learned from `PaneUpdate`. `None` until the
    /// first one arrives, when auto-fitting stays off rather than guessing.
    terminal_width: Option<usize>,
    overflow_str: String,
    hide_in_base_mode: bool,
    key_format: Option<String>,
    desc_format: Option<String>,
    hint_spacer: Option<String>,
    drop_indicator: Option<String>,
    discover_hints: bool,
    direction_keys: DirectionKeys,
    key_order: KeyOrder,
    hint_order: HintOrder,
    hint_precedence: Precedence,
    hide_shared_hints: bool,
    /// Render nothing when this session is nested (see `is_nested`), so a
    /// shared layout gives nested sessions no bottom bar without a second
    /// layout file.
    hide_when_nested: bool,
    /// Hand this plugin's row back to the panes around it whenever there is
    /// nothing to draw on it, so a layout that reserves a row for hints does
    /// not leave a blank one behind when they are hidden. Turn it off to keep
    /// the row reserved no matter what.
    collapse_when_empty: bool,
    dim_when_unfocused: bool,
    /// How strongly to dim, in `zjkit::dim::color`'s `0.0..=1.0` scale.
    dim_strength: f32,
    /// Prefix the hints line with the current mode, so this plugin can stand
    /// on its own in a pane without needing zjstatus's own `{mode}` widget
    /// alongside it. See `render_mode_prefix`.
    show_mode: bool,
    mode_format: Option<String>,
    /// What each nested session hosted in one of this session's panes reports
    /// about itself, keyed by the pane it runs in so several of them never
    /// overwrite each other. Populated from `NestedSessionModeUpdate` and
    /// the keybindings `fetch_keybinds` returns; read back by
    /// `descended_guest` while this session is descended into one of them.
    nested_guests: BTreeMap<PaneId, NestedGuest>,
    /// How a nested session's keybindings are fetched. Always
    /// `get_nested_session_keybinds` in the plugin; tests swap in a fake,
    /// because the real one blocks on a reply from the host.
    fetch_keybinds: KeybindsFetcher,
    /// Which pane holds the focus in each tab, learned from `PaneUpdate`.
    focus_by_tab: BTreeMap<usize, TabFocus>,
    /// The focused tab, learned from `TabUpdate`. `None` until the first one
    /// arrives.
    active_tab: Option<ActiveTab>,
    /// Make the bar clickable: right-click for Zellij's context menu,
    /// left-click to run a hint, and a menu of the hints that did not fit.
    /// Off by default, since it asks for permission to run actions.
    mouse: bool,
    /// With `mouse` on, also make a descended-into nested session's hints and
    /// the way back out of it clickable, by typing their keys into the pane
    /// that session runs in.
    mouse_nested: bool,
    /// With `mouse` on, preview a hint's menu while the pointer rests on it.
    mouse_hover: bool,
    /// With `mouse` on, open a hint's menu when it is clicked.
    mouse_menus: bool,
    /// Which columns of the line last drawn hold which hint.
    click_map: ClickMap,
    /// Menus shown and not yet answered, by prompt request id.
    pending_menus: BTreeMap<u64, Vec<MenuEntry>>,
    /// Clicked actions not yet run, started one at a time.
    action_queue: ActionQueue,
    /// The hover preview waiting to open or open over the bar, if any.
    preview_memory: PreviewMemory,
    /// How many rows the bar's pane had when it was last drawn, which is
    /// where a hover preview starts below it.
    pane_rows: usize,
}

/// What clicking the descended-into indicator does: press the nested
/// session's ascend keys, in order, in the pane it runs in.
fn descended_indicator_target(mode_info: &ModeInfo, config: &BTreeMap<String, String>) -> Item {
    let label = menu_label(
        &compose_key_text(&mode_info.nested_ascend_keys, config),
        DESCENDED_HINT_LABEL,
    );
    Item {
        label: label.clone(),
        choices: vec![Choice {
            label,
            actions: keypress::press_keys(&mode_info.nested_ascend_keys),
        }],
    }
}

/// What a nested session running in one of this session's panes has told us
/// about itself.
///
/// `mode` and `base_mode` arrive unprompted, once when the nested session first
/// makes contact and again on every mode change it makes, so they are current
/// by the time the user descends into it. `keybinds` is the expensive half and
/// is fetched on request. Every report carries a `keybinds_generation` that
/// changes whenever the bindings behind it may have, including when a nested
/// session that is itself descended moves between its own bindings and a
/// deeper session's. `fetched_generation` is the generation last fetched (or
/// last tried), so a fetch happens once per generation rather than on every
/// event or render.
#[derive(Default, Clone, PartialEq)]
struct NestedGuest {
    /// The chain of sessions this report describes, outermost first: the
    /// nested session in the pane, then whichever session it is descended
    /// into, and so on down. The last one is whose mode and bindings these are.
    session_path: Vec<String>,
    mode: InputMode,
    base_mode: Option<InputMode>,
    keybinds: KeybindsVec,
    fetched_generation: Option<u64>,
}

/// The function `State` fetches a nested session's keybindings with. A wrapper
/// only so `State` can keep deriving `Default`.
#[derive(Clone, Copy)]
struct KeybindsFetcher(fn(PaneId) -> NestedSessionKeybindsResponse);

impl Default for KeybindsFetcher {
    fn default() -> Self {
        KeybindsFetcher(get_nested_session_keybinds)
    }
}

register_plugin!(State);

const TO_NORMAL: Action = Action::SwitchToMode {
    input_mode: InputMode::Normal,
};

/// Plugins with a curated Session-mode hint, as `(plugin name, id, label)`.
///
/// Every entry here is launched by a `LaunchOrFocusPlugin` binding, and those all
/// share one action signature — so without a curated id they would discover as a
/// single fused hint carrying every launcher key. Listing them gives each its own
/// concept id and a label worth reading.
const SESSION_PLUGINS: &[(&str, &str, &str)] = &[
    ("session-manager", "manager", "manager"),
    ("configuration", "config", "config"),
    ("plugin-manager", "plugins", "plugins"),
    ("zellij:about", "about", "about"),
    ("zellij:share", "share", "share"),
    ("zellij:layout-manager", "layout_manager", "layouts"),
];

const DEFAULT_MAX_LENGTH: usize = 0;
const DEFAULT_OVERFLOW_STR: &str = "...";
const DEFAULT_PIPE_NAME: &str = "zjhints";
/// The name this plugin published on before it was renamed. Still published
/// to alongside the current default, so a zjstatus config written against
/// `{pipe_zjstatus_hints}` keeps rendering without being touched.
const LEGACY_PIPE_NAME: &str = "zjstatus_hints";

const CONFIG_KEY_FORMAT: &str = "key_format";
const CONFIG_DESC_FORMAT: &str = "desc_format";
const CONFIG_HINT_SPACER: &str = "hint_spacer";
const CONFIG_KEY_ALIAS_PREFIX: &str = "key_alias_";
const CONFIG_MOD_ALIAS_PREFIX: &str = "mod_alias_";
const CONFIG_CHORD_MODS_PREFIX: &str = "chord_mods_";
const CONFIG_CHORD_ALIAS_PREFIX: &str = "chord_alias_";
const CONFIG_LABEL_PREFIX: &str = "label_";
// Per-hint overrides of the global styling, keyed the same way labels are.
const CONFIG_KEY_FORMAT_PREFIX: &str = "key_format_";
const CONFIG_DESC_FORMAT_PREFIX: &str = "desc_format_";
const CONFIG_KEYS_PREFIX: &str = "keys_";
const CONFIG_DISCOVER_HINTS: &str = "discover_hints";
const CONFIG_DIRECTION_KEYS: &str = "direction_keys";
const CONFIG_KEY_ORDER: &str = "key_order";
const CONFIG_HINT_ORDER: &str = "hint_order";
const CONFIG_DROP_INDICATOR: &str = "drop_indicator";
const CONFIG_OVERFLOW_STR: &str = "overflow_str";
const CONFIG_HINT_PRECEDENCE: &str = "hint_precedence";
const CONFIG_AUTO_WIDTH: &str = "auto_width";
const CONFIG_RESERVE_COLUMNS: &str = "reserve_columns";
const CONFIG_HIDE_SHARED_HINTS: &str = "hide_shared_hints";
const CONFIG_HIDE_WHEN_NESTED: &str = "hide_when_nested";
const CONFIG_COLLAPSE_WHEN_EMPTY: &str = "collapse_when_empty";
const CONFIG_DIM_WHEN_UNFOCUSED: &str = "dim_when_unfocused";
const CONFIG_DIM_STRENGTH: &str = "dim_strength";
const CONFIG_SHOW_MODE: &str = "show_mode";
const CONFIG_MODE_FORMAT: &str = "mode_format";
const CONFIG_MOUSE: &str = "mouse";
const CONFIG_MOUSE_NESTED: &str = "mouse_nested";
const CONFIG_MOUSE_HOVER: &str = "mouse_hover";
const CONFIG_MOUSE_MENUS: &str = "mouse_menus";
// Per-mode override of the mode prefix, e.g. `mode_format_normal`. Matches
// zjstatus's own `mode_<suffix>` naming exactly (see `mode_config_suffix`),
// so an icon set up for zjstatus's {mode} widget can be reused here.
const CONFIG_MODE_FORMAT_PREFIX: &str = "mode_format_";
const CONFIG_MODE_LABEL_PREFIX: &str = "mode_label_";
const CONFIG_MODE_KEY_PREFIX: &str = "mode_key_";

/// Concept id of the synthetic hint standing in for the descended-session
/// placeholder (see `render`'s `is_host_descended` branch), and its default
/// label. Rendering it as an ordinary `Hint` through `render_hint` means
/// `key_format`/`desc_format`/`label_descended`/`key_format_descended`/
/// `desc_format_descended`/`keys_descended` all apply to it exactly as they
/// would to any other hint, including key and modifier aliases, with no
/// bespoke styling path of its own to keep in sync.
const DESCENDED_HINT_ID: &str = "descended";
const DESCENDED_HINT_LABEL: &str = "return to host";

// Discovery is on by default, so the bar names every binding the mode accepts
// rather than only the ones the curated list knows about. The curated list
// still runs first and supplies the order, the grouped concepts and the
// hand-written labels; turning discovery off leaves that alone, which is the
// shorter and more readable bar.
const DEFAULT_DISCOVER_HINTS: bool = true;
const DEFAULT_HIDE_SHARED_HINTS: bool = true;
// A nested session gets no bottom bar by default, so hints for it show up in
// the host's bar instead (see `is_nested`) rather than doubling up.
const DEFAULT_HIDE_WHEN_NESTED: bool = true;
const DEFAULT_COLLAPSE_WHEN_EMPTY: bool = true;
const DEFAULT_DIM_WHEN_UNFOCUSED: bool = true;
// Matches the zjstatus fork's default, so a shared layout's two bars dim in
// step. See that fork's `dim_color` for the same blend-toward-gray formula.
const DEFAULT_DIM_STRENGTH: f32 = 0.5;
// Off by default: most setups pair this plugin with zjstatus's own {mode}
// widget on the same line, so a second mode indicator would be redundant.
const DEFAULT_SHOW_MODE: bool = false;
// Off by default: clicks need permission to run actions as the user, which a
// bar that only shows hints has no reason to ask for.
const DEFAULT_MOUSE: bool = false;
// Off by default: a click on a nested session's bar types keys into it, which
// is less obviously what clicking a bar does than running a binding here.
const DEFAULT_MOUSE_NESTED: bool = false;
// On by default: with `mouse` on, the menus and their previews are how the
// hints a click cannot run directly are reached.
const DEFAULT_MOUSE_HOVER: bool = true;
const DEFAULT_MOUSE_MENUS: bool = true;
// Drawn where hints were dropped when mouse support is on and no
// `drop_indicator` is configured, so the hints that did not fit always have
// something to click.
const DEFAULT_MOUSE_DROP_INDICATOR: &str = "…";
/// Context key marking the actions this plugin runs for a click, so their
/// `ActionComplete` can be told apart.
const MOUSE_ACTION_CONTEXT: &str = "zjhints_mouse";

type ActionLabel = (Action, &'static str);
type ActionSequenceLabel = (&'static [Action], &'static str);

/// Which keys to show when a hint is bound to both the `hjkl` letters and the
/// arrow keys (e.g. move/focus). `Both` (the default) keeps the existing
/// behavior; `Arrows`/`Letters` drop the other family so the hint is shorter.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum DirectionKeys {
    #[default]
    Both,
    Arrows,
    Letters,
}

impl DirectionKeys {
    fn from_config(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "arrows" | "arrow" => Self::Arrows,
            "letters" | "hjkl" | "vim" => Self::Letters,
            _ => Self::Both,
        }
    }
}

/// Physical key order used to sort the keys within a single hint, so a hint
/// bound to several keys reads in the order they sit under your hands rather
/// than the order Zellij happens to report them.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum KeyOrder {
    #[default]
    Qwerty,
    Dvorak,
    Colemak,
    /// Ignore the keyboard entirely: digits `0-9`, then letters `a-z`.
    Alphabetical,
    /// Leave keys exactly as Zellij reports them.
    Unsorted,
}

impl KeyOrder {
    fn from_config(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "dvorak" => Self::Dvorak,
            "colemak" => Self::Colemak,
            "abcdef" | "alphabetical" | "alpha" | "abc" => Self::Alphabetical,
            "none" | "off" | "unsorted" => Self::Unsorted,
            _ => Self::Qwerty,
        }
    }

    /// The rows of this layout, top to bottom, each listed left to right and
    /// including the digit row. Position within these rows is the whole of the
    /// ordering — digits included, which is what puts `0` after `9`.
    fn rows(self) -> &'static [&'static str] {
        match self {
            Self::Qwerty => &[
                "1234567890-=",
                "qwertyuiop[]\\",
                "asdfghjkl;'",
                "zxcvbnm,./",
            ],
            Self::Dvorak => &[
                "1234567890[]",
                "',.pyfgcrl/=\\",
                "aoeuidhtns-",
                ";qjkxbmwvz",
            ],
            Self::Colemak => &[
                "1234567890-=",
                "qwfpgjluy;[]\\",
                "arstdhneio'",
                "zxcvbkm,./",
            ],
            Self::Alphabetical | Self::Unsorted => &[],
        }
    }
}

/// A user-supplied ordering for the hints within a mode, parsed from a
/// comma-separated `hint_order` list.
///
/// A `*` in the list marks where everything unlisted goes, so entries before it
/// are pinned to the front and entries after it to the back. Omitting `*` is the
/// same as ending with one: the list becomes the leading order and everything
/// else follows.
#[derive(Clone, Default)]
struct HintOrder {
    first: Vec<String>,
    last: Vec<String>,
}

impl HintOrder {
    fn from_config(value: &str) -> Self {
        let (mut first, mut last) = (vec![], vec![]);
        let mut after_wildcard = false;
        for entry in value.split(',') {
            match entry.trim() {
                "" => continue,
                "*" => after_wildcard = true,
                entry if after_wildcard => last.push(entry.to_lowercase()),
                entry => first.push(entry.to_lowercase()),
            }
        }
        Self { first, last }
    }

    fn is_empty(&self) -> bool {
        self.first.is_empty() && self.last.is_empty()
    }

    /// Which group a hint belongs to: `Leading`, `Middle`, or `Trailing`. This
    /// is what decides the order hints are given up in when space runs short.
    fn group(&self, hint: &Hint) -> Group {
        if Self::position(&self.first, hint).is_some() {
            Group::Leading
        } else if Self::position(&self.last, hint).is_some() {
            Group::Trailing
        } else {
            Group::Middle
        }
    }

    /// Sort key placing a hint in the leading, middle, or trailing group. Every
    /// unlisted hint shares one key, so a stable sort leaves them in the order
    /// the mode built them.
    fn rank(&self, hint: &Hint) -> (u8, usize) {
        if let Some(index) = Self::position(&self.first, hint) {
            return (0, index);
        }
        if let Some(index) = Self::position(&self.last, hint) {
            return (2, index);
        }
        (1, 0)
    }

    /// Entries name a hint by concept id or by the label it displays. The label
    /// matters for a hint fused from several under a shared label, whose id is
    /// only that of whichever binding came first.
    fn position(entries: &[String], hint: &Hint) -> Option<usize> {
        let id = hint.id.to_lowercase();
        let label = hint.label.to_lowercase();
        entries
            .iter()
            .position(|entry| *entry == id || *entry == label)
    }
}

/// How each hint should be styled while rendering. Borrows from `State` so the
/// styling functions can fall back to the theme palette when no format string
/// is configured, and resolve `$alias` colors from the plugin configuration.
struct HintStyle<'a> {
    colors: &'a Styling,
    /// How strongly to dim custom-format colors (`key_format`, `desc_format`,
    /// `hint_spacer`, `drop_indicator`), `0.0` = unchanged. `colors` is
    /// already the dimmed `Styling` when this is nonzero; this is only for
    /// colors resolved by `format::render_template`, which bypasses `colors`
    /// entirely, so it needs the dim amount passed in separately.
    dim: f32,
    key_format: Option<&'a str>,
    desc_format: Option<&'a str>,
    /// Rendered between consecutive hints, and before the first when
    /// `spacer_after_prefix` is set, never after the last. `None` when unset,
    /// leaving hints adjacent as before.
    spacer: Option<&'a str>,
    /// Whether the mode prefix is drawn in front of the hints, so `spacer`
    /// separates it from the first hint as it separates hints from each other.
    spacer_after_prefix: bool,
    discover: bool,
    direction_keys: DirectionKeys,
    key_order: KeyOrder,
    /// The mode being rendered, lowercased, for mode-scoped `label_` lookups.
    mode: &'a str,
    /// User-chosen ordering for the hints within a mode; empty leaves the order
    /// each mode builds.
    hint_order: &'a HintOrder,
    /// Columns available for the hints, or `None` when unbounded. Whole hints
    /// are dropped to fit rather than the line being cut mid-hint.
    limit: Option<usize>,
    /// See `State::wide_ambiguous`.
    wide_ambiguous: bool,
    /// Rendered in place of hints dropped for lack of room. `None` leaves the
    /// gap unmarked.
    drop_indicator: Option<&'a str>,
    /// Which pinned group outlives the other when space runs short.
    precedence: Precedence,
    /// Base-mode keybindings, used to suppress globals that every mode inherits.
    /// Empty when `hide_shared_hints` is off or when rendering the base mode.
    shared: &'a [(KeyWithModifier, Vec<Action>)],
    config: &'a BTreeMap<String, String>,
    /// What clicking one of the hints does.
    click: ClickBehavior,
}

/// What clicking a hint does.
#[derive(Clone, Copy, PartialEq)]
enum ClickBehavior {
    /// Run the actions its keys are bound to, here. For this session's own
    /// hints.
    RunActions,
    /// Press one of its keys in the focused pane. For a descended-into nested
    /// session's hints, whose keys mean something only to that session.
    PressKeys,
}

const NORMAL_MODE_ACTIONS: &[ActionLabel] = &[
    (
        Action::SwitchToMode {
            input_mode: InputMode::Pane,
        },
        "pane",
    ),
    (
        Action::SwitchToMode {
            input_mode: InputMode::Tab,
        },
        "tab",
    ),
    (
        Action::SwitchToMode {
            input_mode: InputMode::Resize,
        },
        "resize",
    ),
    (
        Action::SwitchToMode {
            input_mode: InputMode::Move,
        },
        "move",
    ),
    (
        Action::SwitchToMode {
            input_mode: InputMode::Scroll,
        },
        "scroll",
    ),
    (
        Action::SwitchToMode {
            input_mode: InputMode::Search,
        },
        "search",
    ),
    (
        Action::SwitchToMode {
            input_mode: InputMode::Session,
        },
        "session",
    ),
    (Action::Quit, "quit"),
];

const PANE_MODE_ACTION_SEQUENCES: &[ActionSequenceLabel] = &[
    (
        &[
            Action::NewPane {
                direction: None,
                pane_name: None,
                start_suppressed: false,
            },
            TO_NORMAL,
        ],
        "new",
    ),
    (&[Action::CloseFocus, TO_NORMAL], "close"),
    (&[Action::ToggleFocusFullscreen, TO_NORMAL], "fullscreen"),
    (&[Action::ToggleFloatingPanes, TO_NORMAL], "float"),
    (&[Action::TogglePaneEmbedOrFloating, TO_NORMAL], "embed"),
    (
        &[
            Action::NewPane {
                direction: Some(Direction::Right),
                pane_name: None,
                start_suppressed: false,
            },
            TO_NORMAL,
        ],
        "split right",
    ),
    (
        &[
            Action::NewPane {
                direction: Some(Direction::Down),
                pane_name: None,
                start_suppressed: false,
            },
            TO_NORMAL,
        ],
        "split down",
    ),
];

const TAB_MODE_ACTION_SEQUENCES: &[ActionSequenceLabel] = &[
    (
        &[
            Action::NewTab {
                tiled_layout: None,
                floating_layouts: vec![],
                swap_tiled_layouts: None,
                swap_floating_layouts: None,
                tab_name: None,
                should_change_focus_to_new_tab: true,
                cwd: None,
                initial_panes: None,
                first_pane_unblock_condition: None,
            },
            TO_NORMAL,
        ],
        "new",
    ),
    (&[Action::CloseTab, TO_NORMAL], "close"),
    (&[Action::BreakPane, TO_NORMAL], "break pane"),
    (&[Action::ToggleActiveSyncTab, TO_NORMAL], "sync"),
];

impl State {
    /// How wide the hint line may be, or `None` for no limit.
    ///
    /// `max_length` is a hard cap the user set; auto-fitting derives one from
    /// the bar's own pane when it has one, and otherwise from the terminal,
    /// less what zjstatus needs beside it. With both in play the smaller wins,
    /// so an explicit cap is never exceeded just because the window is wide.
    fn length_limit(&self) -> Option<usize> {
        let explicit = (self.max_length > 0).then_some(self.max_length);
        let auto = self.auto_width.then(|| self.auto_width_limit()).flatten();
        match (explicit, auto) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (limit, None) | (None, limit) => limit,
        }
    }

    /// The width auto-fitting allows: the whole of the bar's own pane, which
    /// nothing else shares, or for a bar feeding zjstatus, the terminal less
    /// `reserve_columns`.
    fn auto_width_limit(&self) -> Option<usize> {
        self.bar.pane_width.or_else(|| {
            self.terminal_width
                .map(|width| width.saturating_sub(self.reserve_columns))
        })
    }

    /// Whether this session is nested inside another Zellij session.
    fn is_nested(&self) -> bool {
        nesting::is_nested(&self.mode_info)
    }

    /// Whether the host has expanded this session's pane over its whole
    /// display, taking its own status bar off the screen with it. See
    /// `nesting::host_ui_is_covered`.
    fn host_ui_is_covered(&self) -> bool {
        nesting::host_ui_is_covered(&self.mode_info)
    }

    /// Whether this session currently has its own focus deferred to a
    /// nested child it is hosting, regardless of whether this session is
    /// itself also nested inside something else. Independent of the
    /// `dim_when_unfocused`/`dim_strength` display options: this is the raw
    /// fact the descended-into indicator (`render_descended_indicator`) acts
    /// on, not a rendering preference.
    fn is_host_descended(&self) -> bool {
        nesting::is_host_descended(&self.mode_info)
    }

    /// The pane the keyboard currently goes to, or `None` while either half of
    /// the picture (`PaneUpdate`, `TabUpdate`) is still missing.
    fn focused_pane_id(&self) -> Option<PaneId> {
        zjkit::panes::focused_pane_id(self.active_tab, &self.focus_by_tab)
    }

    /// The nested session this one has descended into, when it has descended
    /// into anything and that session has told us about itself.
    ///
    /// Descending hands the keyboard to the session in the focused pane, so
    /// that pane is what identifies the nested session among the several this
    /// one may be hosting.
    fn descended_guest(&self) -> Option<&NestedGuest> {
        if !self.is_host_descended() {
            return None;
        }
        self.nested_guests.get(&self.focused_pane_id()?)
    }

    /// Record a nested session's mode report, fetching its keybindings when
    /// the report's generation says the ones held (if any) are stale.
    ///
    /// Returns whether anything changed, so the caller can skip a redraw that
    /// would produce the same bar.
    ///
    /// The fetch blocks until the host answers, which it bounds at about a
    /// second. A failed fetch still marks the generation as tried: a nested
    /// session that cannot answer now will not answer on its next mode change
    /// either, and retrying would stall the plugin each time. A new generation
    /// gets a fresh try.
    fn record_nested_guest(
        &mut self,
        pane_id: PaneId,
        session_path: Vec<String>,
        mode: InputMode,
        base_mode: Option<InputMode>,
        keybinds_generation: u64,
    ) -> bool {
        let guest = self.nested_guests.entry(pane_id).or_default();
        let mut changed = guest.session_path != session_path
            || guest.mode != mode
            || guest.base_mode != base_mode;
        guest.session_path = session_path;
        guest.mode = mode;
        guest.base_mode = base_mode;
        if guest.fetched_generation != Some(keybinds_generation) {
            guest.fetched_generation = Some(keybinds_generation);
            changed |= self.fetch_nested_keybinds(pane_id);
        }
        changed
    }

    /// Fetch one nested session's keybindings and store what comes back.
    ///
    /// A reply describes the session as it is when answering, which may be a
    /// generation past the report that prompted the fetch, so its generation
    /// is the one recorded. The mode it carries is left alone: mode reports
    /// keep arriving on their own, and the next one brings it. Errors that mean
    /// the pane no longer holds a nested session drop it; the rest keep
    /// whatever bindings were already held, or none, which renders as the
    /// descended-into indicator.
    fn fetch_nested_keybinds(&mut self, pane_id: PaneId) -> bool {
        match (self.fetch_keybinds.0)(pane_id) {
            Ok(reply) => {
                let Some(guest) = self.nested_guests.get_mut(&pane_id) else {
                    return false;
                };
                guest.keybinds = reply.keybinds;
                guest.fetched_generation = Some(reply.keybinds_generation);
                true
            }
            Err(NestedSessionKeybindsError::NotANestedSession)
            | Err(NestedSessionKeybindsError::GuestGone) => {
                self.nested_guests.remove(&pane_id).is_some()
            }
            Err(_) => false,
        }
    }

    /// Forget nested sessions whose panes are gone, so a pane id Zellij later
    /// reuses cannot inherit a dead session's hints.
    fn forget_closed_nested_guests(&mut self, manifest: &PaneManifest) {
        if self.nested_guests.is_empty() {
            return;
        }
        let live_pane_ids: BTreeSet<PaneId> =
            manifest.panes.values().flatten().map(pane_id_of).collect();
        self.nested_guests
            .retain(|pane_id, _| live_pane_ids.contains(pane_id));
    }

    /// The dim strength to render with right now, in `zjkit::dim::color`'s
    /// `0.0..=1.0` scale. See `nesting::dim_amount`.
    fn dim_amount(&self) -> f32 {
        nesting::dim_amount(&self.mode_info, self.dim_when_unfocused, self.dim_strength)
    }

    /// The dim strength for the hints of the nested session this one has
    /// descended into.
    ///
    /// Those hints describe the session the keyboard is reaching, so having
    /// descended (`session_dimmed`) is no reason to fade them: with
    /// `hide_when_nested` the nested session draws no bar of its own, and
    /// this is the only place its hints appear. They still fade when this
    /// session is itself nested and its host has ascended out of it, since
    /// then nothing below this session is receiving input either.
    fn descended_guest_dim_amount(&self) -> f32 {
        if self.dim_when_unfocused && nesting::is_ascended_out_of(&self.mode_info) {
            self.dim_strength
        } else {
            0.0
        }
    }
}

/// A dimmed copy of `colors`. Only the `Styling` groups this plugin's own
/// rendering actually reads are touched, so dimming the rest would be dead
/// work: `ribbon_unselected`/`text_unselected` for hints
/// (`style_key_with_modifier`/`style_key_text`/`style_description`), and
/// `ribbon_selected` for the optional mode prefix (`style_mode`).
fn dim_styling(colors: &Styling, strength: f32) -> Styling {
    Styling {
        ribbon_unselected: zjkit::dim::style_declaration(&colors.ribbon_unselected, strength),
        ribbon_selected: zjkit::dim::style_declaration(&colors.ribbon_selected, strength),
        text_unselected: zjkit::dim::style_declaration(&colors.text_unselected, strength),
        ..*colors
    }
}

/// Renders the placeholder shown in the host's hint bar while descended
/// into a nested session, naming the keys that ascend back out. Built
/// entirely from this session's own `nested_ascend_keys`, not a mirror of
/// the nested session's actual mode or hints, that needs cross-session data
/// this plugin does not have (see docs/configuration.md). Empty when there
/// are no ascend keys to show.
///
/// Rendered as an ordinary `Hint` (id `descended`, see `DESCENDED_HINT_ID`)
/// through the normal `render_hint` path, rather than a bespoke styling
/// function of its own: `key_format`/`desc_format` (default or per-hint,
/// `key_format_descended`/`desc_format_descended`), `label_descended`, and
/// `keys_descended` all apply to it exactly as they would to any other
/// hint, including key and modifier aliases through the usual
/// `style_key_with_modifier`/`compose_key_text` path. `style.key_order`
/// should be `KeyOrder::Unsorted` when the caller builds it: the ascend keys
/// are a sequence (press one, then the other), not the alternative-key set
/// `sort_keys` is meant to reorder for a normal hint.
fn render_descended_indicator(mode_info: &ModeInfo, style: &HintStyle) -> String {
    if mode_info.nested_ascend_keys.is_empty() {
        return String::new();
    }

    let hint = Hint {
        id: DESCENDED_HINT_ID.to_string(),
        merge_key: DESCENDED_HINT_ID.to_string(),
        label: DESCENDED_HINT_LABEL.to_string(),
        keys: mode_info.nested_ascend_keys.clone(),
    };
    let mut parts = vec![];
    render_hint(&mut parts, &hint, style);
    ANSIStrings(&parts).to_string()
}

/// What the bar should be showing.
///
/// The `Hints` variant is the width of a `ModeInfo` and the others carry
/// nothing, which makes this a lopsided enum. Boxing it would trade that for a
/// heap allocation on the path that renders every bar, for a value that is
/// built once per render and dropped at the end of it.
#[allow(clippy::large_enum_variant)]
enum BarSubject<'a> {
    /// Nothing: the bar is deliberately empty.
    Nothing,
    /// The hints of the session this `ModeInfo` describes, which is a nested
    /// session this one has descended into, or this session itself.
    Hints(Cow<'a, ModeInfo>),
    /// Only the way back out of the nested session this one has descended
    /// into, because that session has not said enough to draw its hints.
    DescendedIndicator,
}

impl State {
    /// What the bar should be showing, in priority order.
    ///
    /// Descending outranks `hide_in_base_mode`: descending is exactly the
    /// out-of-the-ordinary state that option exists to keep out of the way of,
    /// and the same reasoning already kept the descended-into indicator
    /// showing regardless of it.
    fn bar_subject(&self) -> BarSubject<'_> {
        if self.is_nested() && self.hide_when_nested && !self.host_ui_is_covered() {
            // `hide_when_nested` hides this bar because the host is drawing one
            // just below it. Once the host has covered its own bar to give this
            // session the whole display, that is no longer true, and staying
            // hidden would leave the screen with no hints on it at all.
            BarSubject::Nothing
        } else if let Some(guest_mode_info) = self.descended_guest_mode_info() {
            // Descended into a nested session that has told us its mode and
            // keybindings: those are the keys the user's typing reaches, so
            // they are what the bar should describe.
            BarSubject::Hints(Cow::Owned(guest_mode_info))
        } else if self.is_host_descended() {
            // Descended into a nested session that has not told us enough (the
            // request for its keybindings is still in flight, or it runs a
            // Zellij too old to answer it). This session's own hints would
            // describe a mode nothing is typing into, so show the way back out
            // instead of them. Independent of dim_when_unfocused/dim_strength:
            // whether to show this at all is not a display preference the way
            // the color dim is.
            BarSubject::DescendedIndicator
        } else if self.hide_in_base_mode && Some(self.mode_info.mode) == self.mode_info.base_mode {
            BarSubject::Nothing
        } else {
            BarSubject::Hints(Cow::Borrowed(&self.mode_info))
        }
    }

    /// Tell Zellij whether this pane's row is worth keeping, when the answer
    /// has changed since the last time it was told.
    ///
    /// A collapsed pane hands its row to the panes around it but keeps its
    /// place in the layout, so expanding gives back the exact row the layout
    /// asked for. Zellij ignores this for a plugin that has no tiled pane, so
    /// running as a zjstatus pipe source alone costs nothing.
    ///
    /// Turning `collapse_when_empty` off expands again on the next render
    /// rather than waiting for the bar to fill up on its own.
    fn sync_collapsed(&mut self, output_is_empty: bool) {
        if let Some(collapsed) = self.next_collapsed(output_is_empty) {
            self.host.set_collapsed(collapsed);
        }
    }

    /// What this render has to say about the pane's row, or `None` when it
    /// says the same thing the last one did and there is no point repeating
    /// it. Records the answer as the new last word.
    fn next_collapsed(&mut self, output_is_empty: bool) -> Option<bool> {
        self.bar
            .visibility
            .next_collapsed(self.collapse_when_empty, output_is_empty)
    }

    /// This session's descended-into nested session, described as a `ModeInfo`
    /// so the ordinary hint path can render it.
    ///
    /// `None` when nothing is descended into, or when the nested session has
    /// not sent its keybindings yet: with no bindings there is nothing to draw
    /// hints from, and the caller falls back to the descended-into indicator.
    /// That covers both the moment between descending and the answer arriving,
    /// and a nested session running a Zellij too old to answer at all.
    ///
    /// The style is this session's own, so a nested session's hints are drawn
    /// in the colors of the bar they appear in rather than the nested
    /// session's.
    fn descended_guest_mode_info(&self) -> Option<ModeInfo> {
        let guest = self.descended_guest()?;
        if guest.keybinds.is_empty() {
            return None;
        }
        Some(ModeInfo {
            mode: guest.mode,
            base_mode: guest.base_mode,
            keybinds: guest.keybinds.clone(),
            style: self.mode_info.style,
            session_name: guest.session_path.last().cloned(),
            ..Default::default()
        })
    }

    /// The line to draw and what clicking each of its columns does.
    ///
    /// This session's own hints run their bindings when clicked. A nested
    /// session's hints describe keys that reach the nested session, not this
    /// one, so they and the way back out of it can only be clicked with
    /// `mouse_nested`, which types their keys into the focused pane, the one
    /// the descended-into session runs in.
    fn draw_bar(&self) -> (String, ClickMap) {
        if self.bar.visibility.is_hidden() {
            return (String::new(), ClickMap::default());
        }
        let mode_info = &self.mode_info;
        let dim = self.dim_amount();

        match self.bar_subject() {
            BarSubject::Nothing => (String::new(), ClickMap::default()),
            BarSubject::Hints(subject) => {
                // An owned subject is the descended-into session's, built by
                // `descended_guest_mode_info`. A borrowed one is this
                // session's own.
                match subject {
                    Cow::Owned(_) => {
                        let guest_dim = self.descended_guest_dim_amount();
                        let (line, click_map) =
                            self.render_hint_line(&subject, guest_dim, ClickBehavior::PressKeys);
                        if self.mouse_nested {
                            (line, click_map)
                        } else {
                            (line, ClickMap::default())
                        }
                    }
                    Cow::Borrowed(_) => {
                        self.render_hint_line(&subject, dim, ClickBehavior::RunActions)
                    }
                }
            }
            BarSubject::DescendedIndicator => {
                let dimmed_colors;
                let colors: &Styling = if dim > 0.0 {
                    dimmed_colors = dim_styling(&mode_info.style.colors, dim);
                    &dimmed_colors
                } else {
                    &mode_info.style.colors
                };
                let mode_key = format!("{:?}", mode_info.mode).to_lowercase();
                let hint_order = HintOrder::default();
                let indicator_style = HintStyle {
                    colors,
                    dim,
                    key_format: self.key_format.as_deref(),
                    desc_format: self.desc_format.as_deref(),
                    spacer: None,
                    spacer_after_prefix: false,
                    discover: false,
                    direction_keys: DirectionKeys::default(),
                    // A sequence ("press Ctrl o, then ]"), not the alternative
                    // keys sort_keys reorders for a normal hint.
                    key_order: KeyOrder::Unsorted,
                    mode: &mode_key,
                    hint_order: &hint_order,
                    limit: None,
                    wide_ambiguous: self.wide_ambiguous,
                    drop_indicator: None,
                    precedence: Precedence::default(),
                    shared: &[],
                    config: &self.bar.configuration,
                    click: ClickBehavior::RunActions,
                };
                let styled_indicator = render_descended_indicator(mode_info, &indicator_style);
                let (prefix, prefix_width) = if styled_indicator.is_empty() || !self.show_mode {
                    (String::new(), 0)
                } else {
                    let mode_prefix = render_mode_prefix(
                        mode_info.mode,
                        colors,
                        self.mode_format.as_deref(),
                        &self.bar.configuration,
                        dim,
                    );
                    // Spaced from the prefix as the first hint would be.
                    let spacer = self
                        .hint_spacer
                        .as_deref()
                        .map(|spacer| {
                            format::render_template(spacer, &[], &self.bar.configuration, dim)
                        })
                        .unwrap_or_default();
                    let prefix_width = calculate_visible_length(
                        &ANSIStrings(&mode_prefix).to_string(),
                        self.wide_ambiguous,
                    );
                    (
                        format!("{}{}", ANSIStrings(&mode_prefix), ANSIStrings(&spacer)),
                        prefix_width,
                    )
                };
                // Ascending is the nested session's own binding, which it acts
                // on when its keys reach it, so clicking types them into it.
                let segments = if self.mouse_nested && !styled_indicator.is_empty() {
                    vec![LineSegment {
                        width: calculate_visible_length(&styled_indicator, self.wide_ambiguous),
                        content: SegmentContent::Item(descended_indicator_target(
                            mode_info,
                            &self.bar.configuration,
                        )),
                    }]
                } else {
                    vec![]
                };
                let start = calculate_visible_length(&prefix, self.wide_ambiguous);
                let (line, click_map) = self.fit_line(
                    format!("{prefix}{styled_indicator}"),
                    segments,
                    start,
                    0..prefix_width,
                    dim,
                );
                if self.mouse_nested {
                    (line, click_map)
                } else {
                    (line, ClickMap::default())
                }
            }
        }
    }

    /// What a mouse event on the bar leads to, with no menu when
    /// `mouse_menus` is off.
    fn bar_click_outcome(&self, mouse: &Mouse) -> MouseOutcome {
        match mouse::click_outcome(&self.click_map, mouse) {
            MouseOutcome::Menu(_) if !self.mouse_menus => MouseOutcome::Nothing,
            outcome => outcome,
        }
    }

    /// Carry out what a click or a menu answer led to. With `run_mouse_action`,
    /// the only part of mouse support that talks to the host; see the `mouse`
    /// module for the rest.
    fn perform_mouse_outcome(&mut self, outcome: MouseOutcome) {
        match outcome {
            MouseOutcome::Nothing => {}
            MouseOutcome::ContextMenu { line, column } => {
                self.host
                    .open_context_menu(ContextMenuTarget::Bar, line, column);
            }
            MouseOutcome::Run(actions) => {
                let first = self.action_queue.enqueue(actions);
                self.run_mouse_action(first);
            }
            MouseOutcome::Menu(entries) => {
                if !entries.is_empty() {
                    let request_id = self.host.prompt(mouse::menu_request(&entries));
                    self.pending_menus.insert(request_id, entries);
                }
            }
            MouseOutcome::OpenSettings => self.bar.open_settings(&self.host, &BAR),
        }
    }

    /// Hand one queued action to Zellij, tagged so its completion starts the
    /// next. See `ActionQueue`.
    fn run_mouse_action(&self, action: Option<Action>) {
        if let Some(action) = action {
            let context = BTreeMap::from([(MOUSE_ACTION_CONTEXT.to_string(), String::new())]);
            self.host.run_action(action, context);
        }
    }

    /// Keep the hover preview in step with a mouse event on the bar: a hover
    /// shows, keeps or closes it, and a click closes it before the click acts.
    fn follow_mouse_with_preview(&mut self, mouse: &Mouse) {
        // The popup is this plugin again, opened by URL, so none can open
        // before the first `PaneUpdate` reports it.
        if self.bar.own_url.is_none() {
            return;
        }
        let step = match mouse {
            Mouse::Hover(..) if self.mouse_hover => {
                self.preview_memory.hover(&self.click_map, mouse)
            }
            // With previews off, a hover only closes one still open from
            // before they were turned off.
            Mouse::Hover(..) | Mouse::LeftClick(..) | Mouse::RightClick(..) => {
                self.preview_memory.dismiss()
            }
            _ => PreviewStep::default(),
        };
        self.take_preview_step(step);
    }

    fn take_preview_step(&self, step: PreviewStep) {
        if let Some(popup) = step.close {
            self.host.close_preview_popup(&popup::NAMES, popup);
        }
        if step.wait {
            self.host.set_timeout(mouse::HOVER_DELAY_SECONDS);
        }
        if let Some((generation, preview)) = step.open {
            self.show_preview(generation, &preview);
        }
    }

    fn show_preview(&self, generation: u64, preview: &Preview) {
        let Some(url) = &self.bar.own_url else {
            return;
        };
        let tag = mouse::preview_tag(self.bar.plugin_id.unwrap_or_default(), generation);
        self.host.open_preview_popup(
            &popup::NAMES,
            url,
            &tag,
            &preview.labels(),
            self.pane_rows.max(1),
            preview.columns.start,
        );
    }

    /// Take `message` if it is a popup of this bar saying it opened, closed
    /// or had an entry clicked.
    fn take_popup_message(&mut self, message: &PipeMessage) {
        let opened = message.name == popup::NAMES.opened;
        let picked = message.name == popup::NAMES.picked;
        if !opened && !picked && message.name != popup::NAMES.closed {
            return;
        }
        let PipeSource::Plugin(popup) = message.source else {
            return;
        };
        let Some(generation) = message
            .payload
            .as_deref()
            .and_then(mouse::parse_preview_tag)
            .filter(|(owner, _)| Some(*owner) == self.bar.plugin_id)
            .map(|(_, generation)| generation)
        else {
            return;
        };
        if picked {
            let index = message
                .args
                .get(mouse::HOVER_PICKED_ENTRY_ARG)
                .and_then(|index| index.parse().ok());
            if let Some(index) = index {
                let outcome = self.preview_memory.picked(popup, generation, index);
                self.perform_mouse_outcome(outcome);
            }
        } else if opened {
            let step = self.preview_memory.popup_opened(popup, generation);
            self.take_preview_step(step);
        } else {
            self.preview_memory.popup_closed(generation);
        }
    }

    /// The hint line for one session's mode and keybindings, curated, ordered,
    /// styled, dimmed and truncated according to this plugin's configuration.
    ///
    /// `mode_info` is what is being described, which is this session normally
    /// and the nested session it has descended into otherwise. Everything else
    /// comes from this session, because the bar being drawn is this session's.
    ///
    /// Also returns which columns of the line hold which hint, for mouse
    /// clicks, which do what `click` says.
    fn render_hint_line(
        &self,
        mode_info: &ModeInfo,
        dim: f32,
        click: ClickBehavior,
    ) -> (String, ClickMap) {
        let keymap = get_keymap_for_mode(mode_info);
        // Bindings the base mode already advertises. Every non-base mode
        // inherits these via Zellij's `shared_except` groups, so listing
        // them again in each mode is pure repetition. Locked mode is the
        // exception: it can last a long time, the base mode's bar is not on
        // screen meanwhile, and only keys bound there deliberately work in it.
        let base_mode = mode_info.base_mode.unwrap_or(InputMode::Normal);
        let shared = if self.hide_shared_hints
            && mode_info.mode != base_mode
            && mode_info.mode != InputMode::Locked
        {
            mode_info.get_keybinds_for_mode(base_mode)
        } else {
            vec![]
        };
        let mode_key = format!("{:?}", mode_info.mode).to_lowercase();
        let limit = self.length_limit();
        let dimmed_colors;
        let colors: &Styling = if dim > 0.0 {
            dimmed_colors = dim_styling(&mode_info.style.colors, dim);
            &dimmed_colors
        } else {
            &mode_info.style.colors
        };
        let mode_prefix = if self.show_mode {
            render_mode_prefix(
                mode_info.mode,
                colors,
                self.mode_format.as_deref(),
                &self.bar.configuration,
                dim,
            )
        } else {
            vec![]
        };
        let mode_prefix_len =
            calculate_visible_length(&ANSIStrings(&mode_prefix).to_string(), self.wide_ambiguous);

        // A bare leading space is only wanted when the hints open the
        // line themselves: it's breathing room against the pane edge.
        // The mode prefix (when shown) already opens with its own
        // styled background, so an unstyled space in front of it would
        // just show up as a gap of default terminal background before
        // the pill starts.
        let leading_space: usize = if mode_prefix_len > 0 { 0 } else { 1 };
        let ctx = HintStyle {
            colors,
            dim,
            key_format: self.key_format.as_deref(),
            desc_format: self.desc_format.as_deref(),
            spacer: self.hint_spacer.as_deref(),
            spacer_after_prefix: mode_prefix_len > 0,
            discover: self.discover_hints,
            direction_keys: self.direction_keys,
            key_order: self.key_order,
            mode: &mode_key,
            hint_order: &self.hint_order,
            limit: limit.map(|limit| {
                limit
                    .saturating_sub(leading_space)
                    .saturating_sub(mode_prefix_len)
            }),
            wide_ambiguous: self.wide_ambiguous,
            drop_indicator: self.drop_indicator.as_deref(),
            precedence: self.hint_precedence,
            shared: &shared,
            config: &self.bar.configuration,
            click,
        };
        let (hint_parts, segments) = lay_out_hints_for_mode(mode_info.mode, &keymap, &ctx);
        let mut parts = mode_prefix;
        parts.extend(hint_parts);

        let ansi_strings = ANSIStrings(&parts);
        let formatted = if leading_space > 0 {
            format!(" {}", ansi_strings)
        } else {
            ansi_strings.to_string()
        };

        self.fit_line(
            formatted,
            segments,
            leading_space + mode_prefix_len,
            0..mode_prefix_len,
            dim,
        )
    }

    /// Cut `line` down to `max_length` when it is longer, and say which of its
    /// columns hold what. `segments` are laid out from column `start`, and
    /// the mode prefix, which opens the settings screen, takes `prefix`.
    /// `overflow_str` is drawn faded by `dim`, as the line it ends is.
    fn fit_line(
        &self,
        line: String,
        segments: Vec<LineSegment>,
        start: usize,
        prefix: Range<usize>,
        dim: f32,
    ) -> (String, ClickMap) {
        let visible_len = calculate_visible_length(&line, self.wide_ambiguous);
        match self.length_limit() {
            Some(limit) if visible_len > limit => {
                let overflow = render_overflow(&self.overflow_str, &self.bar.configuration, dim);
                let truncated = truncate_ansi_string(&line, &overflow, limit, self.wide_ambiguous);
                // Measured from what was drawn rather than worked out from
                // `limit`, since the cut keeps whole characters and a wide one
                // can leave it short of the limit.
                let shown = calculate_visible_length(&truncated, self.wide_ambiguous);
                let marker = calculate_visible_length(&overflow, self.wide_ambiguous).min(shown);
                let cut = Cut {
                    at: shown - marker,
                    overflow: shown - marker..shown,
                };
                let prefix = prefix.start..prefix.end.min(cut.at);
                let map = ClickMap::lay_out(segments, start, Some(cut));
                (truncated, map.with_settings(prefix))
            }
            _ => (
                line,
                ClickMap::lay_out(segments, start, None).with_settings(prefix),
            ),
        }
    }
}

/// `overflow_str` as drawn: a format string like `drop_indicator`, so a theme
/// can shape it. A styled one opens with a reset, so nothing of the hint the
/// cut went through, such as its bold, carries into it.
fn render_overflow(overflow_str: &str, config: &BTreeMap<String, String>, dim: f32) -> String {
    let rendered =
        ANSIStrings(&format::render_template(overflow_str, &[], config, dim)).to_string();
    if rendered.contains('\u{1b}') {
        format!("{ANSI_RESET}{rendered}")
    } else {
        rendered
    }
}

/// The marker drawn where hints were dropped to fit, or `None` for none.
///
/// Unset leaves the gap unmarked, except with mouse support on: the marker is
/// what opens the menu of dropped hints, so they get a default one rather than
/// becoming unreachable.
fn drop_indicator_from_config(
    configuration: &BTreeMap<String, String>,
    mouse: bool,
) -> Option<String> {
    config::non_empty_string_option(configuration, CONFIG_DROP_INDICATOR)
        .or_else(|| mouse.then(|| DEFAULT_MOUSE_DROP_INDICATOR.to_string()))
}

/// The zjstatus pipe names to publish each render on.
///
/// An explicit `pipe_name` is used on its own: someone who has named their
/// pipe has a zjstatus config reading that name and no other. Left unset,
/// both the current default and the pre-rename one go out, so a config
/// written against either keeps rendering.
impl State {
    /// Read every option from `user_configuration`, with the themes laid
    /// under it, as `load` does and as a live update from the settings screen
    /// does, so a value behaves the same whichever way it arrived. See
    /// `BarCore::configure`.
    fn apply_configuration(&mut self, user_configuration: BTreeMap<String, String>) {
        let configuration = self.bar.configure(&BAR, user_configuration);
        self.max_length = config::usize_option(configuration, "max_length", DEFAULT_MAX_LENGTH);
        self.overflow_str = configuration
            .get(CONFIG_OVERFLOW_STR)
            .cloned()
            .unwrap_or_else(|| DEFAULT_OVERFLOW_STR.to_string());
        self.pipe_names = pipe_names_from_config(configuration);
        self.hide_in_base_mode = config::bool_option(configuration, "hide_in_base_mode", false);
        // Render nothing at all when this session is nested (see
        // `is_nested`), so nested sessions get no bottom bar from the same
        // shared config a host session uses.
        self.hide_when_nested = config::bool_option(
            configuration,
            CONFIG_HIDE_WHEN_NESTED,
            DEFAULT_HIDE_WHEN_NESTED,
        );
        // Give the row back to the panes around it while there is nothing on
        // it, rather than drawing a blank one. See `sync_collapsed`.
        self.collapse_when_empty = config::bool_option(
            configuration,
            CONFIG_COLLAPSE_WHEN_EMPTY,
            DEFAULT_COLLAPSE_WHEN_EMPTY,
        );
        // Dim hints when this session isn't the one currently receiving
        // input (host descended into a nested child, or a nested session
        // not yet ascended into). See `dim_amount`.
        self.dim_when_unfocused = config::bool_option(
            configuration,
            CONFIG_DIM_WHEN_UNFOCUSED,
            DEFAULT_DIM_WHEN_UNFOCUSED,
        );
        // Clamped: the dim blend overshoots past neutral gray above
        // 1.0, and inverts the blend direction below 0.0.
        self.dim_strength =
            config::fraction_option(configuration, CONFIG_DIM_STRENGTH, DEFAULT_DIM_STRENGTH);
        // Prefix the hints line with the current mode. Off by default since
        // most setups already get a mode indicator from zjstatus's own
        // {mode} widget on the same line; this exists for standing alone.
        self.show_mode = config::bool_option(configuration, CONFIG_SHOW_MODE, DEFAULT_SHOW_MODE);
        // Optional zjstatus-style format string for the mode prefix, with a
        // `{mode}` placeholder. When unset, the theme's ribbon_selected pill
        // style is used (see `style_mode`).
        self.mode_format = config::non_empty_string_option(configuration, CONFIG_MODE_FORMAT);
        // Fit the hints to the terminal, dropping the trailing ones as it
        // narrows instead of overflowing off the edge.
        self.auto_width = config::bool_option(configuration, CONFIG_AUTO_WIDTH, true);
        // Columns to leave free for whatever else shares the bar — zjstatus's
        // `format_right`, typically, which the plugin cannot see.
        self.reserve_columns = config::usize_option(configuration, CONFIG_RESERVE_COLUMNS, 0);
        // Nerd Font glyphs are East Asian Ambiguous. A terminal set up to show
        // them draws two columns, so measuring them as one makes the plugin
        // think the line fits when it overflows.
        self.wide_ambiguous = config::wide_ambiguous(configuration);
        // Optional zjstatus-style format strings for the key and description
        // parts of each hint. When unset, the theme palette is used (see
        // `style_key_with_modifier` / `style_description`). An empty string is
        // treated as unset so the default styling still applies.
        self.key_format = config::non_empty_string_option(configuration, CONFIG_KEY_FORMAT);
        self.desc_format = config::non_empty_string_option(configuration, CONFIG_DESC_FORMAT);
        // Separator drawn between hints. Also a format string, so it can be a
        // styled glyph (`#[fg=$grey] | `) and not just whitespace.
        self.hint_spacer = config::non_empty_string_option(configuration, CONFIG_HINT_SPACER);
        // Clickable hints, a right-click context menu, and menus of the hints
        // that did not fit. Read before `drop_indicator`, whose default
        // depends on it.
        self.mouse = config::bool_option(configuration, CONFIG_MOUSE, DEFAULT_MOUSE);
        // Marks where hints were dropped to fit the window. Also a format
        // string, so it can be a styled glyph rather than bare text.
        self.drop_indicator = drop_indicator_from_config(configuration, self.mouse);
        // Clicks on a nested session's bar, which need `mouse` for the rest.
        self.mouse_nested = self.mouse
            && config::bool_option(configuration, CONFIG_MOUSE_NESTED, DEFAULT_MOUSE_NESTED);
        // The preview on hover and the menu on a click, which need `mouse`.
        self.mouse_hover = self.mouse
            && config::bool_option(configuration, CONFIG_MOUSE_HOVER, DEFAULT_MOUSE_HOVER);
        self.mouse_menus = self.mouse
            && config::bool_option(configuration, CONFIG_MOUSE_MENUS, DEFAULT_MOUSE_MENUS);
        // When enabled (the default), every enabled keybinding in a mode is
        // shown, not just the curated set. See `add_discovered_hints`.
        self.discover_hints =
            config::bool_option(configuration, CONFIG_DISCOVER_HINTS, DEFAULT_DISCOVER_HINTS);
        // When a hint is bound to both hjkl and the arrow keys, optionally show
        // only one family (`arrows` / `letters`); defaults to showing both.
        self.direction_keys = configuration
            .get(CONFIG_DIRECTION_KEYS)
            .map(|s| DirectionKeys::from_config(s))
            .unwrap_or_default();
        // Keyboard layout used to order the keys within a hint. Only affects
        // ordering, never which keys are shown.
        self.key_order = configuration
            .get(CONFIG_KEY_ORDER)
            .map(|s| KeyOrder::from_config(s))
            .unwrap_or_default();
        // Pin chosen hints to the start or end of each mode, around a `*` that
        // stands for everything left unlisted.
        self.hint_order = configuration
            .get(CONFIG_HINT_ORDER)
            .map(|s| HintOrder::from_config(s))
            .unwrap_or_default();
        // Which pinned group is held onto longest once the `*` is exhausted.
        self.hint_precedence = configuration
            .get(CONFIG_HINT_PRECEDENCE)
            .map(|s| Precedence::from_config(s))
            .unwrap_or_default();
        // Suppress bindings every mode inherits from the base mode, so each mode
        // only advertises what is actually new in it.
        self.hide_shared_hints = config::bool_option(
            configuration,
            CONFIG_HIDE_SHARED_HINTS,
            DEFAULT_HIDE_SHARED_HINTS,
        );
    }
}

impl State {
    /// Act on what `BarCore` left to do with an event or a message. Returns
    /// whether the bar should redraw.
    fn follow(&mut self, handled: Handled) -> bool {
        match handled {
            Handled::Nothing => false,
            Handled::Redraw => true,
            Handled::Reconfigure(configuration) => {
                let had_mouse = self.mouse;
                self.apply_configuration(configuration);
                if self.mouse && !had_mouse {
                    self.bar.enable_mouse(&self.host, &BAR);
                }
                true
            }
        }
    }
}

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.role = Role::load(&BAR, &configuration);
        if !self.role.is_bar() {
            return;
        }
        self.initialized = false;
        self.apply_configuration(configuration);
        self.bar.load(&self.host, &BAR, self.mouse);
    }

    fn update(&mut self, event: Event) -> bool {
        if !self.role.is_bar() {
            return self.role.update(event);
        }
        let mut should_render = !self.initialized;
        let handled = self.bar.update(&self.host, &BAR, &event);
        should_render |= self.follow(handled);
        match event {
            Event::ModeUpdate(mut mode_info) => {
                // Taken out before comparing, so the comparison is between two
                // reports that both lack the table.
                let kept_keybinds = (self.keybinds_arrive_separately
                    && mode_info.keybinds.is_empty())
                .then(|| std::mem::take(&mut self.mode_info.keybinds));
                if self.mode_info != mode_info {
                    should_render = true;
                }
                if let Some(keybinds) = kept_keybinds {
                    mode_info.keybinds = keybinds;
                }
                self.mode_info = mode_info;
                self.base_mode_is_locked = self.mode_info.base_mode == Some(InputMode::Locked);
            }
            // Sent once this plugin subscribes, and again whenever the
            // keybindings are reconfigured.
            Event::InitialKeybinds(keybinds) => {
                self.keybinds_arrive_separately = true;
                if self.mode_info.keybinds != keybinds {
                    self.mode_info.keybinds = keybinds;
                    should_render = true;
                }
            }
            // A bar feeding zjstatus runs in a suppressed pane whose size says
            // nothing about the status bar, so pane geometry is its only view
            // of the terminal's width. A bar in a visible pane of its own
            // fits that pane instead, which `BarCore` keeps.
            Event::PaneUpdate(manifest) => {
                let width = terminal_width(&manifest);
                if width.is_some() && width != self.terminal_width {
                    self.terminal_width = width;
                    should_render = true;
                }
                let focus = focus_by_tab(&manifest);
                if focus != self.focus_by_tab {
                    self.focus_by_tab = focus;
                    should_render = true;
                }
                self.forget_closed_nested_guests(&manifest);
            }
            // Which tab is focused, and whether its floating layer is up.
            // `PaneUpdate` reports focus per tab and per layer, so neither
            // event on its own says where the keyboard actually goes.
            Event::TabUpdate(tabs) => {
                let active_tab = ActiveTab::of(&tabs);
                if active_tab != self.active_tab {
                    self.active_tab = active_tab;
                    should_render = true;
                }
            }
            // A nested session in one of this session's panes reporting its
            // input mode, sent on contact and on every mode change it makes.
            // A nested session that is itself descended reports the mode of
            // the session it is descended into, however deep that goes.
            Event::NestedSessionModeUpdate {
                pane_id,
                session_path,
                mode,
                base_mode,
                keybinds_generation,
            } => {
                should_render |= self.record_nested_guest(
                    pane_id,
                    session_path,
                    mode,
                    base_mode,
                    keybinds_generation,
                );
            }
            // The nested session exited, or stopped answering. Either way its
            // hints can no longer be trusted, and a later session in the same
            // pane will make contact afresh.
            Event::NestedSessionEnded { pane_id, .. } => {
                should_render |= self.nested_guests.remove(&pane_id).is_some();
            }
            // Still subscribed after `mouse` is turned off by a preview, so
            // checked here as well.
            Event::Mouse(mouse) if self.mouse => {
                self.follow_mouse_with_preview(&mouse);
                self.perform_mouse_outcome(self.bar_click_outcome(&mouse));
            }
            Event::Timer(_) => {
                let step = self.preview_memory.timer_fired();
                self.take_preview_step(step);
            }
            Event::PromptResult(request_id, result) => {
                let entries = self.pending_menus.remove(&request_id);
                let outcome = mouse::menu_answer_outcome(entries, &result);
                self.perform_mouse_outcome(outcome);
            }
            Event::ActionComplete(_, _, context) if context.contains_key(MOUSE_ACTION_CONTEXT) => {
                let next = self.action_queue.completed();
                self.run_mouse_action(next);
            }
            _ => {}
        };
        should_render
    }

    fn pipe(&mut self, message: PipeMessage) -> bool {
        if !self.role.is_bar() {
            self.role.pipe(&self.host, &BAR, &message);
            return false;
        }
        if let Some(handled) = self.bar.pipe(&self.host, &BAR, &message) {
            return self.follow(handled);
        }
        self.take_popup_message(&message);
        false
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if !self.role.is_bar() {
            self.role.render(rows, cols);
            return;
        }
        let (output, click_map) = self.draw_bar();
        let mode_info = &self.mode_info;

        // HACK: Because we're not sure when zjstatus will be ready to receive messages,
        // we'll repeatedly send messages until the user has switched to a different mode,
        // at which point we'll assume that zjstatus has been initialized. The render function
        // does not seem to be called too frequently, so this should be fine.
        if !output.is_empty() && Some(mode_info.mode) != mode_info.base_mode {
            self.initialized = true;
        }

        self.sync_collapsed(output.is_empty());

        for pipe_name in &self.pipe_names {
            self.host.broadcast(
                ZJSTATUS_PIPE_MESSAGE,
                format!("zjstatus::pipe::pipe_{}::{}", pipe_name, output),
            );
        }
        print!("{}", output);
        // A preview of a hint that is no longer there would mislead.
        if click_map != self.click_map {
            let step = self.preview_memory.dismiss();
            self.take_preview_step(step);
        }
        self.click_map = click_map;
        self.pane_rows = rows;
    }
}

#[cfg(test)]
mod tests;
