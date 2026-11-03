use super::*;
use std::collections::HashMap;
use zjkit::ansi::{AnsiParser, AnsiSegment};
use zjkit::bar::Handled;
use zjkit::host::{Call, Recorder};

fn cfg(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// A bar loaded with `pairs` for its configuration.
fn loaded(pairs: &[(&str, &str)]) -> State {
    let mut state = State::default();
    state.load(cfg(pairs));
    state
}

// Permissions.

#[test]
fn loading_the_bar_asks_for_its_permissions_and_events() {
    let state = loaded(&[]);
    let calls = state.host.calls();
    assert!(calls.contains(&Call::RequestPermission(BAR_PERMISSIONS.to_vec())));
    assert!(calls.contains(&Call::Subscribe(BAR_EVENTS.to_vec())));
    // No mouse events, as the bar takes no clicks.
    let subscriptions = calls
        .iter()
        .filter(|call| matches!(call, Call::Subscribe(_)))
        .count();
    assert_eq!(subscriptions, 1);
}

#[test]
fn with_no_feature_on_the_bar_asks_only_for_what_it_always_needs() {
    assert_eq!(bar_permissions(&[]), BAR_PERMISSIONS.to_vec());
    assert_eq!(
        BarPlugin::permissions(&loaded(&[]), &BAR),
        BAR_PERMISSIONS.to_vec()
    );
}

#[test]
fn each_feature_adds_its_own_permissions_once() {
    for feature in Feature::ALL {
        let permissions = bar_permissions(&[feature]);
        assert_eq!(permissions[..BAR_PERMISSIONS.len()], BAR_PERMISSIONS);
        for permission in feature.permissions() {
            assert!(permissions.contains(permission), "{feature:?}");
        }
        assert_eq!(
            permissions.len(),
            BAR_PERMISSIONS.len() + feature.permissions().len(),
            "{feature:?}"
        );
    }
    // Two features that need the same permission ask for it once.
    let both = bar_permissions(&[Feature::LauncherCommand, Feature::ClockCommand]);
    let mut expected = BAR_PERMISSIONS.to_vec();
    expected.push(PermissionType::RunCommands);
    assert_eq!(both, expected);
}

#[test]
fn only_the_features_that_run_commands_ask_to() {
    for feature in Feature::ALL {
        let runs_commands = matches!(feature, Feature::LauncherCommand | Feature::ClockCommand);
        assert_eq!(
            bar_permissions(&[feature]).contains(&PermissionType::RunCommands),
            runs_commands,
            "{feature:?}"
        );
    }
}

#[test]
fn the_settings_screen_asks_for_the_permissions_of_a_bar_with_every_feature_on() {
    let every_feature = bar_permissions(&Feature::ALL);
    assert_eq!((settings::SCHEMA.permissions)(), every_feature);
    assert_eq!((BAR.permissions)(true), every_feature);
    for permission in [
        PermissionType::ReadApplicationState,
        PermissionType::ChangeApplicationState,
        PermissionType::MessageAndLaunchOtherPlugins,
        PermissionType::RunActionsAsUser,
        PermissionType::OpenTerminalsOrPlugins,
        PermissionType::RunCommands,
    ] {
        assert_eq!(
            every_feature
                .iter()
                .filter(|asked| **asked == permission)
                .count(),
            1,
            "{permission:?}"
        );
    }
}

#[test]
fn new_options_with_no_feature_ask_for_nothing_more() {
    let mut state = loaded(&[]);
    state.host.clear();
    assert!(BarCore::follow(
        &mut state,
        &BAR,
        Handled::Reconfigure(cfg(&[("pane_scope", "tab")]))
    ));
    assert_eq!(state.host.calls(), vec![]);
}

// Options.

#[test]
fn unset_options_take_their_defaults() {
    let state = loaded(&[]);
    assert_eq!(state.pane_scope, PaneScope::All);
    assert_eq!(PaneScope::of(DEFAULT_PANE_SCOPE), PaneScope::All);
    assert_eq!(state.entry_format, None);
    assert_eq!(state.focused_entry_format, None);
    assert_eq!(state.minimized_entry_format, None);
    assert_eq!(state.entry_spacer, DEFAULT_ENTRY_SPACER);
    assert_eq!(state.tab_separator, DEFAULT_TAB_SEPARATOR);
    assert_eq!(state.empty_format, None);
    assert_eq!(
        state.flag_formats,
        [
            DEFAULT_MINIMIZED_FORMAT,
            DEFAULT_EXITED_FORMAT,
            DEFAULT_PLUGIN_FORMAT,
            DEFAULT_OTHER_TAB_FORMAT,
        ]
    );
    assert_eq!(state.overflow_left_format, None);
    assert_eq!(state.overflow_right_format, None);
    assert_eq!(state.overflow_str, DEFAULT_OVERFLOW_STR);
    assert_eq!(state.auto_width, DEFAULT_AUTO_WIDTH);
    assert_eq!(state.max_length, DEFAULT_MAX_LENGTH);
    assert!(!state.wide_ambiguous);
    assert_eq!(state.hide_when_nested, DEFAULT_HIDE_WHEN_NESTED);
    assert_eq!(state.collapse_when_empty, DEFAULT_COLLAPSE_WHEN_EMPTY);
    assert_eq!(state.dim_when_unfocused, DEFAULT_DIM_WHEN_UNFOCUSED);
    assert_eq!(state.dim_strength, DEFAULT_DIM_STRENGTH);
}

#[test]
fn every_option_is_read_from_its_key() {
    let state = loaded(&[
        ("pane_scope", "tab"),
        ("entry_format", "{title}"),
        ("focused_entry_format", "[{title}]"),
        ("minimized_entry_format", "_{title}_"),
        ("entry_spacer", "|"),
        ("tab_separator", "||"),
        ("empty_format", "none"),
        ("minimized_format", "M"),
        ("exited_format", "E"),
        ("plugin_format", "P"),
        ("other_tab_format", "@{tab}"),
        ("overflow_left_format", "<{count}"),
        ("overflow_right_format", "{count}>"),
        ("overflow_str", "~"),
        ("auto_width", "false"),
        ("max_length", "40"),
        ("ambiguous_width", "2"),
        ("hide_when_nested", "false"),
        ("collapse_when_empty", "false"),
        ("dim_when_unfocused", "false"),
        ("dim_strength", "0.25"),
    ]);
    assert_eq!(state.pane_scope, PaneScope::Tab);
    assert_eq!(state.entry_format.as_deref(), Some("{title}"));
    assert_eq!(state.focused_entry_format.as_deref(), Some("[{title}]"));
    assert_eq!(state.minimized_entry_format.as_deref(), Some("_{title}_"));
    assert_eq!(state.entry_spacer, "|");
    assert_eq!(state.tab_separator, "||");
    assert_eq!(state.empty_format.as_deref(), Some("none"));
    assert_eq!(state.flag_formats, ["M", "E", "P", "@{tab}"]);
    assert_eq!(state.overflow_left_format.as_deref(), Some("<{count}"));
    assert_eq!(state.overflow_right_format.as_deref(), Some("{count}>"));
    assert_eq!(state.overflow_str, "~");
    assert!(!state.auto_width);
    assert_eq!(state.max_length, 40);
    assert!(state.wide_ambiguous);
    assert!(!state.hide_when_nested);
    assert!(!state.collapse_when_empty);
    assert!(!state.dim_when_unfocused);
    assert_eq!(state.dim_strength, 0.25);
}

#[test]
fn pane_scope_is_read_in_any_case_and_falls_back_to_all() {
    for (value, scope) in [
        ("tab", PaneScope::Tab),
        ("TAB", PaneScope::Tab),
        ("all", PaneScope::All),
        ("bogus", PaneScope::All),
        ("", PaneScope::All),
    ] {
        assert_eq!(
            loaded(&[("pane_scope", value)]).pane_scope,
            scope,
            "{value:?}"
        );
    }
}

#[test]
fn an_empty_format_means_zellij_s_look_but_an_empty_spacer_separator_or_flag_means_none() {
    let state = loaded(&[
        ("format_theme", "flat"),
        ("entry_format", ""),
        ("entry_spacer", ""),
        ("tab_separator", ""),
        ("plugin_format", ""),
    ]);
    assert_eq!(state.entry_format, None);
    assert!(state.focused_entry_format.is_some());
    assert_eq!(state.entry_spacer, "");
    assert_eq!(state.tab_separator, "");
    assert_eq!(state.flag_formats[2], "");
}

#[test]
fn the_themes_are_laid_under_the_user_s_options() {
    let state = loaded(&[
        ("format_theme", "pills"),
        ("alias_theme", "ascii"),
        ("focused_entry_format", "*{title}*"),
    ]);
    assert_eq!(state.focused_entry_format.as_deref(), Some("*{title}*"));
    assert!(state
        .entry_format
        .as_deref()
        .is_some_and(|format| format.contains(zjkit::theme::PILL_LEFT)));
    assert_eq!(state.flag_formats, [" _", " X", " P", " @{tab}"]);
    assert_ne!(state.overflow_str, DEFAULT_OVERFLOW_STR);
}

#[test]
fn hidden_hides_the_bar() {
    let state = loaded(&[("hidden", "true")]);
    assert!(state.bar.visibility.is_hidden());
}

// The panes and tabs the bar is shown.

/// Formats with no styling, so a test can read the line as plain text.
const PLAIN: [(&str, &str); 11] = [
    ("entry_format", "{title}{flags}"),
    ("focused_entry_format", "[{title}{flags}]"),
    ("minimized_entry_format", "_{title}{flags}_"),
    ("entry_spacer", "|"),
    ("tab_separator", "/"),
    ("minimized_format", ""),
    ("exited_format", "!"),
    ("plugin_format", "*"),
    ("other_tab_format", "({tab})"),
    ("overflow_left_format", "<{count}"),
    ("overflow_right_format", "{count}>"),
];

/// `PLAIN` with `pairs` added.
fn plain_with(pairs: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
    let mut all = PLAIN.to_vec();
    all.extend_from_slice(pairs);
    all
}

fn tab(position: usize, name: &str) -> TabInfo {
    TabInfo {
        position,
        name: name.to_string(),
        ..Default::default()
    }
}

/// Tabs named `one`, `two`, `three` and so on, the one at `active_position`
/// active and showing its floating panes.
fn tabs(count: usize, active_position: usize) -> Vec<TabInfo> {
    const NAMES: [&str; 5] = ["one", "two", "three", "four", "five"];
    (0..count)
        .map(|position| {
            let mut info = tab(position, NAMES[position]);
            if position == active_position {
                info.active = true;
                info.are_floating_panes_visible = true;
            }
            info
        })
        .collect()
}

/// A floating terminal pane, `id`, titled `title`.
fn floating(id: u32, title: &str) -> PaneInfo {
    PaneInfo {
        id,
        title: title.to_string(),
        is_floating: true,
        is_selectable: true,
        ..Default::default()
    }
}

fn focused(mut pane: PaneInfo) -> PaneInfo {
    pane.is_focused = true;
    pane
}

fn plugin(mut pane: PaneInfo) -> PaneInfo {
    pane.is_plugin = true;
    pane
}

/// `pane` as Zellij reports it once it is hidden from the screen.
fn suppressed(mut pane: PaneInfo) -> PaneInfo {
    pane.is_suppressed = true;
    pane.is_floating = false;
    pane
}

/// The bar's own pane, `width` columns wide.
fn own_pane(width: usize) -> PaneInfo {
    PaneInfo {
        id: Recorder::PLUGIN_ID,
        is_plugin: true,
        pane_content_columns: width,
        ..Default::default()
    }
}

/// A pane update with the bar's own pane `width` columns wide in the first
/// tab, and `panes` in the tab at each position.
fn panes(width: usize, panes: Vec<(usize, Vec<PaneInfo>)>) -> Event {
    let mut by_tab: HashMap<usize, Vec<PaneInfo>> = HashMap::from([(0, vec![own_pane(width)])]);
    for (position, mut infos) in panes {
        by_tab.entry(position).or_default().append(&mut infos);
    }
    Event::PaneUpdate(PaneManifest { panes: by_tab })
}

/// A Zellij palette whose ribbons and text can be told apart in the output.
fn palette() -> Styling {
    let mut styling = Styling::default();
    styling.ribbon_selected.base = PaletteColor::Rgb((1, 2, 3));
    styling.ribbon_selected.background = PaletteColor::Rgb((0xa6, 0xe3, 0xa1));
    styling.ribbon_unselected.base = PaletteColor::Rgb((4, 5, 6));
    styling.ribbon_unselected.background = PaletteColor::Rgb((0x31, 0x32, 0x44));
    styling.text_unselected.base = PaletteColor::Rgb((7, 8, 9));
    styling.text_unselected.background = PaletteColor::Rgb((0x1e, 0x1e, 0x2e));
    styling
}

fn mode() -> ModeInfo {
    let mut mode_info = ModeInfo::default();
    mode_info.style.colors = palette();
    mode_info
}

/// A bar with `pairs` for its configuration, `width` columns wide, showing
/// `infos` for the tabs and `by_tab` for the floating panes.
fn bar(
    pairs: &[(&str, &str)],
    width: usize,
    infos: Vec<TabInfo>,
    by_tab: Vec<(usize, Vec<PaneInfo>)>,
) -> State {
    let mut state = loaded(pairs);
    state.update(Event::ModeUpdate(mode()));
    state.update(panes(width, by_tab));
    state.update(Event::TabUpdate(infos));
    state
}

/// `text` with the styling stripped out.
fn strip_ansi(text: &str) -> String {
    let mut parser = AnsiParser::new(text);
    let mut visible = String::new();
    while let Some(segment) = parser.next_segment() {
        if let AnsiSegment::VisibleChar(ch) = segment {
            visible.push(ch);
        }
    }
    visible
}

/// The line the bar draws, as plain text.
fn shown(state: &State) -> String {
    strip_ansi(&state.draw_bar())
}

// Which panes are listed, and in what order.

#[test]
fn floating_panes_are_listed_and_tiled_and_unselectable_ones_are_not() {
    let tiled = PaneInfo {
        id: 1,
        title: "tiled".to_string(),
        is_selectable: true,
        ..Default::default()
    };
    let mut unselectable = plugin(floating(2, "another bar"));
    unselectable.is_selectable = false;
    let state = bar(
        &PLAIN,
        80,
        tabs(1, 0),
        vec![(0, vec![tiled, unselectable, floating(3, "htop")])],
    );
    assert_eq!(shown(&state), "htop");
}

#[test]
fn the_bar_s_own_pane_is_never_listed() {
    let mut own = own_pane(80);
    own.is_floating = true;
    own.is_selectable = true;
    own.title = "zjtaskbar".to_string();
    let state = bar(
        &PLAIN,
        80,
        tabs(1, 0),
        vec![(0, vec![own, floating(1, "htop")])],
    );
    assert_eq!(shown(&state), "htop");
}

#[test]
fn entries_are_ordered_by_tab_and_then_by_pane_id_terminals_first() {
    let state = bar(
        &PLAIN,
        80,
        tabs(2, 0),
        vec![
            (1, vec![floating(2, "d")]),
            (
                0,
                vec![floating(9, "c"), plugin(floating(7, "p")), floating(4, "b")],
            ),
        ],
    );
    assert_eq!(shown(&state), "b|c|p*/d(two)");
}

#[test]
fn entries_stay_in_place_as_the_focus_moves() {
    let in_tab = |focused_id: u32| {
        let panes = [floating(1, "a"), floating(2, "b"), floating(3, "c")]
            .into_iter()
            .map(|pane| {
                if pane.id == focused_id {
                    focused(pane)
                } else {
                    pane
                }
            })
            .collect();
        vec![(0, panes)]
    };
    let mut state = bar(&PLAIN, 80, tabs(1, 0), in_tab(1));
    assert_eq!(shown(&state), "[a]|b|c");
    assert!(state.update(panes(80, in_tab(3))));
    assert_eq!(shown(&state), "a|b|[c]");
}

#[test]
fn only_the_active_tab_s_focused_floating_pane_while_it_shows_them_is_focused() {
    let by_tab = || {
        vec![
            (0, vec![focused(floating(1, "a"))]),
            (1, vec![focused(floating(2, "b"))]),
        ]
    };
    let state = bar(&PLAIN, 80, tabs(2, 0), by_tab());
    assert_eq!(shown(&state), "[a]/b(two)");
    // With the floating panes put away, the keyboard goes to a tiled pane.
    let mut hidden_layer = tabs(2, 0);
    hidden_layer[0].are_floating_panes_visible = false;
    let state = bar(&PLAIN, 80, hidden_layer, by_tab());
    assert_eq!(shown(&state), "a/b(two)");
}

#[test]
fn panes_in_other_tabs_are_flagged_with_the_tab_s_name_or_number() {
    let mut infos = tabs(3, 1);
    infos[2].name = String::new();
    let state = bar(
        &[],
        200,
        infos,
        vec![
            (0, vec![floating(1, "a")]),
            (1, vec![floating(2, "b")]),
            (2, vec![floating(3, "c")]),
        ],
    );
    let line = shown(&state);
    assert!(line.contains(" a (one) "), "{line:?}");
    assert!(line.contains(" b "), "{line:?}");
    assert!(!line.contains("(two)"), "{line:?}");
    assert!(line.contains(" c (3) "), "{line:?}");
}

#[test]
fn the_tab_scope_lists_only_the_active_tab_s_panes_with_no_other_tab_flags() {
    let by_tab = vec![
        (0, vec![floating(1, "a")]),
        (1, vec![floating(2, "b"), floating(3, "c")]),
    ];
    let state = bar(
        &plain_with(&[("pane_scope", "tab")]),
        80,
        tabs(2, 1),
        by_tab,
    );
    assert_eq!(shown(&state), "b|c");
}

#[test]
fn the_tab_scope_lists_nothing_before_zellij_names_the_active_tab() {
    let mut state = loaded(&plain_with(&[("pane_scope", "tab")]));
    state.update(panes(80, vec![(0, vec![floating(1, "a")])]));
    assert_eq!(shown(&state), "");
    assert!(state.update(Event::TabUpdate(tabs(1, 0))));
    assert_eq!(shown(&state), "a");
}

#[test]
fn changing_the_scope_lists_the_panes_again() {
    let by_tab = vec![(0, vec![floating(1, "a")]), (1, vec![floating(2, "b")])];
    let mut state = bar(&PLAIN, 80, tabs(2, 0), by_tab);
    assert_eq!(shown(&state), "a/b(two)");
    assert!(BarCore::follow(
        &mut state,
        &BAR,
        Handled::Reconfigure(cfg(&plain_with(&[("pane_scope", "tab")])))
    ));
    assert_eq!(shown(&state), "a");
}

#[test]
fn a_pane_the_bar_minimized_stays_listed_while_hidden() {
    let mut state = loaded(&plain_with(&[("minimized_format", "-")]));
    state.minimized.insert(PaneId::Terminal(2));
    state.update(Event::TabUpdate(tabs(1, 0)));
    state.update(panes(
        80,
        vec![(0, vec![floating(1, "a"), suppressed(floating(2, "b"))])],
    ));
    assert_eq!(shown(&state), "a|_b-_");
    // A hidden pane the bar did not minimize is not one it lists.
    state.update(panes(
        80,
        vec![(
            0,
            vec![
                floating(1, "a"),
                suppressed(floating(2, "b")),
                suppressed(floating(3, "c")),
            ],
        )],
    ));
    assert_eq!(shown(&state), "a|_b-_");
}

#[test]
fn a_minimized_pane_shown_again_some_other_way_is_listed_as_any_other() {
    let mut state = loaded(&PLAIN);
    state.minimized.insert(PaneId::Terminal(2));
    state.update(Event::TabUpdate(tabs(1, 0)));
    state.update(panes(80, vec![(0, vec![floating(2, "b")])]));
    assert_eq!(shown(&state), "b");
    // It is still kept, as the update may be one sent before the hide.
    assert!(state.minimized.contains(&PaneId::Terminal(2)));
}

#[test]
fn a_minimized_pane_is_forgotten_once_closed_or_tiled() {
    let mut state = loaded(&PLAIN);
    state.minimized.insert(PaneId::Terminal(1));
    state.minimized.insert(PaneId::Terminal(2));
    let mut tiled = floating(2, "b");
    tiled.is_floating = false;
    state.update(panes(80, vec![(0, vec![tiled])]));
    assert!(state.minimized.is_empty());
}

#[test]
fn exited_and_plugin_panes_are_flagged_in_a_fixed_order() {
    let mut exited = floating(1, "build");
    exited.exited = true;
    let mut exited_plugin = plugin(floating(2, "tool"));
    exited_plugin.exited = true;
    let state = bar(
        &PLAIN,
        80,
        tabs(1, 0),
        vec![(0, vec![exited, exited_plugin])],
    );
    assert_eq!(shown(&state), "build!|tool!*");
}

#[test]
fn each_flag_shows_in_its_own_default_words() {
    let mut exited = floating(1, "build");
    exited.exited = true;
    let mut state = loaded(&[]);
    state.minimized.insert(PaneId::Terminal(3));
    state.update(Event::TabUpdate(tabs(2, 0)));
    state.update(panes(
        200,
        vec![
            (0, vec![exited, suppressed(floating(3, "notes"))]),
            (1, vec![plugin(floating(2, "picker"))]),
        ],
    ));
    assert_eq!(
        shown(&state),
        " build (EXITED)   notes (MINIMIZED)  \u{2502}  picker (PLUGIN) (two) "
    );
}

#[test]
fn entries_can_show_their_index_and_tab() {
    let state = bar(
        &plain_with(&[("entry_format", "{index}:{title}@{tab}")]),
        80,
        tabs(2, 0),
        vec![(0, vec![floating(1, "a")]), (1, vec![floating(2, "b")])],
    );
    assert_eq!(shown(&state), "1:a@one/2:b@two");
}

#[test]
fn a_title_and_a_tab_name_are_drawn_as_text_with_their_control_characters_escaped() {
    let mut infos = tabs(2, 0);
    infos[1].name = "{tab} #[fg=red]".to_string();
    let state = bar(
        &PLAIN,
        80,
        infos,
        vec![
            (0, vec![floating(1, "{title} #[bold]\u{1b}[31m")]),
            (1, vec![floating(2, "b")]),
        ],
    );
    assert_eq!(
        shown(&state),
        "{title} #[bold]\\u{1b}[31m/b({tab} #[fg=red])"
    );
}

#[test]
fn with_no_formats_the_entries_take_zellij_s_colors() {
    let mut state = loaded(&[]);
    state.minimized.insert(PaneId::Terminal(3));
    state.update(Event::ModeUpdate(mode()));
    state.update(Event::TabUpdate(tabs(1, 0)));
    state.update(panes(
        80,
        vec![(
            0,
            vec![
                focused(floating(1, "a")),
                floating(2, "b"),
                suppressed(floating(3, "c")),
            ],
        )],
    ));
    let config = state.bar.configuration.clone();
    let drawn = |format: &str| {
        let parts = zjkit::format::render_template(format, &[], &config, 0.0);
        ansi_term::ANSIStrings(&parts).to_string()
    };
    let expected = [
        drawn("#[fg=#010203,bg=#a6e3a1,bold] a "),
        " ".to_string(),
        drawn("#[fg=#040506,bg=#313244] b "),
        " ".to_string(),
        drawn("#[fg=#070809,bg=#1e1e2e] c (MINIMIZED) "),
    ]
    .concat();
    assert_eq!(state.draw_bar(), expected);
}

#[test]
fn before_zellij_sends_its_theme_the_entries_are_unstyled() {
    let mut state = loaded(&[]);
    state.update(Event::TabUpdate(tabs(1, 0)));
    state.update(panes(80, vec![(0, vec![floating(1, "a")])]));
    assert_eq!(state.draw_bar(), " a ");
}

#[test]
fn a_styled_flag_keeps_the_entry_s_background_under_it() {
    let mut exited = floating(1, "a");
    exited.exited = true;
    let state = bar(
        &[("format_theme", "flat"), ("exited_format", "#[fg=$red] X")],
        80,
        tabs(1, 0),
        vec![(0, vec![exited])],
    );
    let config = &state.bar.configuration;
    let parts = zjkit::format::render_template(
        "#[fg=$fg,bg=$surface] a#[fg=$red,bg=$surface] X#[fg=$fg,bg=$surface] ",
        &[],
        config,
        0.0,
    );
    assert_eq!(state.draw_bar(), ansi_term::ANSIStrings(&parts).to_string());
}

// Hiding, dimming and the empty bar.

#[test]
fn the_bar_dims_while_its_session_is_not_receiving_input() {
    let by_tab = || vec![(0, vec![floating(1, "a"), floating(2, "b")])];
    let undimmed = bar(&[], 80, tabs(1, 0), by_tab()).draw_bar();
    let dimmed_mode = || {
        let mut mode_info = mode();
        mode_info.session_dimmed = Some(true);
        Event::ModeUpdate(mode_info)
    };
    let mut state = bar(&[], 80, tabs(1, 0), by_tab());
    assert!(state.update(dimmed_mode()));
    let dimmed = state.draw_bar();
    assert_ne!(dimmed, undimmed);
    assert_eq!(strip_ansi(&dimmed), strip_ansi(&undimmed));

    let mut state = bar(&[("dim_when_unfocused", "false")], 80, tabs(1, 0), by_tab());
    state.update(dimmed_mode());
    assert_eq!(state.draw_bar(), undimmed);
}

#[test]
fn hidden_draws_nothing() {
    let state = bar(
        &plain_with(&[("hidden", "true")]),
        80,
        tabs(1, 0),
        vec![(0, vec![floating(1, "a")])],
    );
    assert_eq!(state.draw_bar(), "");
}

#[test]
fn a_nested_session_draws_nothing_unless_its_host_s_bars_are_covered() {
    let nested = |host_fullscreen: Option<bool>| {
        let mut mode_info = mode();
        mode_info.session_ancestry = vec!["host".to_string()];
        mode_info.host_fullscreen = host_fullscreen;
        Event::ModeUpdate(mode_info)
    };
    let by_tab = || vec![(0, vec![floating(1, "a")])];
    let mut state = bar(&PLAIN, 80, tabs(1, 0), by_tab());
    assert!(state.update(nested(None)));
    assert_eq!(shown(&state), "");
    assert!(state.update(nested(Some(true))));
    assert_eq!(shown(&state), "a");

    let mut state = bar(
        &plain_with(&[("hide_when_nested", "false")]),
        80,
        tabs(1, 0),
        by_tab(),
    );
    state.update(nested(None));
    assert_eq!(shown(&state), "a");
}

#[test]
fn with_no_floating_panes_the_bar_is_blank_or_draws_empty_format() {
    let state = bar(&PLAIN, 80, tabs(1, 0), vec![]);
    assert_eq!(state.draw_bar(), "");
    let state = bar(
        &plain_with(&[("empty_format", "no floating panes")]),
        80,
        tabs(1, 0),
        vec![],
    );
    assert_eq!(shown(&state), "no floating panes");
    // Cut to fit, like a lone entry.
    let state = bar(
        &plain_with(&[("empty_format", "no floating panes"), ("overflow_str", "~")]),
        6,
        tabs(1, 0),
        vec![],
    );
    assert_eq!(shown(&state), "no fl~");
}

#[test]
fn an_empty_bar_gives_its_row_back_unless_told_to_keep_it() {
    let mut state = bar(&PLAIN, 80, tabs(1, 0), vec![]);
    state.host.clear();
    state.render(1, 80);
    assert_eq!(state.host.calls(), vec![Call::SetCollapsed(true)]);
    state.update(panes(80, vec![(0, vec![floating(1, "a")])]));
    state.host.clear();
    state.render(1, 80);
    assert_eq!(state.host.calls(), vec![Call::SetCollapsed(false)]);

    let mut state = bar(
        &plain_with(&[("collapse_when_empty", "false")]),
        80,
        tabs(1, 0),
        vec![],
    );
    state.host.clear();
    state.render(1, 80);
    assert_eq!(state.host.calls(), vec![]);

    // An empty bar with something to say keeps its row.
    let mut state = bar(
        &plain_with(&[("empty_format", "-")]),
        80,
        tabs(1, 0),
        vec![],
    );
    state.host.clear();
    state.render(1, 80);
    assert_eq!(state.host.calls(), vec![]);
}

// Fitting the entries to the bar.

/// Single-letter floating panes `a`, `b`, `c` and so on in the first tab,
/// the one at `focused_index` focused.
fn letters(count: usize, focused_index: usize) -> Vec<(usize, Vec<PaneInfo>)> {
    let panes = (0..count)
        .map(|index| {
            let title = ((b'a' + index as u8) as char).to_string();
            let pane = floating(index as u32 + 1, &title);
            if index == focused_index {
                focused(pane)
            } else {
                pane
            }
        })
        .collect();
    vec![(0, panes)]
}

#[test]
fn entries_that_do_not_fit_are_counted_by_markers_around_the_focused_one() {
    // Each entry is 1 column, the focused one 3, so 13 columns take three
    // entries and both markers.
    let state = bar(&PLAIN, 13, tabs(1, 0), letters(10, 5));
    assert_eq!(shown(&state), "<4|e|[f]|g|3>");
    // Two columns fewer, and the entry before the focused one gives way.
    assert_eq!(
        shown(&bar(&PLAIN, 11, tabs(1, 0), letters(10, 5))),
        "<5|[f]|g|3>"
    );
    // At the ends, only one side has a marker.
    assert_eq!(
        shown(&bar(&PLAIN, 11, tabs(1, 0), letters(10, 0))),
        "[a]|b|c|7>"
    );
    assert_eq!(
        shown(&bar(&PLAIN, 11, tabs(1, 0), letters(10, 9))),
        "<7|h|i|[j]"
    );
}

#[test]
fn with_none_focused_the_window_starts_at_the_first_entry() {
    let mut infos = tabs(1, 0);
    infos[0].are_floating_panes_visible = false;
    let state = bar(&PLAIN, 7, infos, letters(10, 5));
    assert_eq!(shown(&state), "a|b|8>");
}

#[test]
fn a_separator_wider_than_the_spacer_still_fits_the_line() {
    // Two tabs of three entries; the separator takes three columns where a
    // spacer would take one.
    let by_tab = vec![
        (
            0,
            vec![
                floating(1, "a"),
                floating(2, "b"),
                focused(floating(3, "c")),
            ],
        ),
        (
            1,
            vec![floating(4, "d"), floating(5, "e"), floating(6, "f")],
        ),
    ];
    let options = plain_with(&[("tab_separator", " / "), ("other_tab_format", "")]);
    let wide = bar(&options, 80, tabs(2, 0), by_tab.clone());
    assert_eq!(shown(&wide), "a|b|[c] / d|e|f");
    for width in 5..16 {
        let state = bar(&options, width, tabs(2, 0), by_tab.clone());
        let line = shown(&state);
        assert!(line.chars().count() <= width, "{width}: {line:?}");
        assert!(line.contains("[c]"), "{width}: {line:?}");
    }
}

#[test]
fn the_markers_are_drawn_in_zellij_s_colors_with_no_format() {
    let state = bar(&[("entry_spacer", "|")], 20, tabs(1, 0), letters(10, 5));
    let line = shown(&state);
    assert!(line.starts_with(" \u{2190} +"), "{line:?}");
    assert!(line.ends_with(" \u{2192} "), "{line:?}");
}

#[test]
fn max_length_and_the_pane_width_both_limit_the_line() {
    let state = bar(&PLAIN, 17, tabs(1, 0), letters(10, 5));
    assert_eq!(state.length_limit(), Some(17));
    let capped = plain_with(&[("max_length", "9")]);
    assert_eq!(
        bar(&capped, 17, tabs(1, 0), letters(10, 5)).length_limit(),
        Some(9)
    );
    let unfitted = plain_with(&[("auto_width", "false")]);
    let state = bar(&unfitted, 5, tabs(1, 0), letters(10, 5));
    assert_eq!(state.length_limit(), None);
    assert_eq!(shown(&state), "a|b|c|d|e|[f]|g|h|i|j");
}

#[test]
fn a_lone_entry_too_wide_for_the_bar_is_cut_with_overflow_str() {
    let state = bar(
        &plain_with(&[("overflow_str", "~")]),
        8,
        tabs(1, 0),
        vec![(0, vec![focused(floating(1, "a long title"))])],
    );
    assert_eq!(shown(&state), "[a long~");
}

#[test]
fn a_focused_entry_too_wide_with_its_markers_is_cut_alone_with_no_markers() {
    let mut by_tab = letters(3, 1);
    by_tab[0].1[1].title = "a long title".to_string();
    let state = bar(&plain_with(&[("overflow_str", "~")]), 8, tabs(1, 0), by_tab);
    assert_eq!(shown(&state), "[a long~");
}

#[test]
fn a_cut_line_keeps_its_styles_closed() {
    let state = bar(
        &[],
        6,
        tabs(1, 0),
        vec![(0, vec![focused(floating(1, "a long title"))])],
    );
    let line = state.draw_bar();
    assert_eq!(strip_ansi(&line), " a ...");
    assert!(line.ends_with(zjkit::ansi::ANSI_RESET));
}

#[test]
fn every_theme_combination_draws_a_whole_bar_with_nothing_left_raw() {
    let mut exited = floating(2, "cargo build");
    exited.exited = true;
    let mut state_minimized = BTreeSet::new();
    state_minimized.insert(PaneId::Terminal(3));
    let by_tab = vec![
        (
            0,
            vec![
                floating(1, "htop"),
                exited,
                suppressed(floating(3, "notes")),
            ],
        ),
        (
            1,
            vec![
                focused(floating(4, "editor")),
                plugin(floating(5, "picker")),
            ],
        ),
        (2, vec![floating(6, "logs"), floating(7, "mail")]),
    ];
    for combination in theme::tests::every_combination() {
        // Wide enough for every entry, narrow enough for markers, and too
        // narrow for the focused entry beside its markers.
        for width in [400, 30, 8] {
            let mut state = loaded(&combination);
            state.minimized = state_minimized.clone();
            state.update(Event::ModeUpdate(mode()));
            state.update(Event::TabUpdate(tabs(3, 1)));
            state.update(panes(width, by_tab.clone()));
            let line = state.draw_bar();
            assert!(!line.is_empty(), "{combination:?}");
            for raw in ["#[", "$", "{"] {
                assert!(!line.contains(raw), "{combination:?} {width}: left {raw}");
            }
            let columns = zjkit::ansi::calculate_visible_length(&line, state.wide_ambiguous);
            assert!(columns <= width, "{combination:?} {width}: {columns}");
            if width == 400 {
                let text = strip_ansi(&line);
                for title in ["htop", "notes", "editor", "picker", "mail"] {
                    assert!(text.contains(title), "{combination:?}: {text:?}");
                }
            }
        }
    }
}

// Redrawing only when something shown changed.

#[test]
fn the_same_panes_again_do_not_redraw_but_a_change_to_what_is_shown_does() {
    let by_tab = || vec![(0, vec![focused(floating(1, "a")), floating(2, "b")])];
    let mut state = bar(&PLAIN, 80, tabs(1, 0), by_tab());
    assert!(!state.update(panes(80, by_tab())));
    // Nothing the bar shows: a pane moving, and a tiled pane's title.
    let mut moved = by_tab();
    moved[0].1[1].pane_x = 40;
    moved[0].1.push(PaneInfo {
        id: 9,
        title: "vim".to_string(),
        is_selectable: true,
        ..Default::default()
    });
    assert!(!state.update(panes(80, moved.clone())));
    moved[0].1[2].title = "vim notes".to_string();
    assert!(!state.update(panes(80, moved)));
    // A new title, a new pane, a pane closed, and the focus moving.
    let mut renamed = by_tab();
    renamed[0].1[1].title = "c".to_string();
    assert!(state.update(panes(80, renamed.clone())));
    renamed[0].1.push(floating(3, "d"));
    assert!(state.update(panes(80, renamed.clone())));
    renamed[0].1.pop();
    assert!(state.update(panes(80, renamed.clone())));
    renamed[0].1[0].is_focused = false;
    renamed[0].1[1].is_focused = true;
    assert!(state.update(panes(80, renamed)));
}

#[test]
fn a_tab_update_redraws_only_when_the_entries_change() {
    let by_tab = vec![
        (0, vec![focused(floating(1, "a"))]),
        (1, vec![floating(2, "b")]),
    ];
    let mut state = bar(&PLAIN, 80, tabs(2, 0), by_tab);
    assert!(!state.update(Event::TabUpdate(tabs(2, 0))));
    let mut unseen = tabs(2, 0);
    unseen[0].viewport_rows = 40;
    unseen[0].is_sync_panes_active = true;
    assert!(!state.update(Event::TabUpdate(unseen)));
    // Renaming the tab of a flagged pane, putting the floating panes away,
    // and going to another tab.
    let mut renamed = tabs(2, 0);
    renamed[1].name = "other".to_string();
    assert!(state.update(Event::TabUpdate(renamed.clone())));
    assert_eq!(shown(&state), "[a]/b(other)");
    renamed[0].are_floating_panes_visible = false;
    assert!(state.update(Event::TabUpdate(renamed)));
    assert!(state.update(Event::TabUpdate(tabs(2, 1))));
    assert_eq!(shown(&state), "a(one)/b");
}

#[test]
fn renaming_a_tab_with_no_listed_pane_does_not_redraw() {
    let mut state = bar(&PLAIN, 80, tabs(2, 0), vec![(0, vec![floating(1, "a")])]);
    let mut renamed = tabs(2, 0);
    renamed[1].name = "other".to_string();
    assert!(!state.update(Event::TabUpdate(renamed)));
}

#[test]
fn a_mode_change_redraws_only_for_nesting() {
    let mut state = bar(&PLAIN, 80, tabs(1, 0), vec![(0, vec![floating(1, "a")])]);
    for input_mode in [InputMode::Pane, InputMode::Tab, InputMode::Normal] {
        let mut mode_info = mode();
        mode_info.mode = input_mode;
        assert!(
            !state.update(Event::ModeUpdate(mode_info)),
            "{input_mode:?}"
        );
    }
    let mut nested = mode();
    nested.session_ancestry = vec!["host".to_string()];
    assert!(state.update(Event::ModeUpdate(nested)));
    // A new Zellij theme redraws, as `BarCore` lays the themes again.
    let mut themed = mode();
    themed.style.colors.ribbon_selected.base = PaletteColor::Rgb((9, 9, 9));
    assert!(state.update(Event::ModeUpdate(themed)));
}

#[test]
fn a_pane_update_redraws_when_the_bar_s_width_changes() {
    let by_tab = || vec![(0, vec![floating(1, "a")])];
    let mut state = bar(&PLAIN, 80, tabs(1, 0), by_tab());
    assert!(!state.update(panes(80, by_tab())));
    assert!(state.update(panes(60, by_tab())));
}

#[test]
fn hiding_and_showing_redraw() {
    let mut state = bar(&PLAIN, 80, tabs(1, 0), vec![(0, vec![floating(1, "a")])]);
    let message = |name: &str| PipeMessage::new(PipeSource::Keybind, name, &None, &None, false);
    assert!(state.pipe(message(VISIBILITY.hide)));
    assert_eq!(shown(&state), "");
    assert!(!state.pipe(message(VISIBILITY.hide)));
    assert!(state.pipe(message(VISIBILITY.toggle)));
    assert_eq!(shown(&state), "a");
    assert_eq!(VISIBILITY.hide, "zjtaskbar::hide");
    assert_eq!(VISIBILITY.show, "zjtaskbar::show");
    assert_eq!(VISIBILITY.toggle, "zjtaskbar::toggle");
}

// The settings screen.

use zjkit::settings::live;

#[test]
fn a_grant_adds_the_bar_s_entry_to_zellij_s_settings_screen() {
    let mut granted = State::default();
    assert!(granted.update(Event::PermissionRequestResult(PermissionStatus::Granted)));
    assert_eq!(
        granted.host.calls(),
        vec![
            Call::SetSelectable(false),
            Call::AddSettingsEntry("zjtaskbar".to_string()),
        ]
    );
}

#[test]
fn zellij_s_settings_entry_opens_the_settings_screen_from_the_bar_of_the_client_that_chose_it() {
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
    let mut state = State {
        bar: zjkit::bar::BarCore {
            client_id: Some(2),
            own_url: Some("zjtaskbar".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(!state.pipe(request("3")));
    assert_eq!(state.host.calls(), Vec::new());
    assert!(!state.pipe(request("2")));
    assert_eq!(
        state.host.calls(),
        vec![Call::OpenSettings {
            message: OPEN_SETTINGS_MESSAGE.to_string(),
            pane_title: SETTINGS_PANE_TITLE.to_string(),
            own_url: Some("zjtaskbar".to_string()),
        }]
    );
    assert_eq!(SETTINGS_PANE_TITLE, "zjtaskbar settings");
    assert_eq!(OPEN_SETTINGS_MESSAGE, "zjtaskbar::open_settings");
}

/// A preview from the settings screen of `alias`, as `pipe` gets it, with
/// each change as `(key, value before, value now)`. The screen is plugin 5,
/// whose pane runs from the bar's own URL, as a bar only takes a preview
/// from its own plugin.
fn preview(state: &mut State, alias: &str, changes: &[(&str, Option<&str>, Option<&str>)]) -> bool {
    if let Some(url) = state.bar.own_url.clone() {
        state.bar.plugin_pane_urls.insert(5, url);
    }
    let changes: Vec<live::Change> = changes
        .iter()
        .map(|(key, was, now)| live::Change {
            key: key.to_string(),
            was: was.map(str::to_string),
            now: now.map(str::to_string),
        })
        .collect();
    state.pipe(PipeMessage::new(
        PipeSource::Plugin(5),
        settings::SCHEMA.live_message,
        &None,
        &Some(live::encode(alias, &changes)),
        false,
    ))
}

#[test]
fn a_preview_through_the_bar_s_pipe_changes_what_it_draws() {
    let by_tab = vec![(0, vec![floating(1, "a"), focused(floating(2, "b"))])];
    let mut state = bar(&PLAIN, 80, tabs(1, 0), by_tab);
    state.bar.own_url = Some("zjtaskbar".to_string());
    assert_eq!(shown(&state), "a|[b]");

    let changes = [
        ("entry_spacer", Some("|"), Some("/")),
        ("focused_entry_format", Some("[{title}{flags}]"), None),
    ];
    assert!(preview(&mut state, "zjtaskbar", &changes));
    // The focused entry is back in Zellij's look, which draws no brackets.
    assert_eq!(shown(&state), "a/ b ");

    let undo = [
        ("entry_spacer", Some("/"), Some("|")),
        ("focused_entry_format", None, Some("[{title}{flags}]")),
    ];
    assert!(preview(&mut state, "zjtaskbar", &undo));
    assert_eq!(shown(&state), "a|[b]");
}

#[test]
fn a_preview_of_another_alias_changes_nothing() {
    let mut state = bar(&PLAIN, 80, tabs(1, 0), vec![(0, vec![floating(1, "a")])]);
    state.bar.own_url = Some("zjtaskbar".to_string());
    let changes = [("entry_spacer", Some("|"), Some("/"))];
    assert!(!preview(&mut state, "another", &changes));
}

#[test]
fn the_preview_draws_the_sample_entries_with_markers_in_every_theme() {
    for combination in theme::tests::every_combination() {
        let line = (settings::SCHEMA.draw_bar)(cfg(&combination), &mode(), 200);
        let columns = zjkit::ansi::calculate_visible_length(&line, false);
        assert!(columns <= 56, "{combination:?}: {columns}");
        let text = strip_ansi(&line);
        assert!(text.contains("editor"), "{combination:?}: {text:?}");
        for raw in ["#[", "$", "{"] {
            assert!(!line.contains(raw), "{combination:?}: left {raw}");
        }
    }
    // Hidden, nested and dimmed options do not reach the preview.
    let mut nested = mode();
    nested.session_ancestry = vec!["host".to_string()];
    let line = (settings::SCHEMA.draw_bar)(cfg(&[("hidden", "true")]), &nested, 200);
    assert!(strip_ansi(&line).contains("editor"));
}

#[test]
fn the_list_samples_fill_every_placeholder_of_the_entry_formats() {
    let config = cfg(&[]);
    for key in [
        "entry_format",
        "focused_entry_format",
        "minimized_entry_format",
    ] {
        let names: Vec<&str> = (settings::SCHEMA.sample_values)(key, &config)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert_eq!(names, ["title", "index", "tab", "flags"], "{key}");
    }
    let flags = (settings::SCHEMA.sample_values)("entry_format", &config);
    assert_eq!(flags[3].1, DEFAULT_EXITED_FORMAT);
    assert_eq!(
        (settings::SCHEMA.sample_values)("other_tab_format", &config),
        vec![("tab", "docs".to_string())]
    );
}

// Drawing for the docs images.

#[test]
fn drawing_for_an_image_draws_what_the_plugin_would() {
    let infos = tabs(2, 0);
    let by_tab = vec![
        (0, vec![focused(floating(1, "a"))]),
        (1, vec![floating(2, "b")]),
    ];
    let state = bar(&PLAIN, 40, infos.clone(), by_tab.clone());
    let Event::PaneUpdate(manifest) = panes(40, by_tab) else {
        unreachable!()
    };
    assert_eq!(
        draw_for_image(cfg(&PLAIN), mode(), &infos, manifest, 40),
        state.draw_bar()
    );
}
