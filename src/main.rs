use ansi_term::{
    ANSIString, ANSIStrings,
    Colour::{Fixed, RGB},
    Style,
};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use unicode_width::UnicodeWidthChar;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::actions::SearchDirection;
use zellij_tile::prelude::*;
use zellij_tile_utils::palette_match;

mod format;

#[derive(Default)]
struct State {
    initialized: bool,
    /// The zjstatus pipe names each render is published on. See
    /// [`pipe_names_from_config`].
    pipe_names: Vec<String>,
    mode_info: ModeInfo,
    base_mode_is_locked: bool,
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
    hint_precedence: HintPrecedence,
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
    /// What Zellij was last told about this pane, so the collapse is only sent
    /// when it changes rather than on every render. Starts out matching how
    /// Zellij places the pane, which is expanded.
    collapsed: bool,
    dim_when_unfocused: bool,
    /// How strongly to dim, in `dim_color`'s `0.0..=1.0` scale.
    dim_strength: f32,
    /// Prefix the hints line with the current mode, so this plugin can stand
    /// on its own in a pane without needing zjstatus's own `{mode}` widget
    /// alongside it. See `render_mode_prefix`.
    show_mode: bool,
    mode_format: Option<String>,
    /// What each nested session hosted in one of this session's panes reports
    /// about itself, keyed by the pane it runs in so several of them never
    /// overwrite each other. Populated from `NestedSessionModeUpdate` and
    /// `NestedSessionKeybinds`; read back by `descended_guest` while this
    /// session is descended into one of them.
    nested_guests: BTreeMap<PaneId, NestedGuest>,
    /// Which pane holds the focus in each tab, learned from `PaneUpdate`.
    focus_by_tab: BTreeMap<usize, TabFocus>,
    /// The focused tab, learned from `TabUpdate`. `None` until the first one
    /// arrives.
    active_tab: Option<ActiveTab>,
    config: BTreeMap<String, String>,
}

/// Where the focus sits in one tab.
///
/// Zellij tracks focus per layer, so a tab can have a focused tiled pane and a
/// focused floating pane at once. Which of the two actually has the keyboard
/// depends on whether the floating layer is up, and only `TabUpdate` reports
/// that, so both are kept here and `focused_pane_id` picks between them.
#[derive(Default, Clone, PartialEq)]
struct TabFocus {
    tiled: Option<PaneId>,
    floating: Option<PaneId>,
}

/// The focused tab, and whether its floating layer is up.
#[derive(Default, Clone, Copy, PartialEq)]
struct ActiveTab {
    position: usize,
    floating_panes_visible: bool,
}

/// What a nested session running in one of this session's panes has told us
/// about itself.
///
/// `mode` and `base_mode` arrive unprompted, once when the nested session first
/// makes contact and again on every mode change it makes, so they are current
/// by the time the user descends into it. `keybinds` is the expensive half and
/// arrives only in answer to a request, which is what `keybinds_requested`
/// guards: the request goes out once per session, not on every event or render.
#[derive(Default, Clone, PartialEq)]
struct NestedGuest {
    session_name: Option<String>,
    mode: InputMode,
    base_mode: Option<InputMode>,
    keybinds: KeybindsVec,
    keybinds_requested: bool,
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

/// The pane id Zellij events use to refer to a pane, which `PaneInfo` carries
/// split across two fields.
fn pane_id_of(pane: &PaneInfo) -> PaneId {
    if pane.is_plugin {
        PaneId::Plugin(pane.id)
    } else {
        PaneId::Terminal(pane.id)
    }
}

/// Which pane holds the focus in each tab of a pane manifest.
///
/// Suppressed panes are skipped: they keep whatever focus flag they had when
/// they were hidden, and a hidden pane is not where the keyboard goes.
fn focus_by_tab(manifest: &PaneManifest) -> BTreeMap<usize, TabFocus> {
    let mut focus_by_tab = BTreeMap::new();
    for (tab_position, panes) in &manifest.panes {
        let mut focus = TabFocus::default();
        for pane in panes
            .iter()
            .filter(|pane| pane.is_focused && !pane.is_suppressed)
        {
            if pane.is_floating {
                focus.floating = Some(pane_id_of(pane));
            } else {
                focus.tiled = Some(pane_id_of(pane));
            }
        }
        if focus != TabFocus::default() {
            focus_by_tab.insert(*tab_position, focus);
        }
    }
    focus_by_tab
}

/// The terminal's width, taken as the right edge of the widest visible pane.
///
/// Tiled panes tile the whole terminal, so the largest `pane_x + pane_columns`
/// among them is its width. Two kinds are skipped:
///
/// - **Floating** panes sit within the terminal but outside the tiling, so
///   their edge says nothing about its width.
/// - **Suppressed** panes are not on screen and stop tracking resizes. Their
///   geometry is whatever it was when they were hidden, and being stale it is
///   often the largest — which would pin the measurement to an old width and
///   silently stop the hints from ever being refitted.
fn terminal_width(manifest: &PaneManifest) -> Option<usize> {
    manifest
        .panes
        .values()
        .flatten()
        .filter(|pane| !pane.is_floating && !pane.is_suppressed)
        .map(|pane| pane.pane_x + pane.pane_columns)
        .max()
        .filter(|width| *width > 0)
}

/// SGR reset. Terminates a truncated styled run so its colours stop at the cut.
const ANSI_RESET: &str = "\u{1b}[0m";

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
const CONFIG_HINT_PRECEDENCE: &str = "hint_precedence";
const CONFIG_AUTO_WIDTH: &str = "auto_width";
const CONFIG_RESERVE_COLUMNS: &str = "reserve_columns";
const CONFIG_AMBIGUOUS_WIDTH: &str = "ambiguous_width";
const CONFIG_HIDE_SHARED_HINTS: &str = "hide_shared_hints";
const CONFIG_HIDE_WHEN_NESTED: &str = "hide_when_nested";
const CONFIG_COLLAPSE_WHEN_EMPTY: &str = "collapse_when_empty";
const CONFIG_DIM_WHEN_UNFOCUSED: &str = "dim_when_unfocused";
const CONFIG_DIM_STRENGTH: &str = "dim_strength";
const CONFIG_SHOW_MODE: &str = "show_mode";
const CONFIG_MODE_FORMAT: &str = "mode_format";
// Per-mode override of the mode prefix, e.g. `mode_format_normal`. Matches
// zjstatus's own `mode_<suffix>` naming exactly (see `mode_config_suffix`),
// so an icon set up for zjstatus's {mode} widget can be reused here.
const CONFIG_MODE_FORMAT_PREFIX: &str = "mode_format_";

/// Concept id of the synthetic hint standing in for the descended-session
/// placeholder (see `render`'s `is_host_descended` branch), and its default
/// label. Rendering it as an ordinary `Hint` through `render_hint` means
/// `key_format`/`desc_format`/`label_descended`/`key_format_descended`/
/// `desc_format_descended`/`keys_descended` all apply to it exactly as they
/// would to any other hint, including key and modifier aliases, with no
/// bespoke styling path of its own to keep in sync.
const DESCENDED_HINT_ID: &str = "descended";
const DESCENDED_HINT_LABEL: &str = "return to host";

// The curated list alone is the readable default; discovery is comprehensive but
// long, and on a narrow bar the extra hints are the first to be dropped anyway.
const DEFAULT_DISCOVER_HINTS: bool = false;
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
    fn group(&self, hint: &Hint) -> HintGroup {
        if Self::position(&self.first, hint).is_some() {
            HintGroup::Leading
        } else if Self::position(&self.last, hint).is_some() {
            HintGroup::Trailing
        } else {
            HintGroup::Middle
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

    /// Entries name a hint by concept id or by the label it displays. Matching
    /// the label too is what keeps relabeled hints addressable — a hint fused by
    /// a shared label has the internal id `=swap layout`, which no one wants to
    /// type.
    fn position(entries: &[String], hint: &Hint) -> Option<usize> {
        let id = hint.id.to_lowercase();
        let label = hint.label.to_lowercase();
        entries
            .iter()
            .position(|entry| *entry == id || *entry == label)
    }
}

/// Which pinned group has precedence — is kept longest — when hints must be
/// given up. Named for zjstatus's `format_precedence`, and read the same way:
/// the groups in the order they are held onto.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum HintPrecedence {
    /// `"tl"` — the trailing group outlives the leading one.
    #[default]
    Trailing,
    /// `"lt"` — the leading group outlives the trailing one.
    Leading,
}

impl HintPrecedence {
    fn from_config(value: &str) -> Self {
        match value.to_lowercase().trim() {
            "lt" => Self::Leading,
            _ => Self::Trailing,
        }
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
    /// Rendered between consecutive hints (never before the first or after the
    /// last). `None` when unset, leaving hints adjacent as before.
    spacer: Option<&'a str>,
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
    precedence: HintPrecedence,
    /// Base-mode keybindings, used to suppress globals that every mode inherits.
    /// Empty when `hide_shared_hints` is off or when rendering the base mode.
    shared: &'a [(KeyWithModifier, Vec<Action>)],
    config: &'a BTreeMap<String, String>,
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
    /// the terminal. With both in play the smaller wins, so an explicit cap is
    /// never exceeded just because the window is wide.
    fn length_limit(&self) -> Option<usize> {
        let explicit = (self.max_length > 0).then_some(self.max_length);
        let auto = self
            .auto_width
            .then_some(self.terminal_width)
            .flatten()
            .map(|width| width.saturating_sub(self.reserve_columns));
        match (explicit, auto) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (limit, None) | (None, limit) => limit,
        }
    }

    /// Whether this session is nested inside another Zellij session.
    fn is_nested(&self) -> bool {
        !self.mode_info.session_ancestry.is_empty()
    }

    /// Whether the host has expanded this session's pane over its whole
    /// display, taking its own status bar off the screen with it.
    ///
    /// Zellij has two fullscreens. The ordinary one (`ToggleFocusFullscreen`)
    /// expands a pane over the viewport only, so the host's tab bar and status
    /// bar stay where they are; Zellij does not report that one here at all.
    /// The other (`ToggleFocusNoUiFullscreen`) expands over the whole display
    /// and hides every other pane, the host's bars included, and that is the
    /// one this reports. It is set only for a nested session, since it arrives
    /// from a host.
    fn host_ui_is_covered(&self) -> bool {
        self.mode_info.host_fullscreen == Some(true)
    }

    /// Whether this session currently has its own focus deferred to a
    /// nested child it is hosting, regardless of whether this session is
    /// itself also nested inside something else. Independent of the
    /// `dim_when_unfocused`/`dim_strength` display options: this is the raw
    /// fact the descended-into indicator (`render_descended_indicator`) acts
    /// on, not a rendering preference.
    fn is_host_descended(&self) -> bool {
        self.mode_info.session_dimmed == Some(true)
    }

    /// The pane the keyboard currently goes to, or `None` while either half of
    /// the picture (`PaneUpdate`, `TabUpdate`) is still missing.
    fn focused_pane_id(&self) -> Option<PaneId> {
        let active_tab = self.active_tab?;
        let focus = self.focus_by_tab.get(&active_tab.position)?;
        if active_tab.floating_panes_visible {
            focus.floating.or(focus.tiled)
        } else {
            focus.tiled
        }
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

    /// Record what a nested session reported about itself, asking it for its
    /// keybindings the first time it speaks.
    ///
    /// `update` applies the report and says whether it changed anything, which
    /// becomes the return value so callers can skip a redraw that would produce
    /// the same bar. It reports that itself rather than being diffed here,
    /// because diffing would mean copying a whole keybinding table on every
    /// mode change to compare against.
    ///
    /// The keybinding request is what makes `descended_guest` eventually
    /// renderable: mode reports arrive on their own, keybindings only on
    /// request.
    fn record_nested_guest(
        &mut self,
        pane_id: PaneId,
        update: impl FnOnce(&mut NestedGuest) -> bool,
    ) -> bool {
        let guest = self.nested_guests.entry(pane_id).or_default();
        let changed = update(guest);
        if !guest.keybinds_requested {
            guest.keybinds_requested = true;
            request_nested_session_keybinds(pane_id);
        }
        changed
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

    /// The dim strength to render with right now, in `dim_color`'s
    /// `0.0..=1.0` scale: `0.0` (no change) unless `dim_when_unfocused` is
    /// enabled and this session is currently the dimmed side of a
    /// nested-session pair, a host that has descended into a child, or a
    /// nested session not currently ascended into.
    /// `session_ascended`/`session_dimmed` are the same fields Zellij's own
    /// bundled tab-bar/compact-bar plugins use for this exact purpose, and
    /// the zjstatus fork's `ZellijState::dim_amount` mirrors this exactly
    /// so a shared layout's two bars dim in step.
    fn dim_amount(&self) -> f32 {
        let is_dimmed = self.mode_info.session_ascended == Some(true)
            || self.mode_info.session_dimmed == Some(true);

        if self.dim_when_unfocused && is_dimmed {
            self.dim_strength
        } else {
            0.0
        }
    }
}

/// Fades a `PaletteColor` toward a dark, desaturated gray by `strength`
/// (`0.0` = unchanged, `1.0` = fully dimmed), mirroring the zjstatus fork's
/// `dim_color`. The actual blend (`format::dim_rgb`) also backs custom
/// `key_format`/`desc_format`/`mode_format` colors, so a hand-styled bar
/// dims the same amount as the theme-palette default. `EightBit` entries
/// pass through unchanged: they're indices into a terminal-defined palette,
/// not RGB triples, so there's no well-defined "dimmed version" of one to
/// compute without also assuming a specific palette.
fn dim_color(color: PaletteColor, strength: f32) -> PaletteColor {
    match color {
        PaletteColor::Rgb((r, g, b)) => {
            let (r, g, b) = format::dim_rgb(r, g, b, strength);
            PaletteColor::Rgb((r, g, b))
        }
        other => other,
    }
}

fn dim_style_declaration(decl: &StyleDeclaration, strength: f32) -> StyleDeclaration {
    StyleDeclaration {
        base: dim_color(decl.base, strength),
        background: dim_color(decl.background, strength),
        emphasis_0: dim_color(decl.emphasis_0, strength),
        emphasis_1: dim_color(decl.emphasis_1, strength),
        emphasis_2: dim_color(decl.emphasis_2, strength),
        emphasis_3: dim_color(decl.emphasis_3, strength),
    }
}

/// A dimmed copy of `colors`. Only the `Styling` groups this plugin's own
/// rendering actually reads are touched, so dimming the rest would be dead
/// work: `ribbon_unselected`/`text_unselected` for hints
/// (`style_key_with_modifier`/`style_key_text`/`style_description`), and
/// `ribbon_selected` for the optional mode prefix (`style_mode`).
fn dim_styling(colors: &Styling, strength: f32) -> Styling {
    Styling {
        ribbon_unselected: dim_style_declaration(&colors.ribbon_unselected, strength),
        ribbon_selected: dim_style_declaration(&colors.ribbon_selected, strength),
        text_unselected: dim_style_declaration(&colors.text_unselected, strength),
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
            set_self_collapsed(collapsed);
        }
    }

    /// What this render has to say about the pane's row, or `None` when it
    /// says the same thing the last one did and there is no point repeating
    /// it. Records the answer as the new last word.
    fn next_collapsed(&mut self, output_is_empty: bool) -> Option<bool> {
        let should_collapse = self.collapse_when_empty && output_is_empty;
        if self.collapsed == should_collapse {
            return None;
        }
        self.collapsed = should_collapse;
        Some(should_collapse)
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
            session_name: guest.session_name.clone(),
            ..Default::default()
        })
    }

    /// The hint line for one session's mode and keybindings, curated, ordered,
    /// styled, dimmed and truncated according to this plugin's configuration.
    ///
    /// `mode_info` is what is being described, which is this session normally
    /// and the nested session it has descended into otherwise. Everything else
    /// comes from this session, because the bar being drawn is this session's.
    fn render_hint_line(&self, mode_info: &ModeInfo, dim: f32) -> String {
        let keymap = get_keymap_for_mode(mode_info);
        // Bindings the base mode already advertises. Every non-base mode
        // inherits these via Zellij's `shared_except` groups, so listing
        // them again in each mode is pure repetition.
        let base_mode = mode_info.base_mode.unwrap_or(InputMode::Normal);
        let shared = if self.hide_shared_hints && mode_info.mode != base_mode {
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
                &self.config,
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
            config: &self.config,
        };
        let mut parts = mode_prefix;
        parts.extend(render_hints_for_mode(mode_info.mode, &keymap, &ctx));

        let ansi_strings = ANSIStrings(&parts);
        let formatted = if leading_space > 0 {
            format!(" {}", ansi_strings)
        } else {
            ansi_strings.to_string()
        };

        let visible_len = calculate_visible_length(&formatted, self.wide_ambiguous);
        match limit {
            Some(limit) if visible_len > limit => {
                truncate_ansi_string(&formatted, &self.overflow_str, limit, self.wide_ambiguous)
            }
            _ => formatted.to_string(),
        }
    }
}

/// The zjstatus pipe names to publish each render on.
///
/// An explicit `pipe_name` is used on its own: someone who has named their
/// pipe has a zjstatus config reading that name and no other. Left unset,
/// both the current default and the pre-rename one go out, so a config
/// written against either keeps rendering.
fn pipe_names_from_config(configuration: &BTreeMap<String, String>) -> Vec<String> {
    match configuration.get("pipe_name") {
        Some(name) => vec![name.clone()],
        None => vec![DEFAULT_PIPE_NAME.to_string(), LEGACY_PIPE_NAME.to_string()],
    }
}

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.initialized = false;

        // TODO: configuration validation
        self.max_length = configuration
            .get("max_length")
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_MAX_LENGTH);
        self.overflow_str = configuration
            .get("overflow_str")
            .cloned()
            .unwrap_or_else(|| DEFAULT_OVERFLOW_STR.to_string());
        self.pipe_names = pipe_names_from_config(&configuration);
        self.hide_in_base_mode = configuration
            .get("hide_in_base_mode")
            .map(|s| s.to_lowercase().parse::<bool>().unwrap_or(false))
            .unwrap_or(false);
        // Render nothing at all when this session is nested (see
        // `is_nested`), so nested sessions get no bottom bar from the same
        // shared config a host session uses.
        self.hide_when_nested = configuration
            .get(CONFIG_HIDE_WHEN_NESTED)
            .map(|s| {
                s.to_lowercase()
                    .parse::<bool>()
                    .unwrap_or(DEFAULT_HIDE_WHEN_NESTED)
            })
            .unwrap_or(DEFAULT_HIDE_WHEN_NESTED);
        // Give the row back to the panes around it while there is nothing on
        // it, rather than drawing a blank one. See `sync_collapsed`.
        self.collapse_when_empty = configuration
            .get(CONFIG_COLLAPSE_WHEN_EMPTY)
            .map(|s| {
                s.to_lowercase()
                    .parse::<bool>()
                    .unwrap_or(DEFAULT_COLLAPSE_WHEN_EMPTY)
            })
            .unwrap_or(DEFAULT_COLLAPSE_WHEN_EMPTY);
        // Dim hints when this session isn't the one currently receiving
        // input (host descended into a nested child, or a nested session
        // not yet ascended into). See `dim_amount`.
        self.dim_when_unfocused = configuration
            .get(CONFIG_DIM_WHEN_UNFOCUSED)
            .map(|s| {
                s.to_lowercase()
                    .parse::<bool>()
                    .unwrap_or(DEFAULT_DIM_WHEN_UNFOCUSED)
            })
            .unwrap_or(DEFAULT_DIM_WHEN_UNFOCUSED);
        // Clamped: dim_color's blend overshoots past neutral gray above
        // 1.0, and inverts the blend direction below 0.0.
        self.dim_strength = configuration
            .get(CONFIG_DIM_STRENGTH)
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(DEFAULT_DIM_STRENGTH)
            .clamp(0.0, 1.0);
        // Prefix the hints line with the current mode. Off by default since
        // most setups already get a mode indicator from zjstatus's own
        // {mode} widget on the same line; this exists for standing alone.
        self.show_mode = configuration
            .get(CONFIG_SHOW_MODE)
            .map(|s| {
                s.to_lowercase()
                    .parse::<bool>()
                    .unwrap_or(DEFAULT_SHOW_MODE)
            })
            .unwrap_or(DEFAULT_SHOW_MODE);
        // Optional zjstatus-style format string for the mode prefix, with a
        // `{mode}` placeholder. When unset, the theme's ribbon_selected pill
        // style is used (see `style_mode`).
        self.mode_format = configuration
            .get(CONFIG_MODE_FORMAT)
            .filter(|s| !s.is_empty())
            .cloned();
        // Fit the hints to the terminal, dropping the trailing ones as it
        // narrows instead of overflowing off the edge.
        self.auto_width = configuration
            .get(CONFIG_AUTO_WIDTH)
            .map(|s| s.to_lowercase().parse::<bool>().unwrap_or(true))
            .unwrap_or(true);
        // Columns to leave free for whatever else shares the bar — zjstatus's
        // `format_right`, typically, which the plugin cannot see.
        self.reserve_columns = configuration
            .get(CONFIG_RESERVE_COLUMNS)
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        // Nerd Font glyphs are East Asian Ambiguous. A terminal set up to show
        // them draws two columns, so measuring them as one makes the plugin
        // think the line fits when it overflows.
        self.wide_ambiguous = configuration
            .get(CONFIG_AMBIGUOUS_WIDTH)
            .map(|s| s.trim() == "2")
            .unwrap_or(false);
        // Optional zjstatus-style format strings for the key and description
        // parts of each hint. When unset, the theme palette is used (see
        // `style_key_with_modifier` / `style_description`). An empty string is
        // treated as unset so the default styling still applies.
        self.key_format = configuration
            .get(CONFIG_KEY_FORMAT)
            .filter(|s| !s.is_empty())
            .cloned();
        self.desc_format = configuration
            .get(CONFIG_DESC_FORMAT)
            .filter(|s| !s.is_empty())
            .cloned();
        // Separator drawn between hints. Also a format string, so it can be a
        // styled glyph (`#[fg=$grey] | `) and not just whitespace.
        self.hint_spacer = configuration
            .get(CONFIG_HINT_SPACER)
            .filter(|s| !s.is_empty())
            .cloned();
        // Marks where hints were dropped to fit the window. Also a format
        // string, so it can be a styled glyph rather than bare text.
        self.drop_indicator = configuration
            .get(CONFIG_DROP_INDICATOR)
            .filter(|s| !s.is_empty())
            .cloned();
        // When enabled (the default), every enabled keybinding in a mode is
        // shown, not just the curated set. See `add_discovered_hints`.
        self.discover_hints = configuration
            .get(CONFIG_DISCOVER_HINTS)
            .map(|s| {
                s.to_lowercase()
                    .parse::<bool>()
                    .unwrap_or(DEFAULT_DISCOVER_HINTS)
            })
            .unwrap_or(DEFAULT_DISCOVER_HINTS);
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
            .map(|s| HintPrecedence::from_config(s))
            .unwrap_or_default();
        // Suppress bindings every mode inherits from the base mode, so each mode
        // only advertises what is actually new in it.
        self.hide_shared_hints = configuration
            .get(CONFIG_HIDE_SHARED_HINTS)
            .map(|s| {
                s.to_lowercase()
                    .parse::<bool>()
                    .unwrap_or(DEFAULT_HIDE_SHARED_HINTS)
            })
            .unwrap_or(DEFAULT_HIDE_SHARED_HINTS);
        // Retained so `$alias` colors and `label_<action>` overrides can be
        // resolved from the raw configuration.
        self.config = configuration;

        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::MessageAndLaunchOtherPlugins,
        ]);

        // Deliberately still selectable at this point.
        //
        // Zellij draws the permission prompt in the plugin's own pane, and a
        // pane that is not selectable cannot be focused — so making this
        // unselectable before the prompt is answered leaves the user unable to
        // answer it at all. The pane is a one-line status bar, so the prompt
        // has nowhere to go and no way to be reached.
        //
        // `set_selectable(false)` happens once the result arrives instead. That
        // is safe for an already-granted plugin too: `request_permission`
        // replies immediately from the permission cache rather than staying
        // silent, so the event always comes.
        subscribe(&[
            EventType::ModeUpdate,
            EventType::SessionUpdate,
            EventType::PaneUpdate,
            EventType::PermissionRequestResult,
            EventType::TabUpdate,
            EventType::NestedSessionModeUpdate,
            EventType::NestedSessionKeybinds,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = !self.initialized;
        match event {
            Event::ModeUpdate(mode_info) => {
                if self.mode_info != mode_info {
                    should_render = true;
                }
                self.mode_info = mode_info;
                self.base_mode_is_locked = self.mode_info.base_mode == Some(InputMode::Locked);
            }
            // The plugin runs headless, so its own `render` dimensions say
            // nothing about the status bar. Pane geometry is the only view of
            // the terminal's width it gets.
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
                let active_tab = tabs.iter().find(|tab| tab.active).map(|tab| ActiveTab {
                    position: tab.position,
                    floating_panes_visible: tab.are_floating_panes_visible,
                });
                if active_tab != self.active_tab {
                    self.active_tab = active_tab;
                    should_render = true;
                }
            }
            // A nested session in one of this session's panes reporting its
            // input mode, sent on contact and on every mode change it makes.
            Event::NestedSessionModeUpdate {
                pane_id,
                session_name,
                mode,
                base_mode,
            } => {
                should_render |= self.record_nested_guest(pane_id, |guest| {
                    let changed = guest.session_name != session_name
                        || guest.mode != mode
                        || guest.base_mode != base_mode;
                    guest.session_name = session_name;
                    guest.mode = mode;
                    guest.base_mode = base_mode;
                    changed
                });
            }
            // A nested session answering the keybinding request that
            // `record_nested_guest` issued when it first spoke.
            Event::NestedSessionKeybinds {
                pane_id,
                session_name,
                keybinds,
            } => {
                should_render |= self.record_nested_guest(pane_id, |guest| {
                    let changed = guest.session_name != session_name || guest.keybinds != keybinds;
                    guest.session_name = session_name;
                    guest.keybinds = keybinds;
                    changed
                });
            }
            // Answered, or granted from the cache. Either way the prompt is
            // gone and the pane has no further reason to take focus.
            Event::PermissionRequestResult(_) => {
                set_selectable(false);
                should_render = true;
            }
            _ => {}
        };
        should_render
    }

    fn render(&mut self, _rows: usize, _cols: usize) {
        let mode_info = &self.mode_info;
        let dim = self.dim_amount();

        let output = match self.bar_subject() {
            BarSubject::Nothing => String::new(),
            BarSubject::Hints(subject) => self.render_hint_line(&subject, dim),
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
                    precedence: HintPrecedence::default(),
                    shared: &[],
                    config: &self.config,
                };
                let styled_indicator = render_descended_indicator(mode_info, &indicator_style);
                let styled = if styled_indicator.is_empty() {
                    String::new()
                } else if self.show_mode {
                    let mode_prefix = render_mode_prefix(
                        mode_info.mode,
                        colors,
                        self.mode_format.as_deref(),
                        &self.config,
                        dim,
                    );
                    format!("{}{}", ANSIStrings(&mode_prefix), styled_indicator)
                } else {
                    styled_indicator
                };
                let limit = self.length_limit();
                let visible_len = calculate_visible_length(&styled, self.wide_ambiguous);
                match limit {
                    Some(limit) if visible_len > limit => truncate_ansi_string(
                        &styled,
                        &self.overflow_str,
                        limit,
                        self.wide_ambiguous,
                    ),
                    _ => styled,
                }
            }
        };

        // HACK: Because we're not sure when zjstatus will be ready to receive messages,
        // we'll repeatedly send messages until the user has switched to a different mode,
        // at which point we'll assume that zjstatus has been initialized. The render function
        // does not seem to be called too frequently, so this should be fine.
        if !output.is_empty() && Some(mode_info.mode) != mode_info.base_mode {
            self.initialized = true;
        }

        self.sync_collapsed(output.is_empty());

        for pipe_name in &self.pipe_names {
            pipe_message_to_plugin(
                MessageToPlugin::new("pipe")
                    .with_payload(format!("zjstatus::pipe::pipe_{}::{}", pipe_name, output)),
            );
        }
        print!("{}", output);
    }
}

struct AnsiParser<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> AnsiParser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            chars: text.chars().peekable(),
        }
    }

    fn next_segment(&mut self) -> Option<AnsiSegment> {
        let ch = self.chars.next()?;

        if ch == '\x1b' {
            let mut escape_seq = String::from(ch);
            for escape_ch in self.chars.by_ref() {
                escape_seq.push(escape_ch);
                if escape_ch == 'm' {
                    break;
                }
            }
            Some(AnsiSegment::EscapeSequence(escape_seq))
        } else {
            Some(AnsiSegment::VisibleChar(ch))
        }
    }
}

enum AnsiSegment {
    EscapeSequence(String),
    VisibleChar(char),
}

/// Columns the text occupies on screen, ignoring escape sequences.
///
/// Counting characters is not the same as counting columns: a CJK ideograph or
/// an emoji takes two. `wide_ambiguous` decides the East Asian Ambiguous class,
/// which includes every Nerd Font glyph — narrow by the standard, but two
/// columns in a terminal actually configured to show them.
fn calculate_visible_length(text: &str, wide_ambiguous: bool) -> usize {
    let mut parser = AnsiParser::new(text);
    let mut len = 0;

    while let Some(segment) = parser.next_segment() {
        if let AnsiSegment::VisibleChar(ch) = segment {
            len += char_width(ch, wide_ambiguous);
        }
    }

    len
}

fn char_width(ch: char, wide_ambiguous: bool) -> usize {
    let width = if wide_ambiguous {
        ch.width_cjk()
    } else {
        ch.width()
    };
    // Control characters report no width; treat them as taking none.
    width.unwrap_or(0)
}

fn truncate_ansi_string(
    text: &str,
    overflow_str: &str,
    max_len: usize,
    wide_ambiguous: bool,
) -> String {
    let visible_len = calculate_visible_length(text, wide_ambiguous);
    // Width on screen, not bytes: an overflow marker like `…` is one column.
    let overflow_len = overflow_str.chars().count();

    if visible_len <= max_len {
        return text.to_string();
    }

    // Too narrow for even the marker, so show as much of it as fits rather than
    // overshooting the limit the caller asked for.
    if max_len <= overflow_len {
        return overflow_str.chars().take(max_len).collect();
    }

    let target_len = max_len - overflow_len;
    let mut result = String::new();
    let mut visible_count = 0;
    let mut parser = AnsiParser::new(text);

    while let Some(segment) = parser.next_segment() {
        match segment {
            AnsiSegment::EscapeSequence(seq) => {
                result.push_str(&seq);
            }
            AnsiSegment::VisibleChar(ch) => {
                if visible_count >= target_len {
                    break;
                }
                result.push(ch);
                visible_count += 1;
            }
        }
    }

    result.push_str(overflow_str);
    // `ANSIStrings` closes the styled run with a reset, which the loop above
    // discards by breaking early. Without one, whatever colour was active at the
    // cut keeps painting the rest of the status bar.
    if !result.ends_with(ANSI_RESET) {
        result.push_str(ANSI_RESET);
    }
    result
}

fn find_keys_for_actions(
    keymap: &[(KeyWithModifier, Vec<Action>)],
    target_actions: &[Action],
    exact_match: bool,
) -> Vec<KeyWithModifier> {
    keymap
        .iter()
        .filter_map(|(key, key_actions)| {
            if exact_match {
                let matching = key_actions
                    .iter()
                    .zip(target_actions)
                    .filter(|(a, b)| a.shallow_eq(b))
                    .count();
                if matching == key_actions.len() && matching == target_actions.len() {
                    Some(key.clone())
                } else {
                    None
                }
            } else if key_actions.iter().next() == target_actions.iter().next() {
                Some(key.clone())
            } else {
                None
            }
        })
        .collect()
}

fn find_keys_for_action_groups(
    keymap: &[(KeyWithModifier, Vec<Action>)],
    action_groups: &[&[Action]],
) -> Vec<KeyWithModifier> {
    action_groups
        .iter()
        .flat_map(|actions| find_keys_for_actions(keymap, actions, true))
        .collect()
}

/// A run of keys that share the exact same set of modifiers, so the modifier
/// can be factored out and shown once (e.g. `Ctrl+h` and `Ctrl+k` collapse to
/// `^h|k`). `modifiers` is in canonical order (Ctrl, Alt, Shift, Super) since
/// `key_modifiers` is a `BTreeSet`.
struct KeyGroup {
    modifiers: Vec<KeyModifier>,
    keys: Vec<String>,
}

/// Partition a hint's key bindings into groups sharing the same modifiers,
/// preserving first-seen order of both groups and keys. This is what lets
/// `Ctrl h|Alt <|Ctrl k|Alt l` collapse to `^h|k ⌥<|l`.
fn group_keys(
    key_bindings: &[KeyWithModifier],
    config: &BTreeMap<String, String>,
) -> Vec<KeyGroup> {
    let mut groups: Vec<KeyGroup> = vec![];
    for key in key_bindings {
        let modifiers: Vec<KeyModifier> = key.key_modifiers.iter().copied().collect();
        let display = format_bare_key(&key.bare_key, config);
        if let Some(group) = groups.iter_mut().find(|g| g.modifiers == modifiers) {
            group.keys.push(display);
        } else {
            groups.push(KeyGroup {
                modifiers,
                keys: vec![display],
            });
        }
    }
    groups
}

/// Render a modifier alias for display, e.g. `Ctrl` -> `^` when
/// `mod_alias_ctrl "^"` is configured. Without an alias, the modifier's normal
/// name is used (`Ctrl`, `Alt`, `Shift`, `Super`).
fn modifier_display(modifier: KeyModifier, config: &BTreeMap<String, String>) -> String {
    let name = modifier.to_string();
    config
        .get(&format!(
            "{}{}",
            CONFIG_MOD_ALIAS_PREFIX,
            name.to_lowercase()
        ))
        .cloned()
        .unwrap_or(name)
}

/// The prefix shown before a group's keys, including its trailing separator. A
/// word-like modifier (`Ctrl`) is followed by a space (`Ctrl h`); a symbol
/// alias (`^`) hugs the keys (`^h`). An empty modifier set yields no prefix.
/// A group whose modifiers exactly match a configured chord (see
/// `chord_alias`) uses that alias in place of the per-modifier names.
fn group_prefix(modifiers: &[KeyModifier], config: &BTreeMap<String, String>) -> String {
    if modifiers.is_empty() {
        return String::new();
    }
    let joined = chord_alias(modifiers, config).unwrap_or_else(|| {
        modifiers
            .iter()
            .map(|m| modifier_display(*m, config))
            .collect()
    });
    if joined
        .chars()
        .next_back()
        .is_some_and(|c| c.is_alphanumeric())
    {
        format!("{} ", joined)
    } else {
        joined
    }
}

/// Parse one of the four modifier names accepted by `mod_alias_<name>`
/// (`ctrl`, `alt`, `shift`, `super`) back into a `KeyModifier`.
fn parse_modifier_name(name: &str) -> Option<KeyModifier> {
    match name.trim().to_lowercase().as_str() {
        "ctrl" => Some(KeyModifier::Ctrl),
        "alt" => Some(KeyModifier::Alt),
        "shift" => Some(KeyModifier::Shift),
        "super" => Some(KeyModifier::Super),
        _ => None,
    }
}

/// Parse a `chord_mods_<name>` value, e.g. `"ctrl+alt+super+shift"`, into the
/// set of modifiers it names. `None` if any token is unrecognized, so a typo
/// silently disables that one chord rather than matching the wrong
/// combination.
fn parse_chord_mods(value: &str) -> Option<BTreeSet<KeyModifier>> {
    value.split('+').map(parse_modifier_name).collect()
}

/// Look up a configured chord alias for an exact modifier combination. Named
/// pairs are set via `chord_mods_<name>` (the combination) and
/// `chord_alias_<name>` (the symbol shown instead), letting a custom leader
/// key's full chord collapse to one glyph everywhere it appears, e.g. mapping
/// `Ctrl+Alt+Super+Shift` to `&`. A `chord_mods_<name>` with no matching
/// `chord_alias_<name>` has no effect. Matches only the exact set — a hint
/// bound to part of the chord is unaffected.
fn chord_alias(modifiers: &[KeyModifier], config: &BTreeMap<String, String>) -> Option<String> {
    if modifiers.is_empty() {
        return None;
    }
    let wanted: BTreeSet<KeyModifier> = modifiers.iter().copied().collect();
    config
        .iter()
        .filter_map(|(k, v)| Some((k.strip_prefix(CONFIG_CHORD_MODS_PREFIX)?, v)))
        .find(|(_, mods)| parse_chord_mods(mods).as_ref() == Some(&wanted))
        .and_then(|(name, _)| {
            config
                .get(&format!("{}{}", CONFIG_CHORD_ALIAS_PREFIX, name))
                .cloned()
        })
}

/// Render a bare key, substituting a configured alias symbol when one is set.
/// Without an alias this is exactly the key's normal Zellij representation
/// (e.g. `ENTER`, `ESC`, `←`), so behavior is unchanged unless configured.
fn format_bare_key(bare_key: &BareKey, config: &BTreeMap<String, String>) -> String {
    key_alias(bare_key, config).unwrap_or_else(|| bare_key.to_string())
}

/// Look up a configured symbol for `bare_key`. Aliases are set via
/// `key_alias_<name>` options (mirroring zjstatus's `color_<name>` aliases),
/// where `<name>` is a lowercase key name, e.g. `key_alias_enter "↵"`. Several
/// common keys accept a short synonym (e.g. `esc`/`escape`); the first name
/// with a configured value wins.
fn key_alias(bare_key: &BareKey, config: &BTreeMap<String, String>) -> Option<String> {
    key_alias_names(bare_key).into_iter().find_map(|name| {
        config
            .get(&format!("{}{}", CONFIG_KEY_ALIAS_PREFIX, name))
            .cloned()
    })
}

/// The accepted `key_alias_<name>` names for a bare key, in priority order.
fn key_alias_names(bare_key: &BareKey) -> Vec<String> {
    let names: &[&str] = match bare_key {
        BareKey::Enter => &["enter", "return"],
        BareKey::Esc => &["esc", "escape"],
        BareKey::Tab => &["tab"],
        BareKey::Char(' ') => &["space"],
        BareKey::Backspace => &["backspace"],
        BareKey::Delete => &["delete", "del"],
        BareKey::Insert => &["insert", "ins"],
        BareKey::Home => &["home"],
        BareKey::End => &["end"],
        BareKey::PageUp => &["pageup", "pgup"],
        BareKey::PageDown => &["pagedown", "pgdn"],
        BareKey::Up => &["up"],
        BareKey::Down => &["down"],
        BareKey::Left => &["left"],
        BareKey::Right => &["right"],
        BareKey::CapsLock => &["capslock"],
        BareKey::ScrollLock => &["scrolllock"],
        BareKey::NumLock => &["numlock"],
        BareKey::PrintScreen => &["printscreen"],
        BareKey::Pause => &["pause"],
        BareKey::Menu => &["menu"],
        BareKey::F(n) => return vec![format!("f{}", n)],
        // Any other character can be aliased by the character itself.
        BareKey::Char(c) => return vec![c.to_lowercase().to_string()],
    };
    names.iter().map(|s| s.to_string()).collect()
}

fn style_key_with_modifier(
    key_bindings: &[KeyWithModifier],
    palette: &Styling,
    config: &BTreeMap<String, String>,
) -> Vec<ANSIString<'static>> {
    if key_bindings.is_empty() {
        return vec![];
    }

    let saturated_bg = palette_match!(palette.ribbon_unselected.background);
    let contrasting_fg = palette_match!(palette.ribbon_unselected.base);
    let ribbon = || Style::new().fg(contrasting_fg).on(saturated_bg);

    let mut styled_parts = vec![Style::new().paint(" "), ribbon().paint(" ")];

    for (group_idx, group) in group_keys(key_bindings, config).iter().enumerate() {
        if group_idx > 0 {
            styled_parts.push(ribbon().paint(" "));
        }

        let prefix = group_prefix(&group.modifiers, config);
        if !prefix.is_empty() {
            styled_parts.push(ribbon().bold().paint(prefix));
        }

        for key in &group.keys {
            styled_parts.push(ribbon().bold().paint(key.clone()));
        }
    }

    styled_parts.push(ribbon().paint(" "));

    styled_parts
}

/// Style arbitrary text as a key, for hints whose keys have been replaced by a
/// fixed string. Mirrors `style_key_with_modifier` so a replaced key sits in
/// the same ribbon as a real one.
fn style_key_text(text: &str, palette: &Styling) -> Vec<ANSIString<'static>> {
    if text.is_empty() {
        return vec![];
    }
    let saturated_bg = palette_match!(palette.ribbon_unselected.background);
    let contrasting_fg = palette_match!(palette.ribbon_unselected.base);
    let ribbon = Style::new().fg(contrasting_fg).on(saturated_bg);
    vec![
        Style::new().paint(" "),
        ribbon.paint(" ".to_string()),
        ribbon.bold().paint(text.to_string()),
        ribbon.paint(" ".to_string()),
    ]
}

/// Compose the plain-text representation of a key binding as it appears in a
/// hint, e.g. `^p`, `h|j|k|l`, or `←↓↑→`. This is the value substituted for the
/// `{key}` placeholder when a custom `key_format` is configured; the styling
/// itself is left to the format string. Keys sharing a modifier are grouped so
/// the modifier is shown once per group (see `group_keys`).
fn compose_key_text(key_bindings: &[KeyWithModifier], config: &BTreeMap<String, String>) -> String {
    group_keys(key_bindings, config)
        .iter()
        .map(|group| {
            format!(
                "{}{}",
                group_prefix(&group.modifiers, config),
                group.keys.concat()
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Named keys that are not letters, digits or arrows, in rough physical order
/// around the main block. Anything absent sorts after everything listed.
const NAMED_KEY_ORDER: &[BareKey] = &[
    BareKey::Esc,
    BareKey::Tab,
    BareKey::CapsLock,
    BareKey::Enter,
    BareKey::Backspace,
    BareKey::Insert,
    BareKey::Delete,
    BareKey::Home,
    BareKey::End,
    BareKey::PageUp,
    BareKey::PageDown,
    BareKey::PrintScreen,
    BareKey::ScrollLock,
    BareKey::Pause,
    BareKey::NumLock,
    BareKey::Menu,
];

/// Sort a hint's keys into a predictable reading order.
///
/// Modifiers lead the sort so that keys sharing one stay contiguous —
/// `group_keys` renders each modifier group as a single run (`^hjkl`), and
/// interleaving would shatter that into `^h h ^j j`.
fn sort_keys(keys: &mut [KeyWithModifier], order: KeyOrder) {
    if order == KeyOrder::Unsorted {
        return;
    }
    keys.sort_by_key(|key| {
        (
            modifier_rank(&key.key_modifiers),
            modifier_tiebreak(&key.key_modifiers),
            key_rank(&key.bare_key, order),
        )
    });
}

/// Which modifier group a key belongs to: unmodified first, then `Ctrl`,
/// `Super`, `Alt`, `Shift`. A key carrying several modifiers sorts with the
/// strongest one it has, so `Ctrl Shift p` groups under `Ctrl`.
///
/// Unmodified keys lead because they are the plainest way to reach an action,
/// and because a hint too wide to fit is cut from the right — so whatever sorts
/// first is what survives.
fn modifier_rank(modifiers: &BTreeSet<KeyModifier>) -> u8 {
    modifiers
        .iter()
        .map(|modifier| match modifier {
            KeyModifier::Ctrl => 1,
            KeyModifier::Super => 2,
            KeyModifier::Alt => 3,
            KeyModifier::Shift => 4,
        })
        .min()
        .unwrap_or(0)
}

/// Separates combinations that share a `modifier_rank` (`Ctrl` from
/// `Ctrl Shift`) so their grouping stays stable rather than depending on the
/// order Zellij reported them.
fn modifier_tiebreak(modifiers: &BTreeSet<KeyModifier>) -> (usize, u8) {
    (modifiers.len(), modifiers.iter().map(modifier_bit).sum())
}

fn modifier_bit(modifier: &KeyModifier) -> u8 {
    match modifier {
        KeyModifier::Ctrl => 1,
        KeyModifier::Super => 2,
        KeyModifier::Alt => 4,
        KeyModifier::Shift => 8,
    }
}

/// Where a key sits in the reading order, as `(category, row, column)`.
///
/// Categories keep unlike keys from interleaving, so letters gather before
/// punctuation rather than mixing at row boundaries — `opas\`, not `op\as`.
/// Within a category, position on the layout does the rest: function keys and
/// digits by number, letters and punctuation by row then column, arrows in
/// `hjkl` order rather than the `Left, Down, Up, Right` the enum declares.
fn key_rank(key: &BareKey, order: KeyOrder) -> (u8, u32, u32) {
    match key {
        BareKey::F(n) => (0, *n as u32, 0),
        BareKey::Char(c) => {
            let lowered = c.to_ascii_lowercase();
            let category = if lowered.is_ascii_digit() {
                1
            } else if lowered.is_ascii_alphabetic() {
                2
            } else {
                3
            };
            // No layout to consult: order by the character itself, which for
            // ASCII is `0-9` then `a-z`.
            if order == KeyOrder::Alphabetical {
                return (category, lowered as u32, 0);
            }
            for (row, keys) in order.rows().iter().enumerate() {
                if let Some(column) = keys.chars().position(|k| k == lowered) {
                    return (category, row as u32, column as u32);
                }
            }
            // Off-layout characters (space, non-ASCII) keep a stable order of
            // their own rather than landing at row 0 alongside the digits.
            (4, lowered as u32, 0)
        }
        BareKey::Left => (5, 0, 0),
        BareKey::Down => (5, 1, 0),
        BareKey::Up => (5, 2, 0),
        BareKey::Right => (5, 3, 0),
        other => {
            let position = NAMED_KEY_ORDER.iter().position(|k| k == other);
            (6, position.unwrap_or(NAMED_KEY_ORDER.len()) as u32, 0)
        }
    }
}

/// Drop one directional family (hjkl or arrows) when a hint is bound to both,
/// per the `direction_keys` setting. Only reduces when both families are
/// present, so a hint bound to just one is never emptied.
fn filter_direction_keys(
    key_bindings: &[KeyWithModifier],
    preference: DirectionKeys,
) -> Vec<KeyWithModifier> {
    if preference == DirectionKeys::Both {
        return key_bindings.to_vec();
    }

    let has_arrows = key_bindings.iter().any(|k| is_arrow_key(&k.bare_key));
    let has_letters = key_bindings.iter().any(|k| is_hjkl_key(&k.bare_key));
    if !(has_arrows && has_letters) {
        return key_bindings.to_vec();
    }

    key_bindings
        .iter()
        .filter(|k| match preference {
            DirectionKeys::Arrows => !is_hjkl_key(&k.bare_key),
            DirectionKeys::Letters => !is_arrow_key(&k.bare_key),
            DirectionKeys::Both => true,
        })
        .cloned()
        .collect()
}

fn is_arrow_key(bare_key: &BareKey) -> bool {
    matches!(
        bare_key,
        BareKey::Left | BareKey::Right | BareKey::Up | BareKey::Down
    )
}

fn is_hjkl_key(bare_key: &BareKey) -> bool {
    matches!(bare_key, BareKey::Char(c) if matches!(c.to_ascii_lowercase(), 'h' | 'j' | 'k' | 'l'))
}

fn style_description(description: &str, palette: &Styling) -> Vec<ANSIString<'static>> {
    let less_saturated_bg = palette_match!(palette.text_unselected.background);
    let contrasting_fg = palette_match!(palette.text_unselected.base);

    vec![Style::new()
        .fg(contrasting_fg)
        .on(less_saturated_bg)
        .paint(format!(" {} ", description))]
}

/// Renders the current input mode as a styled prefix (e.g. " NORMAL "), shown
/// at the very start of the hints line when `show_mode` is enabled. Exists so
/// this plugin can stand on its own in a pane, with its own mode indicator,
/// instead of relying on zjstatus's `{mode}` widget on the same line: that's
/// the only way to get a mode indicator next to hints piped through a
/// headless setup, which doesn't receive nested-session state (see
/// docs/nested-sessions.md).
///
/// A per-mode override (`mode_format_normal`, `mode_format_locked`, ...)
/// takes precedence over the global `mode_format`, which takes precedence
/// over the built-in theme styling. Per-mode overrides use the same suffix
/// names as zjstatus's own `mode_<suffix>` config keys (see
/// `mode_config_suffix`), so an icon already set up for zjstatus's `{mode}`
/// widget can be copied straight over. `dim` fades either path the same
/// amount: the built-in styling through `colors` (already dimmed by the
/// caller), a custom format string through `format::render_template`, which
/// resolves its own literal colors and would otherwise ignore `colors`
/// (and `dim_when_unfocused`) entirely.
fn render_mode_prefix(
    mode: InputMode,
    colors: &Styling,
    mode_format: Option<&str>,
    config: &BTreeMap<String, String>,
    dim: f32,
) -> Vec<ANSIString<'static>> {
    let mode_name = format!("{:?}", mode).to_uppercase();
    let per_mode_key = format!("{}{}", CONFIG_MODE_FORMAT_PREFIX, mode_config_suffix(mode));
    let format_str = config
        .get(&per_mode_key)
        .map(String::as_str)
        .or(mode_format);

    match format_str {
        Some(fmt) => format::render_template(fmt, &[("mode", &mode_name)], config, dim),
        None => style_mode(&mode_name, colors),
    }
}

/// The zjstatus-style suffix for a mode's per-mode config keys, e.g.
/// `InputMode::EnterSearch` -> `"enter_search"`, matching zjstatus's own
/// `mode_enter_search` naming exactly (see `zjstatus`'s `ModeWidget::new`).
fn mode_config_suffix(mode: InputMode) -> &'static str {
    match mode {
        InputMode::Normal => "normal",
        InputMode::Locked => "locked",
        InputMode::Resize => "resize",
        InputMode::Pane => "pane",
        InputMode::Tab => "tab",
        InputMode::Scroll => "scroll",
        InputMode::EnterSearch => "enter_search",
        InputMode::Search => "search",
        InputMode::RenameTab => "rename_tab",
        InputMode::RenamePane => "rename_pane",
        InputMode::Session => "session",
        InputMode::Move => "move",
        InputMode::Prompt => "prompt",
        InputMode::Tmux => "tmux",
    }
}

/// Default mode styling when neither a per-mode nor a global `mode_format`
/// is set: the theme's `ribbon_selected` pill, the same "currently active"
/// style Zellij's own ribbons use, bold so it reads as a badge rather than
/// plain text.
fn style_mode(mode_name: &str, palette: &Styling) -> Vec<ANSIString<'static>> {
    let bg = palette_match!(palette.ribbon_selected.background);
    let fg = palette_match!(palette.ribbon_selected.base);

    vec![Style::new()
        .fg(fg)
        .on(bg)
        .bold()
        .paint(format!(" {} ", mode_name))]
}

fn plugin_key(
    keymap: &[(KeyWithModifier, Vec<Action>)],
    plugin_name: &str,
) -> Option<KeyWithModifier> {
    keymap.iter().find_map(|(key, key_actions)| {
        if key_actions
            .iter()
            .any(|action| action.launches_plugin(plugin_name))
        {
            Some(key.clone())
        } else {
            None
        }
    })
}

/// Every key that leaves this mode for Normal.
///
/// Zellij binds Enter and Esc to the very same `SwitchToMode "Normal"`, so they
/// form one hint listing both keys rather than being split into a "select" and
/// an "exit" — a distinction Zellij does not make.
fn exit_keys(keymap: &[(KeyWithModifier, Vec<Action>)]) -> Vec<KeyWithModifier> {
    find_keys_for_actions(keymap, &[TO_NORMAL], true)
}

/// A hint queued for rendering: a concept `id`, the `label` shown for it, and
/// every key bound to it.
///
/// `id` — not `label` — is the merge key, and it is also the `label_<id>` config
/// key. Separating the two is what lets unrelated hints share a display label
/// without fusing (Pane's `new` is `new_pane`, Tab's is `new_tab`), and lets one
/// concept gather keys from several sources (a curated `focus` on `hjkl` and a
/// discovered `focus` on the arrows become one hint).
struct Hint {
    id: String,
    label: String,
    keys: Vec<KeyWithModifier>,
}

/// Queue a hint under concept `id`, recording the keys it consumed in `used` so
/// later discovery does not repeat them.
fn add_hint(
    hints: &mut Vec<Hint>,
    keys: &[KeyWithModifier],
    id: &str,
    label: &str,
    used: &mut Vec<KeyWithModifier>,
) {
    if keys.is_empty() {
        return;
    }
    merge_hint(hints, id, label, keys);
    used.extend(keys.iter().cloned());
}

/// Add `keys` to the hint with this `id`, creating it if it does not exist yet.
/// Keys already present are skipped so a merged hint never repeats a key. The
/// first source to create a hint sets its label; later merges only add keys.
fn merge_hint(hints: &mut Vec<Hint>, id: &str, label: &str, keys: &[KeyWithModifier]) {
    match hints.iter_mut().find(|hint| hint.id == id) {
        Some(hint) => {
            for key in keys {
                if !hint.keys.contains(key) {
                    hint.keys.push(key.clone());
                }
            }
        }
        None => hints.push(Hint {
            id: id.to_string(),
            label: label.to_string(),
            keys: keys.to_vec(),
        }),
    }
}

/// Render a hint's styled segments, without tracking consumed keys.
fn render_hint(parts: &mut Vec<ANSIString<'static>>, hint: &Hint, style: &HintStyle) {
    // Optionally collapse hjkl/arrow duplicates down to a single family.
    let mut keys = filter_direction_keys(&hint.keys, style.direction_keys);
    sort_keys(&mut keys, style.key_order);
    let keys = keys.as_slice();

    // A per-hint format overrides the global one; either overrides the theme
    // palette. An empty value falls back to the palette rather than rendering
    // nothing, matching how the global options treat empty.
    let key_format = hint_override(CONFIG_KEY_FORMAT_PREFIX, hint, style.mode, style.config);
    let key_format = match &key_format {
        Some(per_hint) => per_hint.as_deref(),
        None => style.key_format,
    };

    // Keys can be replaced outright by a fixed string, for hints better
    // described than enumerated — a chord, a mouse gesture, or a whole family
    // of keys standing in as `1-9`. An empty value renders no key part at all,
    // leaving the label to speak for itself.
    let replacement = hint_override(CONFIG_KEYS_PREFIX, hint, style.mode, style.config);
    let key_text = match &replacement {
        Some(fixed) => fixed.clone().unwrap_or_default(),
        None => compose_key_text(keys, style.config),
    };

    match key_format {
        Some(fmt) if !key_text.is_empty() => parts.extend(format::render_template(
            fmt,
            &[("key", &key_text)],
            style.config,
            style.dim,
        )),
        Some(_) => {}
        // Real keys keep the modifier grouping the palette styling does;
        // a replacement is already a finished string.
        None if replacement.is_some() => parts.extend(style_key_text(&key_text, style.colors)),
        None => parts.extend(style_key_with_modifier(keys, style.colors, style.config)),
    }

    // Description part, resolved the same way.
    let desc_format = hint_override(CONFIG_DESC_FORMAT_PREFIX, hint, style.mode, style.config);
    let desc_format = match &desc_format {
        Some(per_hint) => per_hint.as_deref(),
        None => style.desc_format,
    };

    match desc_format {
        Some(fmt) => parts.extend(format::render_template(
            fmt,
            &[("desc", &hint.label)],
            style.config,
            style.dim,
        )),
        None => parts.extend(style_description(&hint.label, style.colors)),
    }
}

fn render_hints_for_mode(
    mode: InputMode,
    keymap: &[(KeyWithModifier, Vec<Action>)],
    style: &HintStyle,
) -> Vec<ANSIString<'static>> {
    let mut hints: Vec<Hint> = vec![];
    // Keys consumed by curated hints, so discovery does not repeat them.
    let mut used: Vec<KeyWithModifier> = vec![];
    let exit = exit_keys(keymap);

    match mode {
        InputMode::Normal => {
            for (action, label) in NORMAL_MODE_ACTIONS {
                let keys = find_keys_for_actions(keymap, std::slice::from_ref(action), true);
                add_curated_hint(&mut hints, &keys, action, label, style, &mut used);
            }
        }
        InputMode::Pane => {
            for (actions, label) in PANE_MODE_ACTION_SEQUENCES {
                let keys = find_keys_for_actions(keymap, actions, false);
                if let (false, Some(action)) = (keys.is_empty(), actions.first()) {
                    add_curated_hint(&mut hints, &keys, action, label, style, &mut used);
                }
            }

            let rename_keys = find_keys_for_actions(
                keymap,
                &[
                    Action::SwitchToMode {
                        input_mode: InputMode::RenamePane,
                    },
                    Action::PaneNameInput { input: vec![0] },
                ],
                false,
            );
            if !rename_keys.is_empty() {
                add_group_hint(
                    &mut hints,
                    &rename_keys,
                    "rename",
                    "rename",
                    style,
                    &mut used,
                );
            }

            let focus_keys = find_keys_for_action_groups(
                keymap,
                &[
                    &[Action::MoveFocus {
                        direction: Direction::Left,
                    }],
                    &[Action::MoveFocus {
                        direction: Direction::Down,
                    }],
                    &[Action::MoveFocus {
                        direction: Direction::Up,
                    }],
                    &[Action::MoveFocus {
                        direction: Direction::Right,
                    }],
                ],
            );
            add_group_hint(&mut hints, &focus_keys, "focus", "focus", style, &mut used);
            add_exit_hint(&mut hints, &exit, style, &mut used);
        }
        InputMode::Tab => {
            for (actions, label) in TAB_MODE_ACTION_SEQUENCES {
                let keys = find_keys_for_actions(keymap, actions, false);
                if let (false, Some(action)) = (keys.is_empty(), actions.first()) {
                    add_curated_hint(&mut hints, &keys, action, label, style, &mut used);
                }
            }

            let rename_keys = find_keys_for_actions(
                keymap,
                &[
                    Action::SwitchToMode {
                        input_mode: InputMode::RenameTab,
                    },
                    Action::TabNameInput { input: vec![0] },
                ],
                false,
            );
            if !rename_keys.is_empty() {
                add_group_hint(
                    &mut hints,
                    &rename_keys,
                    "rename",
                    "rename",
                    style,
                    &mut used,
                );
            }

            // Every key bound to tab navigation, both families. Narrowing to one
            // is `direction_keys`' job, done at render time — deciding it here
            // would both ignore that setting and leave the keys it dropped
            // unclaimed, for discovery to resurface as separate hints.
            let focus_keys = find_keys_for_action_groups(
                keymap,
                &[&[Action::GoToPreviousTab], &[Action::GoToNextTab]],
            );
            add_group_hint(&mut hints, &focus_keys, "focus", "focus", style, &mut used);
            add_exit_hint(&mut hints, &exit, style, &mut used);
        }
        InputMode::Resize => {
            let resize_keys = find_keys_for_action_groups(
                keymap,
                &[
                    &[Action::Resize {
                        resize: Resize::Increase,
                        direction: None,
                    }],
                    &[Action::Resize {
                        resize: Resize::Decrease,
                        direction: None,
                    }],
                ],
            );
            add_group_hint(
                &mut hints,
                &resize_keys,
                "resize",
                "resize",
                style,
                &mut used,
            );

            let increase_keys = find_keys_for_action_groups(
                keymap,
                &[
                    &[Action::Resize {
                        resize: Resize::Increase,
                        direction: Some(Direction::Left),
                    }],
                    &[Action::Resize {
                        resize: Resize::Increase,
                        direction: Some(Direction::Down),
                    }],
                    &[Action::Resize {
                        resize: Resize::Increase,
                        direction: Some(Direction::Up),
                    }],
                    &[Action::Resize {
                        resize: Resize::Increase,
                        direction: Some(Direction::Right),
                    }],
                ],
            );
            add_group_hint(
                &mut hints,
                &increase_keys,
                "increase",
                "increase",
                style,
                &mut used,
            );

            let decrease_keys = find_keys_for_action_groups(
                keymap,
                &[
                    &[Action::Resize {
                        resize: Resize::Decrease,
                        direction: Some(Direction::Left),
                    }],
                    &[Action::Resize {
                        resize: Resize::Decrease,
                        direction: Some(Direction::Down),
                    }],
                    &[Action::Resize {
                        resize: Resize::Decrease,
                        direction: Some(Direction::Up),
                    }],
                    &[Action::Resize {
                        resize: Resize::Decrease,
                        direction: Some(Direction::Right),
                    }],
                ],
            );
            add_group_hint(
                &mut hints,
                &decrease_keys,
                "decrease",
                "decrease",
                style,
                &mut used,
            );
            add_exit_hint(&mut hints, &exit, style, &mut used);
        }
        InputMode::Move => {
            let move_keys = find_keys_for_action_groups(
                keymap,
                &[
                    &[Action::MovePane {
                        direction: Some(Direction::Left),
                    }],
                    &[Action::MovePane {
                        direction: Some(Direction::Down),
                    }],
                    &[Action::MovePane {
                        direction: Some(Direction::Up),
                    }],
                    &[Action::MovePane {
                        direction: Some(Direction::Right),
                    }],
                ],
            );
            add_group_hint(
                &mut hints,
                &move_keys,
                "move_pane",
                "move",
                style,
                &mut used,
            );
            add_exit_hint(&mut hints, &exit, style, &mut used);
        }
        InputMode::Scroll => {
            let search_keys = find_keys_for_actions(
                keymap,
                &[
                    Action::SwitchToMode {
                        input_mode: InputMode::EnterSearch,
                    },
                    Action::SearchInput { input: vec![0] },
                ],
                true,
            );
            add_group_hint(
                &mut hints,
                &search_keys,
                "mode_search",
                "search",
                style,
                &mut used,
            );

            let scroll_keys =
                find_keys_for_action_groups(keymap, &[&[Action::ScrollDown], &[Action::ScrollUp]]);
            add_group_hint(
                &mut hints,
                &scroll_keys,
                "scroll",
                "scroll",
                style,
                &mut used,
            );

            let page_scroll_keys = find_keys_for_action_groups(
                keymap,
                &[&[Action::PageScrollDown], &[Action::PageScrollUp]],
            );
            add_group_hint(
                &mut hints,
                &page_scroll_keys,
                "page",
                "page",
                style,
                &mut used,
            );

            let half_page_scroll_keys = find_keys_for_action_groups(
                keymap,
                &[&[Action::HalfPageScrollDown], &[Action::HalfPageScrollUp]],
            );
            add_group_hint(
                &mut hints,
                &half_page_scroll_keys,
                "half_page",
                "half page",
                style,
                &mut used,
            );

            let edit_keys = find_keys_for_actions(
                keymap,
                &[Action::EditScrollback { ansi: false }, TO_NORMAL],
                false,
            );
            if !edit_keys.is_empty() {
                add_group_hint(&mut hints, &edit_keys, "edit", "edit", style, &mut used);
            }
            add_exit_hint(&mut hints, &exit, style, &mut used);
        }
        InputMode::Search => {
            let search_keys = find_keys_for_actions(
                keymap,
                &[
                    Action::SwitchToMode {
                        input_mode: InputMode::EnterSearch,
                    },
                    Action::SearchInput { input: vec![0] },
                ],
                true,
            );
            add_group_hint(
                &mut hints,
                &search_keys,
                "mode_search",
                "search",
                style,
                &mut used,
            );

            let scroll_keys =
                find_keys_for_action_groups(keymap, &[&[Action::ScrollDown], &[Action::ScrollUp]]);
            add_group_hint(
                &mut hints,
                &scroll_keys,
                "scroll",
                "scroll",
                style,
                &mut used,
            );

            let page_scroll_keys = find_keys_for_action_groups(
                keymap,
                &[&[Action::PageScrollDown], &[Action::PageScrollUp]],
            );
            add_group_hint(
                &mut hints,
                &page_scroll_keys,
                "page",
                "page",
                style,
                &mut used,
            );

            let half_page_scroll_keys = find_keys_for_action_groups(
                keymap,
                &[&[Action::HalfPageScrollDown], &[Action::HalfPageScrollUp]],
            );
            add_group_hint(
                &mut hints,
                &half_page_scroll_keys,
                "half_page",
                "half page",
                style,
                &mut used,
            );

            let down_keys = find_keys_for_actions(
                keymap,
                &[Action::Search {
                    direction: SearchDirection::Down,
                }],
                true,
            );
            add_group_hint(
                &mut hints,
                &down_keys,
                "search_down",
                "down",
                style,
                &mut used,
            );

            let up_keys = find_keys_for_actions(
                keymap,
                &[Action::Search {
                    direction: SearchDirection::Up,
                }],
                true,
            );
            add_group_hint(&mut hints, &up_keys, "search_up", "up", style, &mut used);

            add_exit_hint(&mut hints, &exit, style, &mut used);
        }
        InputMode::Session => {
            let detach_keys = find_keys_for_actions(keymap, &[Action::Detach], true);
            add_group_hint(
                &mut hints,
                &detach_keys,
                "detach",
                "detach",
                style,
                &mut used,
            );

            for (plugin, id, label) in SESSION_PLUGINS {
                if let Some(key) = plugin_key(keymap, plugin) {
                    add_group_hint(&mut hints, &[key], id, label, style, &mut used);
                }
            }

            add_exit_hint(&mut hints, &exit, style, &mut used);
        }
        _ => {
            let keys = find_keys_for_actions(
                keymap,
                &[Action::SwitchToMode {
                    input_mode: InputMode::Normal,
                }],
                true,
            );
            add_exit_hint(&mut hints, &keys, style, &mut used);
        }
    }

    // Append every other enabled keybinding in this mode that the curated
    // hints above didn't already cover.
    if style.discover {
        add_discovered_hints(&mut hints, keymap, &used, style);
    }

    // Stable, so hints the user didn't name keep the order this mode built them.
    if !style.hint_order.is_empty() {
        hints.sort_by_key(|hint| style.hint_order.rank(hint));
    }

    // Render each hint on its own, so whole hints can be dropped to fit rather
    // than the line being cut mid-hint.
    let mut pieces: Vec<HintPiece> = hints
        .iter()
        .map(|hint| {
            let mut parts = vec![];
            render_hint(&mut parts, hint, style);
            HintPiece {
                width: visible_width(&parts, style.wide_ambiguous),
                group: style.hint_order.group(hint),
                parts,
            }
        })
        .collect();

    let spacer: Vec<ANSIString<'static>> = style
        .spacer
        .map(|spacer| format::render_template(spacer, &[], style.config, style.dim))
        .unwrap_or_default();

    let indicator: Vec<ANSIString<'static>> = style
        .drop_indicator
        .map(|indicator| format::render_template(indicator, &[], style.config, style.dim))
        .unwrap_or_default();

    let gap = style.limit.and_then(|limit| {
        drop_to_fit(
            &mut pieces,
            visible_width(&spacer, style.wide_ambiguous),
            limit,
            visible_width(&indicator, style.wide_ambiguous),
            style.precedence,
        )
    });

    // The indicator stands in for the dropped run, spaced like a hint so it
    // reads as one.
    let mut rendered: Vec<Vec<ANSIString<'static>>> =
        pieces.into_iter().map(|piece| piece.parts).collect();
    if let Some(gap) = gap.filter(|_| !indicator.is_empty()) {
        rendered.insert(gap.min(rendered.len()), indicator);
    }

    let mut parts = vec![];
    for (idx, piece) in rendered.into_iter().enumerate() {
        if idx > 0 {
            parts.extend(spacer.iter().cloned());
        }
        parts.extend(piece);
    }
    parts
}

/// Where a hint sits relative to the `*` in `hint_order`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum HintGroup {
    /// Pinned ahead of the `*`.
    Leading,
    /// Unpinned — the `*` itself.
    Middle,
    /// Pinned after the `*`.
    Trailing,
}

/// One rendered hint, kept separate so it can be dropped whole.
struct HintPiece {
    parts: Vec<ANSIString<'static>>,
    width: usize,
    group: HintGroup,
}

fn visible_width(parts: &[ANSIString<'static>], wide_ambiguous: bool) -> usize {
    calculate_visible_length(&format!("{}", ANSIStrings(parts)), wide_ambiguous)
}

/// Drop hints until the line fits, and report where the gap opened.
///
/// Hints go in a single contiguous run so one indicator can stand for all of
/// them. Unpinned hints — the `*` in `hint_order`, the ones the user never spoke
/// for — are given up first, from the right. When those run out the run keeps
/// growing *outward from that gap*: first the leading group from its inner edge,
/// then the trailing group from its inner edge. Dropping from the far ends
/// instead would open a second gap and need a second indicator.
///
/// Returns the index the indicator belongs at, or `None` if nothing was
/// dropped. A single hint wider than the whole limit survives and is cut by
/// `truncate_ansi_string` instead.
fn drop_to_fit(
    pieces: &mut Vec<HintPiece>,
    spacer_width: usize,
    limit: usize,
    indicator_width: usize,
    precedence: HintPrecedence,
) -> Option<usize> {
    let width = |pieces: &Vec<HintPiece>, extra: usize| {
        let hints: usize = pieces.iter().map(|piece| piece.width).sum();
        let count = pieces.len() + usize::from(extra > 0);
        hints + extra + spacer_width * count.saturating_sub(1)
    };

    let mut gap = None;
    while pieces.len() > 1 {
        // Once a hint has been dropped the indicator is part of the line, so it
        // has to be paid for out of the same budget.
        let extra = gap.map_or(0, |_| indicator_width);
        if width(pieces, extra) <= limit {
            break;
        }
        let Some(index) = next_to_drop(pieces, gap, precedence) else {
            break;
        };
        pieces.remove(index);
        gap = Some(index);
    }
    gap
}

/// The hint to give up next, as an index into `pieces`.
///
/// Always the one adjacent to the gap, so what is dropped stays a single run.
/// Unpinned hints go first; between the two pinned groups, `precedence` says
/// which is held onto longer.
fn next_to_drop(
    pieces: &[HintPiece],
    gap: Option<usize>,
    precedence: HintPrecedence,
) -> Option<usize> {
    // Only ever take from the inner edge of a group, the side facing the gap.
    let inner_left = |group: HintGroup| {
        pieces
            .iter()
            .rposition(|piece| piece.group == group)
            .filter(|index| gap.is_none_or(|gap| *index < gap))
    };
    let inner_right = |group: HintGroup| {
        pieces
            .iter()
            .position(|piece| piece.group == group)
            .filter(|index| gap.is_none_or(|gap| *index >= gap))
    };

    let leading = || inner_left(HintGroup::Leading);
    let trailing = || inner_right(HintGroup::Trailing);

    inner_left(HintGroup::Middle)
        .or_else(|| inner_right(HintGroup::Middle))
        .or_else(|| match precedence {
            // Keep the trailing group: spend the leading one first.
            HintPrecedence::Trailing => leading().or_else(trailing),
            HintPrecedence::Leading => trailing().or_else(leading),
        })
}

/// Discover every enabled keybinding the curated hints didn't already show, and
/// merge it into `hints`. Bindings are keyed by their resolved label (so
/// families like the directional focus keys collapse into a single hint, and a
/// label a curated hint already produced gains the extra keys instead of being
/// shown a second time). Labels resolve from, in order: a `label_<action>`
/// config override, the built-in label table, or a name derived from the action.
///
/// Bindings inherited from the base mode (Zellij's `shared_except` groups) are
/// skipped, so entering Pane mode doesn't re-list the global mode switches that
/// are already advertised in the base mode.
fn add_discovered_hints(
    hints: &mut Vec<Hint>,
    keymap: &[(KeyWithModifier, Vec<Action>)],
    used: &[KeyWithModifier],
    style: &HintStyle,
) {
    for (key, actions) in keymap {
        if used.contains(key) {
            continue;
        }
        // A binding's first action is often plumbing — a pipe to a plugin, a
        // rename input — with the part worth advertising coming after it. Judge
        // the binding by its first action that is worth a hint, so a key like
        // `MessagePlugin "autolock"; SwitchToMode "Normal"` still shows as the
        // unlock it visibly is, rather than vanishing entirely.
        let Some(primary) = actions.iter().find(|action| !is_hidden_action(action)) else {
            continue;
        };
        if is_shared_binding(key, primary, style.shared) {
            continue;
        }
        let Some((id, label)) = resolve_hint(primary, style) else {
            continue;
        };

        merge_hint(hints, &id, &label, std::slice::from_ref(key));
    }
}

/// Whether `key` is bound to the same action in the base mode, meaning it is a
/// global binding already shown there rather than one specific to this mode.
fn is_shared_binding(
    key: &KeyWithModifier,
    action: &Action,
    shared: &[(KeyWithModifier, Vec<Action>)],
) -> bool {
    shared.iter().any(|(shared_key, shared_actions)| {
        shared_key == key
            && shared_actions
                .first()
                .is_some_and(|shared_action| shared_action.shallow_eq(action))
    })
}

/// Resolve the label for a discovered keybinding's primary action. A user
/// `label_<action>` override wins; an empty override hides the hint. Otherwise
/// the built-in table is consulted, falling back to a name derived from the
/// action's own signature so nothing is ever left unlabeled.
fn resolve_hint(action: &Action, style: &HintStyle) -> Option<(String, String)> {
    let signature = action_signature(action);
    let (id, default) = match builtin_hint(action) {
        Some((id, label)) => (id.to_string(), label.to_string()),
        // No curated entry: the action names its own concept, and the label is
        // derived from that same name.
        None => (signature.clone(), signature.replace('_', " ")),
    };

    match label_override(&id, &signature, style.mode, style.config) {
        // An explicit label is its own merge id, so two actions given the same
        // label become one hint. See `merged_label_id`.
        Some(Some(label)) => Some((merged_label_id(&label), label)),
        // An empty override hides the hint.
        Some(None) => None,
        None => Some((id, default)),
    }
}

/// Look up a `label_<...>` override, accepting the hint's concept `id`
/// (`label_split_down`) or the raw action `signature` (`label_new_pane_down`),
/// each optionally scoped to the current mode (`label_locked_mode_normal`).
///
/// The outer `Option` distinguishes "no override set" from an override that is
/// present; the inner one is `None` for an empty value, meaning hide the hint.
fn label_override(
    id: &str,
    signature: &str,
    mode: &str,
    config: &BTreeMap<String, String>,
) -> Option<Option<String>> {
    // Most specific first: a label scoped to this mode beats a global one, and
    // the concept id beats the raw action signature. This is what lets
    // `label_locked_mode_normal "unlock"` retitle the unlock key without
    // touching every other mode's escape hatch.
    let value = [
        format!("{}{}_{}", CONFIG_LABEL_PREFIX, mode, id),
        format!("{}{}_{}", CONFIG_LABEL_PREFIX, mode, signature),
        format!("{}{}", CONFIG_LABEL_PREFIX, id),
        format!("{}{}", CONFIG_LABEL_PREFIX, signature),
    ]
    .iter()
    .find_map(|key| config.get(key))?;
    if value.is_empty() {
        Some(None)
    } else {
        Some(Some(value.clone()))
    }
}

/// Queue a curated hint, honoring a `label_<action>` override on `action`.
///
/// Curated hints carry hand-tuned labels rather than deriving them, so without
/// this they would silently ignore `label_<action>` entirely. An override that
/// hides the hint (an empty value) still marks its keys used, so discovery does
/// not resurface the same binding under the built-in label.
fn add_curated_hint(
    hints: &mut Vec<Hint>,
    keys: &[KeyWithModifier],
    action: &Action,
    default: &str,
    style: &HintStyle,
    used: &mut Vec<KeyWithModifier>,
) {
    let signature = action_signature(action);
    let id = builtin_hint(action)
        .map(|(id, _)| id.to_string())
        .unwrap_or_else(|| signature.clone());
    apply_label(hints, keys, &id, &signature, default, style, used);
}

/// Queue the hint for the keys that leave a mode for Normal.
///
/// Routed through `add_curated_hint` with the `SwitchToMode "Normal"` action —
/// not `add_group_hint` — so the override resolves by action name as well as by
/// id. Both `label_mode_normal` and `label_switch_to_mode_normal` (and their
/// mode-scoped forms) reach it. That the action form works matters: it is what
/// the built-in status bar used, so configs carried over from it keep working.
fn add_exit_hint(
    hints: &mut Vec<Hint>,
    keys: &[KeyWithModifier],
    style: &HintStyle,
    used: &mut Vec<KeyWithModifier>,
) {
    add_curated_hint(hints, keys, &TO_NORMAL, "normal", style, used);
}

/// Queue a curated hint that has no single backing action — one assembled from a
/// group of related bindings (the four resize directions), from alternatives
/// (`rename` matches whichever of two actions is bound), or discovered at
/// runtime (`select`, the plugin launchers). These declare their concept `id`
/// explicitly, since there is no action to derive it from.
fn add_group_hint(
    hints: &mut Vec<Hint>,
    keys: &[KeyWithModifier],
    id: &str,
    default: &str,
    style: &HintStyle,
    used: &mut Vec<KeyWithModifier>,
) {
    apply_label(hints, keys, id, id, default, style, used);
}

/// Queue a hint under `id`, applying a `label_<id>` / `label_<signature>`
/// override if one is set. An override that hides the hint still marks its keys
/// used, so discovery does not resurface it under the built-in label.
fn apply_label(
    hints: &mut Vec<Hint>,
    keys: &[KeyWithModifier],
    id: &str,
    signature: &str,
    default: &str,
    style: &HintStyle,
    used: &mut Vec<KeyWithModifier>,
) {
    match label_override(id, signature, style.mode, style.config) {
        Some(Some(label)) => add_hint(hints, keys, &merged_label_id(&label), &label, used),
        Some(None) => used.extend(keys.iter().cloned()),
        None => add_hint(hints, keys, id, default, used),
    }
}

/// Look up a per-hint setting, e.g. `key_format_split_down`.
///
/// Tries the most specific key first, so a mode-scoped setting beats a global
/// one, exactly as labels resolve. A hint can be named by its concept id or by
/// its label — the latter matters for hints fused under a label you chose,
/// whose id is that label with a marker prefix and not something anyone wants
/// to type.
///
/// The outer `Option` separates "not set" from set; the inner one is `None`
/// for an empty value, which each caller reads for itself.
fn hint_override(
    prefix: &str,
    hint: &Hint,
    mode: &str,
    config: &BTreeMap<String, String>,
) -> Option<Option<String>> {
    let label = config_slug(&hint.label);
    let value = [
        format!("{}{}_{}", prefix, mode, hint.id),
        format!("{}{}_{}", prefix, mode, label),
        format!("{}{}", prefix, hint.id),
        format!("{}{}", prefix, label),
    ]
    .iter()
    .find_map(|key| config.get(key))?;
    Some(Some(value.clone()).filter(|value| !value.is_empty()))
}

/// A label as it can be written in a config key: `swap layout` -> `swap_layout`.
fn config_slug(label: &str) -> String {
    label
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
}

/// The merge id for a hint whose label the user set explicitly.
///
/// Deliberately relabeling two hints to the same text fuses them into one hint —
/// that is how you combine separate bindings you consider a single concept, e.g.
/// giving both swap-layout actions the label "swap layout". Built-in labels do
/// *not* merge this way, so hints that merely ship with the same label (Pane's
/// `new` and Tab's `new`) stay separate.
fn merged_label_id(label: &str) -> String {
    // `=` never appears in a concept id, so this cannot collide with one.
    format!("={}", label)
}

/// A stable, configuration-friendly identifier for an action, used both as the
/// `label_<name>` config key and as the derived fallback label. It is the
/// snake_case variant name, with the target mode appended for mode switches
/// (e.g. `switch_to_mode_locked`) since that distinction is meaningful.
fn action_signature(action: &Action) -> String {
    let debug = format!("{:?}", action);
    let variant = debug.split(['(', '{', ' ']).next().unwrap_or(&debug);
    let mut signature = to_snake_case(variant);
    match action {
        Action::SwitchToMode { input_mode: mode }
        | Action::SwitchModeForAllClients { input_mode: mode } => {
            signature.push('_');
            signature.push_str(&to_snake_case(&format!("{:?}", mode)));
        }
        // Directional variants are distinct hints with distinct labels (`split
        // down` vs `new`), so they need distinct signatures to be addressable:
        // `label_new_pane_down` rather than one `label_new_pane` for all five.
        Action::NewPane {
            direction: Some(direction),
            ..
        }
        | Action::MovePane {
            direction: Some(direction),
        }
        | Action::MoveFocus { direction }
        | Action::MoveFocusOrTab { direction }
        | Action::MoveTab { direction } => {
            signature.push('_');
            signature.push_str(&to_snake_case(&format!("{:?}", direction)));
        }
        Action::Resize { resize, direction } => {
            signature.push('_');
            signature.push_str(&to_snake_case(&format!("{:?}", resize)));
            if let Some(direction) = direction {
                signature.push('_');
                signature.push_str(&to_snake_case(&format!("{:?}", direction)));
            }
        }
        _ => {}
    }
    signature
}

fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Actions that are never useful as hints (text input, mouse, programmatic /
/// CLI-only actions). These are skipped during discovery.
fn is_hidden_action(action: &Action) -> bool {
    matches!(
        action,
        Action::Write { .. }
            | Action::WriteChars { .. }
            | Action::PaneNameInput { .. }
            | Action::TabNameInput { .. }
            | Action::SearchInput { .. }
            | Action::RenameTab { .. }
            | Action::RenameTerminalPane { .. }
            | Action::RenamePluginPane { .. }
            | Action::NoOp
            | Action::MouseEvent { .. }
            | Action::ScrollUpAt { .. }
            | Action::ScrollDownAt { .. }
            | Action::CliPipe { .. }
            | Action::KeybindPipe { .. }
            | Action::DumpScreen { .. }
            | Action::DumpLayout
            | Action::ListClients
            | Action::QueryTabNames
            | Action::Run { .. }
            | Action::SkipConfirm { .. }
            | Action::StackPanes { .. }
            | Action::ChangeFloatingPaneCoordinates { .. }
    )
}

/// The built-in action -> (id, label) table. `id` is the concept the hint
/// belongs to — the merge key and the `label_<id>` config key — and `label` is
/// what gets displayed. Returns `None` for actions with no curated entry, in
/// which case the caller derives both from the action signature.
///
/// Ids must be unique per concept even where labels repeat: `new_pane` and
/// `new_tab` both display "new" but must never merge. Conversely, actions that
/// deliberately form one hint share an id (the four `MoveFocus` directions).
fn builtin_hint(action: &Action) -> Option<(&'static str, &'static str)> {
    if let Action::SwitchToMode { input_mode: mode }
    | Action::SwitchModeForAllClients { input_mode: mode } = action
    {
        return switch_mode_hint(mode);
    }

    Some(match action {
        Action::Quit => ("quit", "quit"),
        Action::Detach => ("detach", "detach"),

        // Panes
        Action::NewPane {
            direction: None, ..
        } => ("new_pane", "new"),
        Action::NewPane {
            direction: Some(Direction::Left),
            ..
        } => ("split_left", "split left"),
        Action::NewPane {
            direction: Some(Direction::Right),
            ..
        } => ("split_right", "split right"),
        Action::NewPane {
            direction: Some(Direction::Up),
            ..
        } => ("split_up", "split up"),
        Action::NewPane {
            direction: Some(Direction::Down),
            ..
        } => ("split_down", "split down"),
        Action::NewStackedPane { .. } => ("stacked_pane", "stacked"),
        Action::NewFloatingPane { .. } => ("floating_pane", "floating"),
        Action::NewInPlacePane { .. } => ("in_place_pane", "in place"),
        Action::CloseFocus => ("close_pane", "close"),
        Action::ToggleFocusFullscreen => ("fullscreen", "fullscreen"),
        Action::ToggleFloatingPanes => ("float", "float"),
        Action::TogglePaneEmbedOrFloating => ("embed", "embed"),
        Action::TogglePaneFrames => ("frames", "frames"),
        Action::TogglePanePinned => ("pin", "pin"),
        Action::TogglePaneInGroup => ("group_pane", "group"),
        Action::ToggleGroupMarking => ("group_marking", "mark"),
        // Directing focus is "focus"; only actions that relocate a pane are
        // "move". Distinct ids keep them apart from each other and from the
        // `SwitchToMode(Move)` hint, which means something else entirely.
        Action::MoveFocus { direction: _ } | Action::MoveFocusOrTab { direction: _ } => {
            ("focus", "focus")
        }
        Action::MovePane { direction: _ } => ("move_pane", "move"),
        Action::MovePaneBackwards => ("move_pane_back", "move back"),
        Action::FocusNextPane => ("next_pane", "next"),
        Action::FocusPreviousPane => ("prev_pane", "prev"),
        Action::SwitchFocus => ("toggle_focus", "toggle focus"),

        // Tabs
        Action::NewTab { .. } => ("new_tab", "new"),
        Action::CloseTab => ("close_tab", "close"),
        Action::GoToNextTab => ("next_tab", "next"),
        Action::GoToPreviousTab => ("prev_tab", "prev"),
        Action::GoToTab { index: _ } | Action::GoToTabName { .. } => ("go_to_tab", "tab"),
        Action::ToggleTab => ("toggle_tab", "toggle"),
        Action::ToggleActiveSyncTab => ("sync", "sync"),
        Action::MoveTab { direction: _ } => ("move_tab", "move tab"),
        Action::BreakPane => ("break_pane", "break pane"),
        Action::BreakPaneLeft => ("break_left", "break left"),
        Action::BreakPaneRight => ("break_right", "break right"),

        // Resize
        Action::Resize {
            resize: Resize::Increase,
            direction: None,
        }
        | Action::Resize {
            resize: Resize::Decrease,
            direction: None,
        } => ("resize", "resize"),
        Action::Resize {
            resize: Resize::Increase,
            direction: Some(_),
        } => ("increase", "increase"),
        Action::Resize {
            resize: Resize::Decrease,
            direction: Some(_),
        } => ("decrease", "decrease"),

        // Scroll / search
        Action::ScrollUp | Action::ScrollDown => ("scroll", "scroll"),
        Action::PageScrollUp | Action::PageScrollDown => ("page", "page"),
        Action::HalfPageScrollUp | Action::HalfPageScrollDown => ("half_page", "half page"),
        Action::ScrollToTop => ("top", "top"),
        Action::ScrollToBottom => ("bottom", "bottom"),
        Action::EditScrollback { .. } => ("edit", "edit"),
        Action::Search {
            direction: SearchDirection::Down,
        } => ("search_down", "down"),
        Action::Search {
            direction: SearchDirection::Up,
        } => ("search_up", "up"),
        Action::SearchToggleOption { option: _ } => ("search_toggle", "toggle"),

        // Misc
        Action::Copy => ("copy", "copy"),
        Action::ClearScreen => ("clear", "clear"),
        Action::ToggleMouseMode => ("mouse", "mouse"),
        Action::PreviousSwapLayout => ("prev_layout", "prev layout"),
        Action::NextSwapLayout => ("next_layout", "next layout"),
        Action::Confirm => ("confirm", "confirm"),
        Action::Deny => ("deny", "deny"),
        Action::RenameSession { name: _ } => ("rename_session", "rename"),
        Action::UndoRenamePane | Action::UndoRenameTab => ("undo_rename", "undo"),

        _ => return None,
    })
}

/// Id and label for a `SwitchToMode` action, based on the target mode. Ids are
/// `mode_`-prefixed so that, e.g., switching to Move mode never merges with the
/// `MovePane` hint that shares its label.
fn switch_mode_hint(mode: &InputMode) -> Option<(&'static str, &'static str)> {
    Some(match mode {
        InputMode::Normal => ("mode_normal", "normal"),
        InputMode::Locked => ("mode_locked", "lock"),
        InputMode::Pane => ("mode_pane", "pane"),
        InputMode::Tab => ("mode_tab", "tab"),
        InputMode::Resize => ("mode_resize", "resize"),
        InputMode::Move => ("mode_move", "move"),
        InputMode::Scroll => ("mode_scroll", "scroll"),
        InputMode::Search | InputMode::EnterSearch => ("mode_search", "search"),
        InputMode::Session => ("mode_session", "session"),
        InputMode::RenameTab | InputMode::RenamePane => ("mode_rename", "rename"),
        InputMode::Tmux => ("mode_tmux", "tmux"),
        InputMode::Prompt => ("mode_prompt", "prompt"),
    })
}

fn get_keymap_for_mode(mode_info: &ModeInfo) -> Vec<(KeyWithModifier, Vec<Action>)> {
    match mode_info.mode {
        InputMode::Normal => mode_info.get_keybinds_for_mode(InputMode::Normal),
        InputMode::Pane => mode_info.get_keybinds_for_mode(InputMode::Pane),
        InputMode::Tab => mode_info.get_keybinds_for_mode(InputMode::Tab),
        InputMode::Resize => mode_info.get_keybinds_for_mode(InputMode::Resize),
        InputMode::Move => mode_info.get_keybinds_for_mode(InputMode::Move),
        InputMode::Scroll => mode_info.get_keybinds_for_mode(InputMode::Scroll),
        InputMode::Search => mode_info.get_keybinds_for_mode(InputMode::Search),
        InputMode::Session => mode_info.get_keybinds_for_mode(InputMode::Session),
        _ => mode_info.get_mode_keybinds(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Sort a string of characters as if they were the keys of one hint, so an
    /// expected ordering can be written as a plain string literal.
    fn sorted(chars: &str, order: KeyOrder) -> String {
        let mut keys: Vec<KeyWithModifier> = chars
            .chars()
            .map(|c| KeyWithModifier::new(BareKey::Char(c)))
            .collect();
        sort_keys(&mut keys, order);
        keys.iter()
            .map(|key| match key.bare_key {
                BareKey::Char(c) => c,
                _ => '?',
            })
            .collect()
    }

    #[test]
    fn every_layout_covers_the_alphabet_and_digits_exactly_once() {
        for order in [KeyOrder::Qwerty, KeyOrder::Dvorak, KeyOrder::Colemak] {
            let all: String = order.rows().concat();
            for expected in "abcdefghijklmnopqrstuvwxyz0123456789".chars() {
                assert_eq!(
                    all.matches(expected).count(),
                    1,
                    "{:?} should place {:?} exactly once",
                    order.rows(),
                    expected
                );
            }
        }
    }

    #[test]
    fn digits_follow_the_digit_row_so_zero_comes_last() {
        assert_eq!(sorted("271543689", KeyOrder::Qwerty), "123456789");
        assert_eq!(sorted("0159", KeyOrder::Qwerty), "1590");
    }

    #[test]
    fn letters_sort_by_row_then_column() {
        assert_eq!(sorted("kh", KeyOrder::Qwerty), "hk");
        assert_eq!(sorted("lkjh", KeyOrder::Qwerty), "hjkl");
        // Rows run top to bottom: q above a above z.
        assert_eq!(sorted("zaq", KeyOrder::Qwerty), "qaz");
    }

    #[test]
    fn letters_group_ahead_of_punctuation_rather_than_interleaving() {
        assert_eq!(sorted("op\\as", KeyOrder::Qwerty), "opas\\");
    }

    #[test]
    fn digits_letters_and_punctuation_stay_in_separate_groups() {
        assert_eq!(sorted("a1/", KeyOrder::Qwerty), "1a/");
        assert_eq!(sorted("/1a", KeyOrder::Qwerty), "1a/");
    }

    #[test]
    fn layout_changes_where_letters_land() {
        // hjkl only reads in order on the layout it was designed for. Dvorak
        // scatters it across all three rows — l on the top, h on the home row,
        // j and k down on the bottom — so the vim ordering is lost.
        assert_eq!(sorted("hjkl", KeyOrder::Qwerty), "hjkl");
        assert_eq!(sorted("hjkl", KeyOrder::Dvorak), "lhjk");
        assert_eq!(sorted("arst", KeyOrder::Colemak), "arst");
    }

    #[test]
    fn alphabetical_ignores_the_keyboard_layout() {
        assert_eq!(sorted("dbca", KeyOrder::Alphabetical), "abcd");
        // The layout modes would order these by position instead.
        assert_eq!(sorted("lkjh", KeyOrder::Alphabetical), "hjkl");
        assert_eq!(sorted("zaq", KeyOrder::Alphabetical), "aqz");
        assert_eq!(sorted("qaz", KeyOrder::Qwerty), "qaz");
    }

    #[test]
    fn alphabetical_puts_zero_first_unlike_the_digit_row() {
        // Plain ascending order, so `0` leads rather than trailing `9`.
        assert_eq!(sorted("0159", KeyOrder::Alphabetical), "0159");
        assert_eq!(sorted("0159", KeyOrder::Qwerty), "1590");
    }

    #[test]
    fn alphabetical_still_groups_digits_letters_and_punctuation() {
        assert_eq!(sorted("/a1", KeyOrder::Alphabetical), "1a/");
    }

    #[test]
    fn unsorted_preserves_the_order_zellij_reported() {
        assert_eq!(sorted("271543689", KeyOrder::Unsorted), "271543689");
        assert_eq!(sorted("op\\as", KeyOrder::Unsorted), "op\\as");
    }

    #[test]
    fn arrows_follow_hjkl_order_not_declaration_order() {
        let mut keys: Vec<KeyWithModifier> =
            [BareKey::Right, BareKey::Up, BareKey::Left, BareKey::Down]
                .into_iter()
                .map(KeyWithModifier::new)
                .collect();
        sort_keys(&mut keys, KeyOrder::Qwerty);
        let bare: Vec<BareKey> = keys.iter().map(|k| k.bare_key.clone()).collect();
        assert_eq!(
            bare,
            vec![BareKey::Left, BareKey::Down, BareKey::Up, BareKey::Right]
        );
    }

    #[test]
    fn function_keys_sort_numerically_not_lexically() {
        let mut keys: Vec<KeyWithModifier> = [10u8, 2, 1]
            .into_iter()
            .map(|n| KeyWithModifier::new(BareKey::F(n)))
            .collect();
        sort_keys(&mut keys, KeyOrder::Qwerty);
        let bare: Vec<BareKey> = keys.iter().map(|k| k.bare_key.clone()).collect();
        assert_eq!(bare, vec![BareKey::F(1), BareKey::F(2), BareKey::F(10)]);
    }

    #[test]
    fn modifier_groups_run_unmodified_then_ctrl_super_alt_shift() {
        let plain = KeyWithModifier::new(BareKey::Char('a'));
        let mut keys = vec![
            plain.clone().with_shift_modifier(),
            plain.clone().with_alt_modifier(),
            plain.clone(),
            plain.clone().with_super_modifier(),
            plain.clone().with_ctrl_modifier(),
        ];
        sort_keys(&mut keys, KeyOrder::Qwerty);
        // Assert the modifiers themselves, not just that the ranks ascend —
        // ascending ranks would hold for any ordering.
        let groups: Vec<Vec<KeyModifier>> = keys
            .iter()
            .map(|key| key.key_modifiers.iter().copied().collect())
            .collect();
        assert_eq!(
            groups,
            vec![
                vec![],
                vec![KeyModifier::Ctrl],
                vec![KeyModifier::Super],
                vec![KeyModifier::Alt],
                vec![KeyModifier::Shift],
            ]
        );
    }

    #[test]
    fn a_key_sorts_with_the_strongest_modifier_it_carries() {
        let plain = KeyWithModifier::new(BareKey::Char('p'));
        let ctrl_shift = plain.clone().with_ctrl_modifier().with_shift_modifier();
        assert_eq!(modifier_rank(&ctrl_shift.key_modifiers), 1);
        // Same group as plain Ctrl, but still ordered after it.
        let ctrl = plain.with_ctrl_modifier();
        assert_eq!(modifier_rank(&ctrl.key_modifiers), 1);
        assert!(
            modifier_tiebreak(&ctrl.key_modifiers) < modifier_tiebreak(&ctrl_shift.key_modifiers)
        );
    }

    /// Build hints with the given ids (label mirrors the id unless it contains
    /// `/`, written as `id/label`), order them, and read back the resulting ids.
    fn ordered(ids: &[&str], config: &str) -> Vec<String> {
        let mut hints: Vec<Hint> = ids
            .iter()
            .map(|spec| {
                let (id, label) = spec.split_once('/').unwrap_or((spec, spec));
                Hint {
                    id: id.to_string(),
                    label: label.to_string(),
                    keys: vec![],
                }
            })
            .collect();
        let order = HintOrder::from_config(config);
        if !order.is_empty() {
            hints.sort_by_key(|hint| order.rank(hint));
        }
        hints.into_iter().map(|hint| hint.id).collect()
    }

    #[test]
    fn wildcard_splits_pinned_front_from_pinned_back() {
        assert_eq!(
            ordered(&["a", "b", "c", "d"], "d, *, a"),
            vec!["d", "b", "c", "a"]
        );
    }

    #[test]
    fn entries_before_the_wildcard_lead_in_the_order_given() {
        assert_eq!(ordered(&["a", "b", "c"], "c, b, *"), vec!["c", "b", "a"]);
    }

    #[test]
    fn entries_after_the_wildcard_trail_in_the_order_given() {
        assert_eq!(ordered(&["a", "b", "c"], "*, b, a"), vec!["c", "b", "a"]);
    }

    #[test]
    fn a_list_without_a_wildcard_leads_and_the_rest_follow() {
        assert_eq!(ordered(&["a", "b", "c"], "c, a"), vec!["c", "a", "b"]);
    }

    #[test]
    fn unlisted_hints_keep_the_order_the_mode_built_them() {
        assert_eq!(
            ordered(&["a", "b", "c", "d", "e"], "e, *"),
            vec!["e", "a", "b", "c", "d"]
        );
    }

    #[test]
    fn an_empty_or_absent_order_changes_nothing() {
        assert_eq!(ordered(&["b", "a"], ""), vec!["b", "a"]);
        assert_eq!(ordered(&["b", "a"], "  ,  "), vec!["b", "a"]);
    }

    #[test]
    fn naming_an_absent_hint_is_harmless() {
        assert_eq!(ordered(&["a", "b"], "nope, *, alsonope"), vec!["a", "b"]);
    }

    #[test]
    fn hints_can_be_named_by_label_as_well_as_id() {
        // A hint fused by a shared label carries the internal id `=swap layout`;
        // naming it by the label is what makes it addressable.
        assert_eq!(
            ordered(&["a", "=swap layout/swap layout"], "swap layout, *"),
            vec!["=swap layout", "a"]
        );
    }

    #[test]
    fn matching_ignores_case_and_surrounding_space() {
        assert_eq!(ordered(&["a", "Quit"], "  QUIT  , *"), vec!["Quit", "a"]);
    }

    /// Render a mode from a synthetic keymap and return the visible text, so a
    /// whole hint line can be asserted as a plain string.
    fn rendered(
        mode: InputMode,
        keymap: &[(KeyWithModifier, Vec<Action>)],
        config: &[(&str, &str)],
    ) -> String {
        let colors = Styling::default();
        let config = label_config(config);
        let hint_order = HintOrder::from_config(config.get("hint_order").map_or("", |v| v));
        let mode_key = format!("{:?}", mode).to_lowercase();
        let style = HintStyle {
            colors: &colors,
            dim: config
                .get("test_dim_strength")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.0),
            // Minimal formats keep assertions readable; a test can override
            // them through config to exercise the real precedence.
            key_format: config
                .get("key_format")
                .map(String::as_str)
                .or(Some("{key} ")),
            desc_format: config
                .get("desc_format")
                .map(String::as_str)
                .or(Some("{desc}")),
            spacer: Some("|"),
            discover: config.get("discover_hints").map_or(true, |v| v == "true"),
            direction_keys: DirectionKeys::from_config(
                config.get("direction_keys").map_or("both", |v| v),
            ),
            key_order: KeyOrder::default(),
            mode: &mode_key,
            hint_order: &hint_order,
            limit: config.get("limit").and_then(|v| v.parse::<usize>().ok()),
            drop_indicator: config.get("drop_indicator").map(|v| v.as_str()),
            precedence: HintPrecedence::from_config(
                config.get("hint_precedence").map_or("", |v| v),
            ),
            wide_ambiguous: config
                .get("ambiguous_width")
                .map(|v| v == "2")
                .unwrap_or(false),
            shared: &[],
            config: &config,
        };
        let parts = render_hints_for_mode(mode, keymap, &style);
        let text = format!("{}", ANSIStrings(&parts));
        let mut parser = AnsiParser::new(&text);
        let mut visible = String::new();
        while let Some(segment) = parser.next_segment() {
            if let AnsiSegment::VisibleChar(ch) = segment {
                visible.push(ch);
            }
        }
        visible
    }

    fn key(bare: BareKey) -> KeyWithModifier {
        KeyWithModifier::new(bare)
    }

    fn ctrl(c: char) -> KeyWithModifier {
        KeyWithModifier::new(BareKey::Char(c)).with_ctrl_modifier()
    }

    /// `MessagePlugin` in KDL, which parses to this plumbing action.
    fn message_plugin() -> Action {
        Action::KeybindPipe {
            name: None,
            payload: Some("enable".to_string()),
            args: None,
            plugin: Some("autolock".to_string()),
            plugin_id: None,
            configuration: None,
            launch_new: false,
            skip_cache: false,
            floating: None,
            in_place: None,
            cwd: None,
            pane_title: None,
        }
    }

    #[test]
    fn a_binding_led_by_a_plumbing_action_still_shows_what_it_does() {
        // Locked mode's only binding is `MessagePlugin ...; SwitchToMode
        // "Normal"`. Judging it by its first action alone hid the mode entirely.
        let keymap = vec![(ctrl('g'), vec![message_plugin(), TO_NORMAL])];
        assert_eq!(rendered(InputMode::Locked, &keymap, &[]), "Ctrl g normal");
        assert_eq!(
            rendered(
                InputMode::Locked,
                &keymap,
                &[("label_locked_mode_normal", "unlock")]
            ),
            "Ctrl g unlock"
        );
    }

    #[test]
    fn a_binding_of_pure_plumbing_stays_hidden() {
        let keymap = vec![(ctrl('g'), vec![message_plugin()])];
        assert_eq!(rendered(InputMode::Locked, &keymap, &[]), "");
    }

    /// Tab navigation as Zellij binds it by default: both families, both ways.
    fn tab_nav_keymap() -> Vec<(KeyWithModifier, Vec<Action>)> {
        vec![
            (key(BareKey::Char('h')), vec![Action::GoToPreviousTab]),
            (key(BareKey::Left), vec![Action::GoToPreviousTab]),
            (key(BareKey::Char('l')), vec![Action::GoToNextTab]),
            (key(BareKey::Right), vec![Action::GoToNextTab]),
        ]
    }

    #[test]
    fn tab_focus_honors_direction_keys_and_claims_every_key_it_shows() {
        // With "letters", the arrows drop out and no stray next/prev hints are
        // left behind for discovery to resurface.
        assert_eq!(
            rendered(
                InputMode::Tab,
                &tab_nav_keymap(),
                &[("direction_keys", "letters")]
            ),
            "hl focus"
        );
        assert_eq!(
            rendered(
                InputMode::Tab,
                &tab_nav_keymap(),
                &[("direction_keys", "arrows")]
            ),
            "←→ focus"
        );
    }

    #[test]
    fn tab_focus_shows_both_families_by_default() {
        assert_eq!(
            rendered(InputMode::Tab, &tab_nav_keymap(), &[]),
            "hl←→ focus"
        );
    }

    #[test]
    fn the_exit_hint_honours_the_action_name_label() {
        let keymap = vec![(key(BareKey::Esc), vec![TO_NORMAL])];
        // The action-name form is what the built-in status bar used, so configs
        // carried over from it rely on this reaching the exit hint.
        assert_eq!(
            rendered(
                InputMode::Tab,
                &keymap,
                &[("label_switch_to_mode_normal", "exit")]
            ),
            "ESC exit"
        );
        // The id form keeps working too.
        assert_eq!(
            rendered(InputMode::Tab, &keymap, &[("label_mode_normal", "exit")]),
            "ESC exit"
        );
    }

    #[test]
    fn the_exit_hint_honours_a_mode_scoped_action_name_label() {
        let keymap = vec![(key(BareKey::Esc), vec![TO_NORMAL])];
        let config = &[
            ("label_switch_to_mode_normal", "exit"),
            ("label_locked_switch_to_mode_normal", "unlock"),
        ];
        assert_eq!(rendered(InputMode::Locked, &keymap, config), "ESC unlock");
        assert_eq!(rendered(InputMode::Tab, &keymap, config), "ESC exit");
    }

    #[test]
    fn a_relabelled_exit_hint_pins_under_its_new_label() {
        // The regression cascade: when the label did not apply, the hint stayed
        // "normal", so `hint_order "*, exit"` never matched it, so it sat in the
        // droppable middle instead of pinned last. With the label applied it is
        // pinned and survives a narrow bar that drops the middle.
        let keymap = vec![
            (key(BareKey::Char('a')), vec![Action::CloseFocus]),
            (key(BareKey::Char('b')), vec![Action::ToggleFocusFullscreen]),
            (key(BareKey::Esc), vec![TO_NORMAL]),
        ];
        let config = &[
            ("label_switch_to_mode_normal", "exit"),
            ("hint_order", "*, exit"),
            ("limit", "12"),
        ];
        let out = rendered(InputMode::Tab, &keymap, config);
        assert!(
            out.contains("exit"),
            "exit was dropped, not pinned: {:?}",
            out
        );
    }

    #[test]
    fn every_key_that_leaves_a_mode_forms_one_hint() {
        // Enter and Esc are bound to the identical action, so they are one hint
        // rather than a "select" and an "exit" for the same thing.
        let keymap = vec![
            (key(BareKey::Enter), vec![TO_NORMAL]),
            (key(BareKey::Esc), vec![TO_NORMAL]),
        ];
        assert_eq!(rendered(InputMode::Pane, &keymap, &[]), "ESCENTER normal");
        assert_eq!(rendered(InputMode::Tab, &keymap, &[]), "ESCENTER normal");
    }

    #[test]
    fn a_mode_keeps_its_exit_hint_without_discovery() {
        // The curated list has to carry the escape hatch itself, since discovery
        // is off by default.
        let keymap = vec![
            (key(BareKey::Char('x')), vec![Action::CloseFocus, TO_NORMAL]),
            (key(BareKey::Esc), vec![TO_NORMAL]),
        ];
        assert_eq!(
            rendered(InputMode::Pane, &keymap, &[("discover_hints", "false")]),
            "x close|ESC normal"
        );
    }

    #[test]
    fn a_mode_scoped_label_reaches_the_rendered_line() {
        let config = &[
            ("label_mode_normal", "exit"),
            ("label_locked_mode_normal", "unlock"),
        ];
        // Locked binds only the one escape hatch.
        let locked = vec![(key(BareKey::Esc), vec![TO_NORMAL])];
        assert_eq!(rendered(InputMode::Locked, &locked, config), "ESC unlock");
        // Pane binds Enter and Esc to the same action, so they form one hint
        // carrying both keys, under the global label.
        let pane = vec![
            (key(BareKey::Enter), vec![TO_NORMAL]),
            (key(BareKey::Esc), vec![TO_NORMAL]),
        ];
        assert_eq!(rendered(InputMode::Pane, &pane, config), "ESCENTER exit");
    }

    /// A styled line of the shape the plugin actually emits: coloured segments
    /// closed by the reset `ANSIStrings` appends.
    fn styled_line() -> String {
        let parts = vec![
            Style::new().on(Fixed(1)).paint("aaaa"),
            Style::new().on(Fixed(2)).paint("bbbb"),
        ];
        format!("{}", ANSIStrings(&parts))
    }

    #[test]
    fn an_untruncated_line_already_ends_reset() {
        assert!(styled_line().ends_with(ANSI_RESET));
    }

    #[test]
    fn truncating_closes_the_styled_run_so_colour_stops_at_the_cut() {
        let truncated = truncate_ansi_string(&styled_line(), "...", 6, false);
        assert!(
            truncated.ends_with(ANSI_RESET),
            "colour would bleed past the cut: {:?}",
            truncated
        );
    }

    #[test]
    fn truncating_keeps_the_visible_width_within_the_limit() {
        for limit in 1..=10 {
            let truncated = truncate_ansi_string(&styled_line(), "...", limit, false);
            assert!(
                calculate_visible_length(&truncated, false) <= limit,
                "limit {} produced {:?}",
                limit,
                truncated
            );
        }
    }

    #[test]
    fn a_line_that_fits_is_left_exactly_as_it_was() {
        let line = styled_line();
        assert_eq!(truncate_ansi_string(&line, "...", 100, false), line);
    }

    /// Four one-key hints, each rendering as `<key> <label>` — 6 columns wide
    /// with the test harness's formats, plus a 1-column spacer between them.
    fn four_hints() -> Vec<(KeyWithModifier, Vec<Action>)> {
        vec![
            (key(BareKey::Char('a')), vec![Action::CloseFocus]),
            (key(BareKey::Char('b')), vec![Action::ToggleFocusFullscreen]),
            (key(BareKey::Char('c')), vec![Action::TogglePaneFrames]),
            (key(BareKey::Char('d')), vec![Action::TogglePanePinned]),
        ]
    }

    #[test]
    fn a_per_hint_format_overrides_the_global_one() {
        let keymap = four_hints();
        let config = &[
            ("desc_format", "[{desc}]"),
            ("desc_format_frames", "<{desc}>"),
        ];
        assert_eq!(
            rendered(InputMode::Pane, &keymap, config),
            "a [close]|b [fullscreen]|c <frames>|d [pin]"
        );
    }

    #[test]
    fn a_per_hint_format_can_be_scoped_to_one_mode() {
        let keymap = four_hints();
        assert_eq!(
            rendered(
                InputMode::Pane,
                &keymap,
                &[("desc_format_pane_frames", "<{desc}>")]
            ),
            "a close|b fullscreen|c <frames>|d pin"
        );
        // A different mode keeps the global formatting.
        assert_eq!(
            rendered(
                InputMode::Tab,
                &keymap,
                &[("desc_format_pane_frames", "<{desc}>")]
            ),
            "a close|b fullscreen|c frames|d pin"
        );
    }

    #[test]
    fn keys_can_be_replaced_with_a_fixed_string() {
        assert_eq!(
            rendered(InputMode::Pane, &four_hints(), &[("keys_frames", "1-9")]),
            "a close|b fullscreen|1-9 frames|d pin"
        );
    }

    #[test]
    fn an_empty_key_replacement_leaves_the_label_alone() {
        assert_eq!(
            rendered(InputMode::Pane, &four_hints(), &[("keys_frames", "")]),
            "a close|b fullscreen|frames|d pin"
        );
    }

    #[test]
    fn a_replaced_key_is_measured_at_its_real_width() {
        // The replacement is wider than the key it stands for, so fitting has
        // to account for it rather than for the original.
        let wide = rendered(
            InputMode::Pane,
            &four_hints(),
            &[("keys_frames", "mouse wheel"), ("limit", "40")],
        );
        assert!(
            calculate_visible_length(&wide, false) <= 40,
            "over the limit: {:?}",
            wide
        );
    }

    #[test]
    fn a_chord_alias_replaces_its_exact_modifier_combination() {
        // A custom leader key programmed to send Ctrl+Alt+Shift at once
        // should render as one symbol, not three modifier names.
        let keymap = vec![(
            KeyWithModifier::new(BareKey::Char('p'))
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier(),
            vec![Action::CloseFocus],
        )];
        let config = &[
            ("chord_mods_leader", "ctrl+alt+shift"),
            ("chord_alias_leader", "&"),
        ];
        assert_eq!(rendered(InputMode::Pane, &keymap, config), "&p close");
    }

    #[test]
    fn a_chord_alias_ignores_a_partial_modifier_match() {
        // Only Ctrl is held, not the full chord, so the normal modifier name
        // is shown rather than the alias.
        let keymap = vec![(ctrl('p'), vec![Action::CloseFocus])];
        let config = &[
            ("chord_mods_leader", "ctrl+alt+shift"),
            ("chord_alias_leader", "&"),
        ];
        assert_eq!(rendered(InputMode::Pane, &keymap, config), "Ctrl p close");
    }

    #[test]
    fn multiple_chords_apply_independently() {
        let keymap = vec![
            (
                KeyWithModifier::new(BareKey::Char('p'))
                    .with_ctrl_modifier()
                    .with_alt_modifier(),
                vec![Action::CloseFocus],
            ),
            (
                KeyWithModifier::new(BareKey::Char('t'))
                    .with_ctrl_modifier()
                    .with_shift_modifier(),
                vec![Action::TogglePaneFrames],
            ),
        ];
        let config = &[
            ("chord_mods_one", "ctrl+alt"),
            ("chord_alias_one", "&"),
            ("chord_mods_two", "ctrl+shift"),
            ("chord_alias_two", "%"),
        ];
        assert_eq!(
            rendered(InputMode::Pane, &keymap, config),
            "&p close|%t frames"
        );
    }

    #[test]
    fn a_chord_alias_is_measured_at_its_real_width() {
        // The alias is narrower than the spelled-out chord, so a limit that
        // just fits "&p close" (8 columns) would drop this hint entirely if
        // fitting were still measuring the unaliased modifier names.
        let keymap = vec![(
            KeyWithModifier::new(BareKey::Char('p'))
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
                .with_super_modifier(),
            vec![Action::CloseFocus],
        )];
        let text = rendered(
            InputMode::Pane,
            &keymap,
            &[
                ("chord_mods_leader", "ctrl+alt+shift+super"),
                ("chord_alias_leader", "&"),
                ("limit", "8"),
            ],
        );
        assert_eq!(text, "&p close");
    }

    #[test]
    fn an_unrecognized_chord_mods_token_disables_that_chord() {
        let keymap = vec![(ctrl('p'), vec![Action::CloseFocus])];
        let config = &[
            ("chord_mods_leader", "ctrl+foo"),
            ("chord_alias_leader", "&"),
        ];
        assert_eq!(rendered(InputMode::Pane, &keymap, config), "Ctrl p close");
    }

    #[test]
    fn a_chord_mods_without_a_paired_alias_has_no_effect() {
        let keymap = vec![(ctrl('p'), vec![Action::CloseFocus])];
        assert_eq!(
            rendered(InputMode::Pane, &keymap, &[("chord_mods_leader", "ctrl")]),
            "Ctrl p close"
        );
    }

    #[test]
    fn a_hint_can_be_named_by_its_label_as_well_as_its_id() {
        // `close_pane` is the id; `close` is what it displays.
        assert_eq!(
            rendered(InputMode::Pane, &four_hints(), &[("keys_close", "^w")]),
            "^w close|b fullscreen|c frames|d pin"
        );
    }

    #[test]
    fn a_relabelled_hint_is_addressable_by_its_new_label() {
        // Setting a label gives the hint an id of `=<label>`, which nobody
        // wants to type; the slugged label works instead.
        assert_eq!(
            rendered(
                InputMode::Pane,
                &four_hints(),
                &[("label_frames", "swap layout"), ("keys_swap_layout", "><")]
            ),
            "a close|b fullscreen|>< swap layout|d pin"
        );
    }

    #[test]
    fn hints_are_dropped_from_the_right_to_fit() {
        let all = rendered(InputMode::Pane, &four_hints(), &[]);
        assert_eq!(all, "a close|b fullscreen|c frames|d pin");
        // Enough room for the first two only.
        let fitted = rendered(InputMode::Pane, &four_hints(), &[("limit", "20")]);
        assert_eq!(fitted, "a close|b fullscreen");
    }

    #[test]
    fn unpinned_hints_yield_before_hints_pinned_to_the_end() {
        // `pin` is pinned last, so the unpinned middle drops around it.
        let fitted = rendered(
            InputMode::Pane,
            &four_hints(),
            &[("hint_order", "*, pin"), ("limit", "20")],
        );
        assert_eq!(fitted, "a close|d pin");
    }

    #[test]
    fn hints_pinned_to_the_front_are_kept_too() {
        let fitted = rendered(
            InputMode::Pane,
            &four_hints(),
            &[("hint_order", "frames, *, pin"), ("limit", "20")],
        );
        assert_eq!(fitted, "c frames|d pin");
    }

    #[test]
    fn the_leading_group_is_given_up_before_the_trailing_one() {
        // `close, fullscreen, *, pin`: once the `*` is gone the leading group is
        // consumed from its inner edge outward, so the trailing `pin` — the hint
        // most deliberately placed — is the last one standing.
        let order = ("hint_order", "close, fullscreen, *, pin");
        assert_eq!(
            rendered(InputMode::Pane, &four_hints(), &[order, ("limit", "26")]),
            "a close|b fullscreen|d pin"
        );
        assert_eq!(
            rendered(InputMode::Pane, &four_hints(), &[order, ("limit", "20")]),
            "a close|d pin"
        );
        assert_eq!(
            rendered(InputMode::Pane, &four_hints(), &[order, ("limit", "8")]),
            "d pin"
        );
    }

    #[test]
    fn precedence_lt_keeps_the_leading_group_instead() {
        let order = ("hint_order", "close, fullscreen, *, pin");
        let lt = ("hint_precedence", "lt");
        // The trailing `pin` is spent first now, so the leading pair outlives it.
        assert_eq!(
            rendered(
                InputMode::Pane,
                &four_hints(),
                &[order, lt, ("limit", "22")]
            ),
            "a close|b fullscreen"
        );
        assert_eq!(
            rendered(
                InputMode::Pane,
                &four_hints(),
                &[order, lt, ("limit", "10")]
            ),
            "a close"
        );
    }

    #[test]
    fn precedence_defaults_to_keeping_the_trailing_group() {
        let order = ("hint_order", "close, fullscreen, *, pin");
        let explicit = ("hint_precedence", "tl");
        let limit = ("limit", "10");
        assert_eq!(
            rendered(InputMode::Pane, &four_hints(), &[order, explicit, limit]),
            rendered(InputMode::Pane, &four_hints(), &[order, limit])
        );
        assert_eq!(
            rendered(InputMode::Pane, &four_hints(), &[order, limit]),
            "d pin"
        );
    }

    #[test]
    fn precedence_still_leaves_the_dropped_hints_in_one_run() {
        // Whichever group is spent first, the gap stays contiguous.
        assert_eq!(
            rendered(
                InputMode::Pane,
                &four_hints(),
                &[
                    ("hint_order", "close, fullscreen, *, pin"),
                    ("hint_precedence", "lt"),
                    ("drop_indicator", "…"),
                    ("limit", "22")
                ]
            ),
            "a close|b fullscreen|…"
        );
    }

    #[test]
    fn dropped_hints_stay_one_run_so_a_single_indicator_covers_them() {
        // Each drop extends the same gap rather than opening a new one, so the
        // indicator never needs a twin.
        let order = ("hint_order", "close, fullscreen, *, pin");
        let indicator = ("drop_indicator", "…");
        assert_eq!(
            rendered(
                InputMode::Pane,
                &four_hints(),
                &[order, indicator, ("limit", "30")]
            ),
            "a close|b fullscreen|…|d pin"
        );
        assert_eq!(
            rendered(
                InputMode::Pane,
                &four_hints(),
                &[order, indicator, ("limit", "20")]
            ),
            "a close|…|d pin"
        );
        assert_eq!(
            rendered(
                InputMode::Pane,
                &four_hints(),
                &[order, indicator, ("limit", "10")]
            ),
            "…|d pin"
        );
    }

    #[test]
    fn the_indicator_marks_where_hints_were_dropped() {
        assert_eq!(
            rendered(
                InputMode::Pane,
                &four_hints(),
                &[("drop_indicator", "…"), ("limit", "22")]
            ),
            "a close|b fullscreen|…"
        );
    }

    #[test]
    fn no_indicator_appears_when_everything_fits() {
        assert_eq!(
            rendered(
                InputMode::Pane,
                &four_hints(),
                &[("drop_indicator", "…"), ("limit", "100")]
            ),
            "a close|b fullscreen|c frames|d pin"
        );
    }

    #[test]
    fn the_indicator_is_paid_for_out_of_the_same_budget() {
        // A wide indicator costs room of its own, so it forces a further drop
        // rather than pushing the line over the limit.
        let wide = rendered(
            InputMode::Pane,
            &four_hints(),
            &[("drop_indicator", "<more>"), ("limit", "22")],
        );
        assert!(
            calculate_visible_length(&wide, false) <= 22,
            "over the limit: {:?}",
            wide
        );
        assert!(wide.contains("<more>"), "indicator missing: {:?}", wide);
    }

    #[test]
    fn one_hint_always_survives_for_truncation_to_handle() {
        let fitted = rendered(InputMode::Pane, &four_hints(), &[("limit", "2")]);
        assert_eq!(fitted, "a close");
    }

    #[test]
    fn width_counts_columns_not_characters() {
        // A CJK ideograph is two columns in any terminal.
        assert_eq!(calculate_visible_length("ab", false), 2);
        assert_eq!(calculate_visible_length("字", false), 2);
        assert_eq!(calculate_visible_length("a字b", false), 4);
    }

    #[test]
    fn nerd_font_glyphs_are_ambiguous_width() {
        // U+F060 is the arrow glyph used for `key_alias_left`. It is East Asian
        // Ambiguous: one column by the standard, two in a terminal set up for
        // Nerd Fonts.
        assert_eq!(calculate_visible_length("\u{f060}", false), 1);
        assert_eq!(calculate_visible_length("\u{f060}", true), 2);
    }

    #[test]
    fn escape_sequences_take_no_columns() {
        assert_eq!(calculate_visible_length(&styled_line(), false), 8);
        assert_eq!(calculate_visible_length(&styled_line(), true), 8);
    }

    #[test]
    fn wide_glyphs_are_measured_when_fitting_hints() {
        // Two hints whose labels are wide glyphs. Counting characters would call
        // this 4 columns of label; it is really 8.
        let keymap = vec![
            (key(BareKey::Char('a')), vec![Action::CloseFocus]),
            (key(BareKey::Char('b')), vec![Action::ToggleFocusFullscreen]),
        ];
        // Distinct labels, or sharing one would deliberately fuse them.
        let labels: Vec<(&str, &str)> =
            vec![("label_close_pane", "字左"), ("label_fullscreen", "字右")];
        // Each hint is "a " plus a 4-column label = 6; with the 1-column spacer
        // the pair is 13 columns, though only 9 characters.
        assert_eq!(rendered(InputMode::Pane, &keymap, &labels), "a 字左|b 字右");
        // 12 columns cannot hold both, even though 9 characters would fit.
        let mut fitted = labels.clone();
        fitted.push(("limit", "12"));
        assert_eq!(rendered(InputMode::Pane, &keymap, &fitted), "a 字左");
    }

    fn pane(x: usize, columns: usize, floating: bool) -> PaneInfo {
        PaneInfo {
            pane_x: x,
            pane_columns: columns,
            is_floating: floating,
            ..Default::default()
        }
    }

    fn suppressed_pane(x: usize, columns: usize) -> PaneInfo {
        PaneInfo {
            pane_x: x,
            pane_columns: columns,
            is_suppressed: true,
            ..Default::default()
        }
    }

    fn manifest(panes: Vec<PaneInfo>) -> PaneManifest {
        let mut map = HashMap::new();
        map.insert(0, panes);
        PaneManifest { panes: map }
    }

    #[test]
    fn terminal_width_is_the_right_edge_of_the_widest_pane() {
        // Two panes side by side across an 80-column terminal.
        let panes = vec![pane(0, 40, false), pane(40, 40, false)];
        assert_eq!(terminal_width(&manifest(panes)), Some(80));
    }

    #[test]
    fn floating_panes_do_not_define_the_terminal_width() {
        let panes = vec![pane(0, 80, false), pane(10, 30, true)];
        assert_eq!(terminal_width(&manifest(panes)), Some(80));
    }

    #[test]
    fn suppressed_panes_do_not_define_the_terminal_width() {
        // Suppressed panes stop tracking resizes, so their stale geometry is
        // often the widest. Trusting it pins the width to an old value and the
        // hints are never refitted.
        let panes = vec![
            pane(0, 103, false),
            suppressed_pane(53, 106),
            suppressed_pane(53, 106),
        ];
        assert_eq!(terminal_width(&manifest(panes)), Some(103));
    }

    #[test]
    fn width_is_unknown_when_there_is_nothing_to_measure() {
        assert_eq!(terminal_width(&manifest(vec![])), None);
        assert_eq!(terminal_width(&manifest(vec![pane(0, 0, false)])), None);
    }

    fn limits(
        max_length: usize,
        auto: bool,
        reserve: usize,
        width: Option<usize>,
    ) -> Option<usize> {
        State {
            max_length,
            auto_width: auto,
            reserve_columns: reserve,
            terminal_width: width,
            ..Default::default()
        }
        .length_limit()
    }

    #[test]
    fn auto_width_fits_the_hints_to_the_terminal() {
        assert_eq!(limits(0, true, 0, Some(80)), Some(80));
    }

    #[test]
    fn reserved_columns_are_kept_free_for_the_rest_of_the_bar() {
        assert_eq!(limits(0, true, 30, Some(80)), Some(50));
        // A reserve wider than the terminal floors at zero rather than wrapping.
        assert_eq!(limits(0, true, 200, Some(80)), Some(0));
    }

    #[test]
    fn an_explicit_max_length_is_never_exceeded_on_a_wide_terminal() {
        assert_eq!(limits(40, true, 0, Some(200)), Some(40));
    }

    #[test]
    fn auto_width_still_narrows_below_an_explicit_max_length() {
        assert_eq!(limits(100, true, 0, Some(60)), Some(60));
    }

    #[test]
    fn nothing_is_capped_before_the_first_pane_update() {
        // Auto-fitting waits for a real width rather than guessing one.
        assert_eq!(limits(0, true, 0, None), None);
        assert_eq!(limits(40, true, 0, None), Some(40));
    }

    #[test]
    fn auto_width_off_leaves_only_the_explicit_cap() {
        assert_eq!(limits(0, false, 0, Some(80)), None);
        assert_eq!(limits(40, false, 0, Some(80)), Some(40));
    }

    fn label_config(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn a_mode_scoped_label_beats_the_global_one() {
        let config = label_config(&[
            ("label_mode_normal", "exit"),
            ("label_locked_mode_normal", "unlock"),
        ]);
        assert_eq!(
            label_override("mode_normal", "switch_to_mode_normal", "locked", &config),
            Some(Some("unlock".to_string()))
        );
        // Every other mode still gets the global label.
        assert_eq!(
            label_override("mode_normal", "switch_to_mode_normal", "pane", &config),
            Some(Some("exit".to_string()))
        );
    }

    #[test]
    fn a_mode_scoped_label_works_by_action_signature_too() {
        let config = label_config(&[("label_locked_switch_to_mode_normal", "unlock")]);
        assert_eq!(
            label_override("mode_normal", "switch_to_mode_normal", "locked", &config),
            Some(Some("unlock".to_string()))
        );
    }

    #[test]
    fn concept_id_wins_over_signature_at_the_same_scope() {
        let config = label_config(&[
            ("label_mode_normal", "by id"),
            ("label_switch_to_mode_normal", "by signature"),
        ]);
        assert_eq!(
            label_override("mode_normal", "switch_to_mode_normal", "pane", &config),
            Some(Some("by id".to_string()))
        );
    }

    #[test]
    fn a_mode_scoped_empty_label_hides_the_hint_in_that_mode_only() {
        let config = label_config(&[("label_pane_mode_normal", "")]);
        assert_eq!(
            label_override("mode_normal", "switch_to_mode_normal", "pane", &config),
            Some(None)
        );
        assert_eq!(
            label_override("mode_normal", "switch_to_mode_normal", "tab", &config),
            None
        );
    }

    #[test]
    fn a_mode_scope_can_reinstate_a_hint_hidden_globally() {
        let config = label_config(&[
            ("label_mode_normal", ""),
            ("label_locked_mode_normal", "unlock"),
        ]);
        assert_eq!(
            label_override("mode_normal", "switch_to_mode_normal", "locked", &config),
            Some(Some("unlock".to_string()))
        );
        assert_eq!(
            label_override("mode_normal", "switch_to_mode_normal", "tab", &config),
            Some(None)
        );
    }

    #[test]
    fn no_matching_label_leaves_the_builtin_in_place() {
        let config = label_config(&[("label_quit", "bye")]);
        assert_eq!(
            label_override("mode_normal", "switch_to_mode_normal", "locked", &config),
            None
        );
    }

    #[test]
    fn keys_sharing_a_modifier_stay_in_one_contiguous_run() {
        let mut keys = vec![
            KeyWithModifier::new(BareKey::Char('j')),
            KeyWithModifier::new(BareKey::Char('h')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char('h')),
            KeyWithModifier::new(BareKey::Char('j')).with_ctrl_modifier(),
        ];
        sort_keys(&mut keys, KeyOrder::Qwerty);
        let rendered: Vec<String> = keys
            .iter()
            .map(|k| {
                let prefix = if k.key_modifiers.is_empty() { "" } else { "^" };
                match k.bare_key {
                    BareKey::Char(c) => format!("{}{}", prefix, c),
                    _ => "?".to_string(),
                }
            })
            .collect();
        assert_eq!(rendered, vec!["h", "j", "^h", "^j"]);
    }

    fn state_with_ancestry(ancestry: Vec<&str>) -> State {
        State {
            mode_info: ModeInfo {
                session_ancestry: ancestry.into_iter().map(str::to_owned).collect(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn a_session_with_no_ancestry_is_not_nested() {
        assert!(!state_with_ancestry(vec![]).is_nested());
    }

    #[test]
    fn a_session_with_ancestry_is_nested() {
        assert!(state_with_ancestry(vec!["host"]).is_nested());
    }

    #[test]
    fn is_host_descended_reflects_session_dimmed_only() {
        let mut state = State {
            mode_info: ModeInfo {
                session_dimmed: Some(true),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(state.is_host_descended());

        // session_ascended is a different signal (this session being the
        // nested, not-yet-ascended-into side); it must not trigger the
        // host-descended indicator on its own.
        state.mode_info.session_dimmed = None;
        state.mode_info.session_ascended = Some(true);
        assert!(!state.is_host_descended());
    }

    #[test]
    fn is_host_descended_ignores_the_dim_display_options() {
        // Whether to show the descended placeholder isn't a rendering
        // preference the way dim_when_unfocused/dim_strength are: it stays
        // true even with dimming turned off.
        let state = State {
            mode_info: ModeInfo {
                session_dimmed: Some(true),
                ..Default::default()
            },
            dim_when_unfocused: false,
            dim_strength: 0.0,
            ..Default::default()
        };
        assert!(state.is_host_descended());
        assert_eq!(state.dim_amount(), 0.0);
    }

    fn state_with_dim(
        dim_when_unfocused: bool,
        dim_strength: f32,
        session_ascended: Option<bool>,
        session_dimmed: Option<bool>,
    ) -> State {
        State {
            mode_info: ModeInfo {
                session_ascended,
                session_dimmed,
                ..Default::default()
            },
            dim_when_unfocused,
            dim_strength,
            ..Default::default()
        }
    }

    #[test]
    fn dim_amount_is_zero_when_not_dimmed() {
        assert_eq!(
            state_with_dim(true, 0.5, Some(false), None).dim_amount(),
            0.0
        );
    }

    #[test]
    fn dim_amount_follows_session_ascended() {
        assert_eq!(
            state_with_dim(true, 0.5, Some(true), None).dim_amount(),
            0.5
        );
    }

    #[test]
    fn dim_amount_follows_session_dimmed() {
        assert_eq!(
            state_with_dim(true, 0.7, None, Some(true)).dim_amount(),
            0.7
        );
    }

    #[test]
    fn dim_amount_respects_the_config_toggle() {
        assert_eq!(
            state_with_dim(false, 0.5, Some(true), Some(true)).dim_amount(),
            0.0
        );
    }

    #[test]
    fn dim_color_blends_rgb_toward_gray() {
        // Pure red's BT.601 luminance is 0.299*255 = 76.245; the dim target
        // is 30% of that (~22.9), so full strength converges there on all
        // three channels: darker than the color's own brightness, not just
        // desaturated to it.
        let red = PaletteColor::Rgb((255, 0, 0));
        assert_eq!(dim_color(red, 0.0), red);
        assert_eq!(dim_color(red, 1.0), PaletteColor::Rgb((23, 23, 23)));
        assert_eq!(dim_color(red, 0.5), PaletteColor::Rgb((139, 11, 11)));

        // A color that's already gray has nothing to desaturate, but still
        // darkens: its luminance equals every channel already, but the dim
        // target is a fraction of that.
        let white = PaletteColor::Rgb((255, 255, 255));
        assert_eq!(dim_color(white, 1.0), PaletteColor::Rgb((77, 77, 77)));
    }

    #[test]
    fn dim_color_passes_through_indexed_colors() {
        let indexed = PaletteColor::EightBit(200);
        assert_eq!(dim_color(indexed, 0.8), indexed);
    }

    /// Test helper: build the `HintStyle` `render_descended_indicator`
    /// expects and render it, so individual tests only vary the ascend
    /// keys and the format overrides they're checking.
    fn rendered_descended_indicator(
        ascend_keys: Vec<KeyWithModifier>,
        key_format: Option<&str>,
        desc_format: Option<&str>,
        config: &BTreeMap<String, String>,
    ) -> String {
        let mode_info = ModeInfo {
            nested_ascend_keys: ascend_keys,
            ..Default::default()
        };
        let colors = Styling::default();
        let hint_order = HintOrder::default();
        let style = HintStyle {
            colors: &colors,
            dim: 0.0,
            key_format,
            desc_format,
            spacer: None,
            discover: false,
            direction_keys: DirectionKeys::default(),
            key_order: KeyOrder::Unsorted,
            mode: "normal",
            hint_order: &hint_order,
            limit: None,
            wide_ambiguous: false,
            drop_indicator: None,
            precedence: HintPrecedence::default(),
            shared: &[],
            config,
        };
        render_descended_indicator(&mode_info, &style)
    }

    fn ascend_keys() -> Vec<KeyWithModifier> {
        vec![
            KeyWithModifier::new(BareKey::Char('o')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char(']')),
        ]
    }

    #[test]
    fn descended_indicator_is_empty_without_ascend_keys() {
        assert_eq!(
            rendered_descended_indicator(vec![], None, None, &BTreeMap::new()),
            ""
        );
    }

    #[test]
    fn descended_indicator_names_the_ascend_keys_in_order() {
        // Unsorted key_order: these are a press-this-then-that sequence, not
        // an alternative-key set a hint's usual sort_keys should reorder.
        let styled = rendered_descended_indicator(
            ascend_keys(),
            Some("{key}"),
            Some("{desc}"),
            &BTreeMap::new(),
        );
        assert_eq!(styled, "Ctrl o ]return to host");
    }

    #[test]
    fn descended_indicator_is_styled_not_plain_text() {
        // Regression test: this used to be printed as bare, unstyled text,
        // the only thing on either bar that didn't carry the theme's
        // colors, and the one thing that visibly didn't fade when
        // dim_when_unfocused grayed everything else out around it.
        let styled = rendered_descended_indicator(ascend_keys(), None, None, &BTreeMap::new());

        assert!(styled.contains("return to host"));
        assert!(styled.contains('\u{1b}'), "expected ANSI escape codes");
    }

    #[test]
    fn descended_indicator_honors_key_format_and_desc_format() {
        // Rendered as an ordinary hint, so the same key_format/desc_format
        // that style the curated hints style this one too.
        let styled = rendered_descended_indicator(
            ascend_keys(),
            Some("#[fg=#ff0000]<{key}>"),
            Some("#[fg=#00ff00]({desc})"),
            &BTreeMap::new(),
        );
        assert_eq!(
            styled,
            "\u{1b}[38;2;255;0;0m<Ctrl o ]>\u{1b}[38;2;0;255;0m(return to host)\u{1b}[0m"
        );
    }

    #[test]
    fn descended_indicator_honors_a_per_hint_desc_format_override() {
        // label_descended/desc_format_descended/keys_descended all apply
        // via the normal hint_override machinery, same as any other hint.
        let config = BTreeMap::from([("desc_format_descended".to_owned(), "{desc}!".to_owned())]);
        let styled =
            rendered_descended_indicator(ascend_keys(), Some("{key}"), Some("{desc}"), &config);
        assert_eq!(styled, "Ctrl o ]return to host!");
    }

    #[test]
    fn mode_prefix_defaults_to_the_ribbon_selected_pill() {
        let colors = Styling::default();
        let parts = render_mode_prefix(InputMode::Pane, &colors, None, &BTreeMap::new(), 0.0);
        let rendered = ANSIStrings(&parts).to_string();

        assert!(rendered.contains("PANE"));
        assert!(rendered.contains('\u{1b}'), "expected ANSI escape codes");
    }

    #[test]
    fn mode_prefix_honors_a_custom_format() {
        let colors = Styling::default();
        let parts = render_mode_prefix(
            InputMode::Locked,
            &colors,
            Some("#[fg=#ff0000]<{mode}>"),
            &BTreeMap::new(),
            0.0,
        );
        let rendered = ANSIStrings(&parts).to_string();

        assert!(rendered.contains("<LOCKED>"));
    }

    #[test]
    fn mode_prefix_per_mode_override_wins_over_the_global_format() {
        let colors = Styling::default();
        let config = BTreeMap::from([(
            "mode_format_locked".to_owned(),
            "[locked: {mode}]".to_owned(),
        )]);

        // Locked has its own override, so it ignores the global format...
        let locked = render_mode_prefix(InputMode::Locked, &colors, Some("<{mode}>"), &config, 0.0);
        assert_eq!(ANSIStrings(&locked).to_string(), "[locked: LOCKED]");

        // ...while a mode without its own override still falls back to it.
        let pane = render_mode_prefix(InputMode::Pane, &colors, Some("<{mode}>"), &config, 0.0);
        assert_eq!(ANSIStrings(&pane).to_string(), "<PANE>");
    }

    #[test]
    fn mode_prefix_custom_format_dims_the_same_as_the_default_pill() {
        // The bug this guards: a custom mode_format resolves its own literal
        // colors through format::render_template, which used to ignore the
        // dim amount entirely, so a hand-styled mode pill stayed at full
        // brightness while everything else around it dimmed.
        let colors = Styling::default();
        let parts = render_mode_prefix(
            InputMode::Normal,
            &colors,
            Some("#[fg=#ff0000]{mode}"),
            &BTreeMap::new(),
            1.0,
        );
        let rendered = ANSIStrings(&parts).to_string();

        // Full dim strength on pure red lands on the same dark gray
        // dim_color/dim_rgb produce elsewhere (see dim_color_blends_rgb_toward_gray).
        assert!(rendered.contains("\u{1b}[38;2;23;23;23m"));
    }

    #[test]
    fn mode_config_suffix_matches_zjstatus_naming() {
        assert_eq!(mode_config_suffix(InputMode::Normal), "normal");
        assert_eq!(mode_config_suffix(InputMode::EnterSearch), "enter_search");
        assert_eq!(mode_config_suffix(InputMode::RenameTab), "rename_tab");
        assert_eq!(mode_config_suffix(InputMode::RenamePane), "rename_pane");
    }

    /// The pane a nested session runs in throughout these tests.
    const GUEST_PANE: PaneId = PaneId::Terminal(7);
    /// A second one, for checking that two nested sessions stay apart.
    const OTHER_GUEST_PANE: PaneId = PaneId::Terminal(9);

    /// A `PaneInfo` for a pane a nested session runs in, focused or not.
    fn guest_pane(pane_id: PaneId, is_focused: bool) -> PaneInfo {
        let (id, is_plugin) = match pane_id {
            PaneId::Terminal(id) => (id, false),
            PaneId::Plugin(id) => (id, true),
        };
        PaneInfo {
            id,
            is_plugin,
            is_focused,
            ..Default::default()
        }
    }

    /// A `TabInfo` for the one tab these tests use.
    fn only_tab() -> TabInfo {
        TabInfo {
            position: 0,
            active: true,
            ..Default::default()
        }
    }

    /// A keybinding table a nested session might report: one binding in Normal
    /// mode that is unmistakable in a rendered hint line, and one in Pane mode
    /// so mode-specific rendering can be told apart from the base mode's.
    fn guest_keybinds() -> KeybindsVec {
        vec![
            (
                InputMode::Normal,
                vec![(
                    ctrl('g'),
                    vec![Action::SwitchToMode {
                        input_mode: InputMode::Pane,
                    }],
                )],
            ),
            (
                InputMode::Pane,
                vec![(key(BareKey::Char('x')), vec![Action::CloseFocus])],
            ),
        ]
    }

    /// A host that has descended into a nested session in `GUEST_PANE`, with
    /// the focus and tab events that identify which pane that is.
    fn state_descended_into_guest() -> State {
        let mut state = State {
            mode_info: ModeInfo {
                session_dimmed: Some(true),
                ..Default::default()
            },
            ..Default::default()
        };
        state.update(Event::TabUpdate(vec![only_tab()]));
        state.update(Event::PaneUpdate(manifest(vec![
            guest_pane(GUEST_PANE, true),
            guest_pane(OTHER_GUEST_PANE, false),
        ])));
        state
    }

    fn guest_mode_update(pane_id: PaneId, mode: InputMode) -> Event {
        Event::NestedSessionModeUpdate {
            pane_id,
            session_name: Some("guest".to_string()),
            mode,
            base_mode: Some(InputMode::Normal),
        }
    }

    fn guest_keybinds_event(pane_id: PaneId) -> Event {
        Event::NestedSessionKeybinds {
            pane_id,
            session_name: Some("guest".to_string()),
            keybinds: guest_keybinds(),
        }
    }

    /// The bar this state renders, with the styling stripped out.
    ///
    /// `render` talks to the host, so this drives the same branch selection
    /// against the state's own fields instead.
    fn rendered_bar(state: &State) -> String {
        let text = match state.bar_subject() {
            BarSubject::Hints(subject) => state.render_hint_line(&subject, 0.0),
            BarSubject::Nothing => String::new(),
            BarSubject::DescendedIndicator => DESCENDED_HINT_LABEL.to_string(),
        };
        let mut parser = AnsiParser::new(&text);
        let mut visible = String::new();
        while let Some(segment) = parser.next_segment() {
            if let AnsiSegment::VisibleChar(ch) = segment {
                visible.push(ch);
            }
        }
        visible
    }

    #[test]
    fn a_nested_session_is_asked_for_its_keybindings_only_once() {
        let mut state = State::default();
        state.update(guest_mode_update(GUEST_PANE, InputMode::Normal));
        assert!(
            state.nested_guests[&GUEST_PANE].keybinds_requested,
            "the first word from a nested session should trigger the request"
        );

        // Every later event from the same session finds the flag already set,
        // so nothing asks again however much that session talks.
        for mode in [InputMode::Pane, InputMode::Tab, InputMode::Normal] {
            state.update(guest_mode_update(GUEST_PANE, mode));
        }
        state.update(guest_keybinds_event(GUEST_PANE));
        assert_eq!(state.nested_guests.len(), 1);
        assert!(state.nested_guests[&GUEST_PANE].keybinds_requested);
    }

    #[test]
    fn two_nested_sessions_are_kept_apart_by_their_panes() {
        let mut state = State::default();
        state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));
        state.update(guest_keybinds_event(GUEST_PANE));
        state.update(guest_mode_update(OTHER_GUEST_PANE, InputMode::Tab));

        assert_eq!(state.nested_guests[&GUEST_PANE].mode, InputMode::Pane);
        assert_eq!(state.nested_guests[&OTHER_GUEST_PANE].mode, InputMode::Tab);
        // Keybindings went to one session only; the other's are still pending.
        assert!(!state.nested_guests[&GUEST_PANE].keybinds.is_empty());
        assert!(state.nested_guests[&OTHER_GUEST_PANE].keybinds.is_empty());
    }

    #[test]
    fn the_descended_into_session_is_the_one_in_the_focused_pane() {
        let mut state = state_descended_into_guest();
        state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));
        state.update(guest_keybinds_event(GUEST_PANE));
        state.update(guest_mode_update(OTHER_GUEST_PANE, InputMode::Tab));
        state.update(guest_keybinds_event(OTHER_GUEST_PANE));

        let descended = state.descended_guest().expect("a descended-into session");
        assert_eq!(descended.mode, InputMode::Pane);
    }

    #[test]
    fn a_focused_floating_pane_wins_only_while_the_floating_layer_is_up() {
        let mut state = State::default();
        let mut floating_guest = guest_pane(OTHER_GUEST_PANE, true);
        floating_guest.is_floating = true;
        state.update(Event::PaneUpdate(manifest(vec![
            guest_pane(GUEST_PANE, true),
            floating_guest,
        ])));

        state.update(Event::TabUpdate(vec![only_tab()]));
        assert_eq!(state.focused_pane_id(), Some(GUEST_PANE));

        state.update(Event::TabUpdate(vec![TabInfo {
            are_floating_panes_visible: true,
            ..only_tab()
        }]));
        assert_eq!(state.focused_pane_id(), Some(OTHER_GUEST_PANE));
    }

    #[test]
    fn a_focused_pane_in_another_tab_is_not_the_focused_pane() {
        let mut state = State::default();
        let mut panes = HashMap::new();
        panes.insert(0, vec![guest_pane(GUEST_PANE, true)]);
        panes.insert(1, vec![guest_pane(OTHER_GUEST_PANE, true)]);
        state.update(Event::PaneUpdate(PaneManifest { panes }));
        state.update(Event::TabUpdate(vec![
            TabInfo {
                position: 0,
                active: false,
                ..Default::default()
            },
            TabInfo {
                position: 1,
                active: true,
                ..Default::default()
            },
        ]));
        assert_eq!(state.focused_pane_id(), Some(OTHER_GUEST_PANE));
    }

    #[test]
    fn descending_renders_the_nested_session_s_own_hints() {
        let mut state = state_descended_into_guest();
        state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));
        state.update(guest_keybinds_event(GUEST_PANE));

        // The nested session's Pane-mode binding, which this session's own
        // (empty) keybindings could not have produced.
        assert!(
            rendered_bar(&state).contains("close"),
            "expected the nested session's Pane-mode hints, got {:?}",
            rendered_bar(&state)
        );
    }

    #[test]
    fn the_nested_session_s_mode_decides_which_of_its_hints_show() {
        let mut state = state_descended_into_guest();
        state.update(guest_keybinds_event(GUEST_PANE));

        state.update(guest_mode_update(GUEST_PANE, InputMode::Normal));
        let normal = rendered_bar(&state);
        state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));
        let pane = rendered_bar(&state);

        assert!(normal.contains("pane"), "got {:?}", normal);
        assert!(pane.contains("close"), "got {:?}", pane);
        assert_ne!(normal, pane);
    }

    #[test]
    fn ascending_returns_the_bar_to_this_session_s_own_hints() {
        let mut state = state_descended_into_guest();
        state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));
        state.update(guest_keybinds_event(GUEST_PANE));
        assert!(state.descended_guest_mode_info().is_some());

        // Ascending is this session no longer being the dimmed side. The
        // nested session is still there and still reporting; it is just not
        // where the keys are going any more.
        state.mode_info.session_dimmed = Some(false);
        assert!(state.descended_guest_mode_info().is_none());
        assert!(state.nested_guests.contains_key(&GUEST_PANE));
    }

    #[test]
    fn a_nested_session_with_no_keybindings_yet_falls_back_to_the_indicator() {
        let mut state = state_descended_into_guest();
        // Mode reported, keybinding request still in flight (or never going to
        // be answered, by a nested session running an older Zellij).
        state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));

        assert!(
            state.descended_guest_mode_info().is_none(),
            "with no bindings there is nothing to draw hints from, so the \
             descended-into indicator should be what renders"
        );
    }

    #[test]
    fn descending_into_a_session_that_never_reported_falls_back_to_the_indicator() {
        let state = state_descended_into_guest();
        assert!(state.descended_guest().is_none());
        assert!(state.descended_guest_mode_info().is_none());
    }

    #[test]
    fn hide_in_base_mode_does_not_blank_the_bar_while_descended() {
        let mut state = state_descended_into_guest();
        state.hide_in_base_mode = true;
        state.update(guest_mode_update(GUEST_PANE, InputMode::Normal));
        state.update(guest_keybinds_event(GUEST_PANE));

        // The nested session sits in its base mode, and so does this one. Being
        // descended is the out-of-the-ordinary state the option is there to
        // stay out of the way of, so the hints show anyway.
        assert!(rendered_bar(&state).contains("pane"));
    }

    #[test]
    fn hiding_the_bar_in_a_nested_session_still_wins_over_a_nested_session_of_its_own() {
        let mut state = state_descended_into_guest();
        state.mode_info.session_ancestry = vec!["host".to_string()];
        state.hide_when_nested = true;
        state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));
        state.update(guest_keybinds_event(GUEST_PANE));

        // A session in the middle of a chain has no bar at all, so there is
        // nowhere to put the hints of the session below it either.
        assert_eq!(rendered_bar(&state), "");
    }

    #[test]
    fn covering_the_host_s_bar_gives_a_nested_session_its_own_back() {
        let mut state = State {
            mode_info: ModeInfo {
                session_ancestry: vec!["host".to_string()],
                keybinds: guest_keybinds(),
                ..Default::default()
            },
            hide_when_nested: true,
            ..Default::default()
        };

        // The host is drawing a bar of its own just below this one, which is
        // what `hide_when_nested` defers to.
        assert_eq!(rendered_bar(&state), "");

        // The host has expanded this session over its whole display and taken
        // its own bar off the screen, so deferring to it would leave no hints
        // anywhere.
        state.mode_info.host_fullscreen = Some(true);
        assert!(rendered_bar(&state).contains("pane"));
    }

    #[test]
    fn covering_the_host_s_bar_surfaces_the_hints_of_the_session_below() {
        let mut state = state_descended_into_guest();
        state.mode_info.session_ancestry = vec!["host".to_string()];
        state.mode_info.host_fullscreen = Some(true);
        state.hide_when_nested = true;
        state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));
        state.update(guest_keybinds_event(GUEST_PANE));

        // This session is the only one left with a bar, and the keys the user
        // is pressing belong to the session below it, so that is what it draws.
        assert!(rendered_bar(&state).contains("close"));
    }

    #[test]
    fn an_ordinary_fullscreen_in_the_host_leaves_a_nested_bar_hidden() {
        let state = State {
            mode_info: ModeInfo {
                session_ancestry: vec!["host".to_string()],
                keybinds: guest_keybinds(),
                // Zellij reports nothing here for the fullscreen that expands a
                // pane over the viewport only: the host's bar is still up.
                host_fullscreen: None,
                ..Default::default()
            },
            hide_when_nested: true,
            ..Default::default()
        };

        assert_eq!(rendered_bar(&state), "");
    }

    #[test]
    fn an_empty_bar_gives_its_row_back_and_a_filled_one_takes_it_again() {
        let mut state = State {
            collapse_when_empty: true,
            ..Default::default()
        };

        assert_eq!(state.next_collapsed(true), Some(true));
        assert_eq!(state.next_collapsed(false), Some(false));
    }

    #[test]
    fn the_row_is_only_spoken_for_when_the_answer_changes() {
        let mut state = State {
            collapse_when_empty: true,
            ..Default::default()
        };

        assert_eq!(
            state.next_collapsed(false),
            None,
            "Zellij already places the pane expanded, so the first full bar has nothing to say"
        );
        assert_eq!(state.next_collapsed(true), Some(true));
        assert_eq!(state.next_collapsed(true), None);
    }

    #[test]
    fn keeping_the_row_reserved_leaves_an_empty_bar_alone() {
        let mut state = State {
            collapse_when_empty: false,
            ..Default::default()
        };

        assert_eq!(state.next_collapsed(true), None);
    }

    #[test]
    fn turning_the_setting_off_hands_the_row_back_without_waiting_for_hints() {
        let mut state = State {
            collapse_when_empty: true,
            ..Default::default()
        };
        assert_eq!(state.next_collapsed(true), Some(true));

        // A reconfigure while the bar is still empty. Leaving the row collapsed
        // until something happened to fill it would look like the setting had
        // been ignored.
        state.collapse_when_empty = false;
        assert_eq!(state.next_collapsed(true), Some(false));
    }

    #[test]
    fn a_nested_session_is_forgotten_when_its_pane_closes() {
        let mut state = state_descended_into_guest();
        state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));
        state.update(guest_keybinds_event(GUEST_PANE));

        state.update(Event::PaneUpdate(manifest(vec![guest_pane(
            OTHER_GUEST_PANE,
            true,
        )])));
        assert!(
            state.nested_guests.is_empty(),
            "a pane id Zellij reuses must not inherit the old session's hints"
        );
    }

    #[test]
    fn nothing_about_a_nested_session_disturbs_a_session_hosting_none() {
        let mut state = State {
            mode_info: ModeInfo {
                keybinds: guest_keybinds(),
                mode: InputMode::Pane,
                base_mode: Some(InputMode::Normal),
                ..Default::default()
            },
            ..Default::default()
        };
        let before = rendered_bar(&state);
        state.update(Event::TabUpdate(vec![only_tab()]));
        state.update(Event::PaneUpdate(manifest(vec![guest_pane(
            GUEST_PANE, true,
        )])));

        assert!(state.nested_guests.is_empty());
        assert_eq!(rendered_bar(&state), before);
    }

    /// Build a plugin configuration from `key = value` pairs.
    fn config(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn an_unconfigured_pipe_publishes_on_both_the_current_and_the_old_name() {
        assert_eq!(
            pipe_names_from_config(&config(&[])),
            vec!["zjhints".to_string(), "zjstatus_hints".to_string()]
        );
    }

    #[test]
    fn a_named_pipe_publishes_on_that_name_alone() {
        assert_eq!(
            pipe_names_from_config(&config(&[("pipe_name", "hints")])),
            vec!["hints".to_string()]
        );
    }

    #[test]
    fn naming_the_old_pipe_explicitly_does_not_publish_it_twice() {
        assert_eq!(
            pipe_names_from_config(&config(&[("pipe_name", "zjstatus_hints")])),
            vec!["zjstatus_hints".to_string()]
        );
    }
}
