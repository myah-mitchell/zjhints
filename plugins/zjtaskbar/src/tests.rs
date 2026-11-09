use super::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use zellij_tile::prelude::actions::Action;
use zellij_utils::input::layout::PercentOrFixed;
use zjkit::ansi::{AnsiParser, AnsiSegment};
use zjkit::bar::Handled;
use zjkit::host::{Call, Recorder};
use zjkit::mouse::labeled_menu_request;

fn cfg(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// A bar loaded with `pairs` for its configuration, standing alone: it
/// shares what an instance's bars share with no other `State`. See
/// `instance`.
fn loaded(pairs: &[(&str, &str)]) -> State {
    let mut state = State {
        instance: Instance::alone(),
        ..Default::default()
    };
    state.load(cfg(pairs));
    state
}

// Permissions.

#[test]
fn loading_the_bar_asks_for_its_permissions_and_events_with_the_mouse_on_by_default() {
    let state = loaded(&[]);
    let calls = state.host.calls();
    // The default tray is empty, so it asks for nothing of its own.
    let permissions = bar_permissions(&[Feature::Mouse]);
    assert!(calls.contains(&Call::RequestPermission(permissions)));
    assert!(calls.contains(&Call::Subscribe(BAR_EVENTS.to_vec())));
    assert!(calls.contains(&Call::Subscribe(MOUSE_EVENTS.to_vec())));
}

#[test]
fn with_the_mouse_off_the_bar_asks_for_no_mouse_events() {
    let state = loaded(&[("mouse", "false"), ("frame_buttons", "false")]);
    let calls = state.host.calls();
    assert!(calls.contains(&Call::RequestPermission(BAR_PERMISSIONS.to_vec())));
    let subscriptions: Vec<_> = calls
        .iter()
        .filter(|call| matches!(call, Call::Subscribe(_)))
        .collect();
    assert_eq!(subscriptions, vec![&Call::Subscribe(BAR_EVENTS.to_vec())]);
}

#[test]
fn with_no_feature_on_the_bar_asks_only_for_what_it_always_needs() {
    assert_eq!(bar_permissions(&[]), BAR_PERMISSIONS.to_vec());
    assert_eq!(
        BarPlugin::permissions(
            &loaded(&[("mouse", "false"), ("frame_buttons", "false")]),
            &BAR
        ),
        BAR_PERMISSIONS.to_vec()
    );
}

#[test]
fn the_mouse_asks_to_run_actions_and_show_prompts() {
    let state = loaded(&[("show_launcher", "false"), ("frame_buttons", "false")]);
    assert_eq!(state.features(), vec![Feature::Mouse]);
    let mut expected = BAR_PERMISSIONS.to_vec();
    expected.extend([
        PermissionType::RunActionsAsUser,
        PermissionType::OpenTerminalsOrPlugins,
    ]);
    assert_eq!(BarPlugin::permissions(&state, &BAR), expected);
    // Asking first or not, and the way round the wheel goes, need nothing
    // more.
    for options in [
        [
            ("mouse_close_confirm", "false"),
            ("mouse_wheel_reversed", "true"),
            ("show_launcher", "false"),
            ("frame_buttons", "false"),
        ],
        [
            ("mouse_close_confirm", "true"),
            ("mouse_wheel_reversed", "false"),
            ("show_launcher", "false"),
            ("frame_buttons", "false"),
        ],
    ] {
        assert_eq!(BarPlugin::permissions(&loaded(&options), &BAR), expected);
    }
}

#[test]
fn turning_the_mouse_on_later_asks_for_it_and_its_events() {
    let mut state = loaded(&[("mouse", "false")]);
    state.host.clear();
    assert!(BarCore::follow(
        &mut state,
        &BAR,
        Handled::Reconfigure(cfg(&[("mouse", "true")]))
    ));
    assert_eq!(
        state.host.calls(),
        vec![
            Call::SetSelectable(true),
            Call::RequestPermission(bar_permissions(&[Feature::Mouse])),
            Call::Subscribe(MOUSE_EVENTS.to_vec()),
        ]
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
    assert_eq!(state.hide_when_nested, DEFAULT_HIDE_WHEN_NESTED);
    assert_eq!(state.collapse_when_empty, DEFAULT_COLLAPSE_WHEN_EMPTY);
    assert_eq!(state.dim_when_unfocused, DEFAULT_DIM_WHEN_UNFOCUSED);
    assert_eq!(state.dim_strength, DEFAULT_DIM_STRENGTH);
    assert_eq!(state.mouse, DEFAULT_MOUSE);
    assert_eq!(state.mouse_wheel_reversed, DEFAULT_MOUSE_WHEEL_REVERSED);
    assert_eq!(state.mouse_close_confirm, DEFAULT_MOUSE_CLOSE_CONFIRM);
    assert_eq!(state.show_launcher, DEFAULT_SHOW_LAUNCHER);
    assert_eq!(state.launcher_format, None);
    assert_eq!(state.launcher_symbol, DEFAULT_LAUNCHER_SYMBOL);
    assert_eq!(state.launcher_new_terminal, DEFAULT_LAUNCHER_NEW_TERMINAL);
    assert_eq!(
        state.launcher_zellij_actions,
        DEFAULT_LAUNCHER_ZELLIJ_ACTIONS
    );
    assert_eq!(state.launch_entries, vec![]);
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
        ("hide_when_nested", "false"),
        ("collapse_when_empty", "false"),
        ("dim_when_unfocused", "false"),
        ("dim_strength", "0.25"),
        ("mouse", "false"),
        ("mouse_wheel_reversed", "true"),
        ("mouse_close_confirm", "false"),
        ("show_launcher", "false"),
        ("launcher_format", "<{symbol}>"),
        ("launcher_symbol", "Go"),
        ("launcher_new_terminal", "false"),
        ("launcher_zellij_actions", "false"),
        ("launch_1_name", "htop"),
        ("launch_1_command", "htop"),
        ("tray", "session mode"),
        ("tray_spacer", "/"),
        ("clock_format", "%H"),
        ("clock_item_format", "<{clock}>"),
        ("clock_utc_offset", "+01:00"),
        ("show_session", "false"),
        ("session_format", "[{session}]"),
        ("mode_format", "({desc})"),
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
    assert!(!state.hide_when_nested);
    assert!(!state.collapse_when_empty);
    assert!(!state.dim_when_unfocused);
    assert_eq!(state.dim_strength, 0.25);
    assert!(!state.mouse);
    assert!(state.mouse_wheel_reversed);
    assert!(!state.mouse_close_confirm);
    assert!(!state.show_launcher);
    assert_eq!(state.launcher_format.as_deref(), Some("<{symbol}>"));
    assert_eq!(state.launcher_symbol, "Go");
    assert!(!state.launcher_new_terminal);
    assert!(!state.launcher_zellij_actions);
    assert_eq!(state.launch_entries.len(), 1);
    assert_eq!(state.tray, [TrayItem::Session, TrayItem::Mode]);
    assert_eq!(state.tray_spacer, "/");
    assert_eq!(state.clock.format, "%H");
    assert_eq!(state.clock.item_format.as_deref(), Some("<{clock}>"));
    assert_eq!(
        state.clock.offset_source,
        clock::OffsetSource::of(Some("+01:00"))
    );
    assert_eq!(state.show_session, Some(false));
    assert_eq!(state.session_format.as_deref(), Some("[{session}]"));
    assert_eq!(state.mode_format.as_deref(), Some("({desc})"));
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

/// Formats with no styling, so a test can read the line as plain text, and
/// no launcher's button, so the entries start at column 0. The launcher's
/// own tests turn it back on.
const PLAIN: [(&str, &str); 12] = [
    ("show_launcher", "false"),
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

/// The tab at `position`, named `name`. Its id is `100 + position`, so a
/// test can tell the two apart.
fn tab(position: usize, name: &str) -> TabInfo {
    TabInfo {
        tab_id: 100 + position,
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
    strip_ansi(&state.draw_bar().0)
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
    let by_tab = vec![
        (0, vec![floating(1, "a")]),
        (1, vec![floating(2, "b")]),
        (2, vec![floating(3, "c")]),
    ];
    let state = bar(
        &[("other_tab_format", " ({tab})")],
        200,
        infos.clone(),
        by_tab.clone(),
    );
    let line = shown(&state);
    assert!(line.contains(" a (one) "), "{line:?}");
    assert!(line.contains(" b "), "{line:?}");
    assert!(!line.contains("(two)"), "{line:?}");
    assert!(line.contains(" c (3) "), "{line:?}");
    // Unset, the flag shows nothing: the separator sets the tabs apart.
    let state = bar(&[], 200, infos, by_tab);
    let line = shown(&state);
    assert!(line.contains(" a  \u{2502}  b "), "{line:?}");
    assert!(!line.contains("(one)") && !line.contains("(3)"), "{line:?}");
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
    state
        .minimized_mut()
        .insert(PaneId::Terminal(2), FloatingPaneCoordinates::default());
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
    state
        .minimized_mut()
        .insert(PaneId::Terminal(2), FloatingPaneCoordinates::default());
    state.update(Event::TabUpdate(tabs(1, 0)));
    state.update(panes(80, vec![(0, vec![floating(2, "b")])]));
    assert_eq!(shown(&state), "b");
    // It is still kept, as the update may be one sent before the hide.
    assert!(state.minimized().contains_key(&PaneId::Terminal(2)));
}

#[test]
fn a_minimized_pane_is_forgotten_once_closed_or_tiled() {
    let mut state = loaded(&PLAIN);
    state
        .minimized_mut()
        .insert(PaneId::Terminal(1), FloatingPaneCoordinates::default());
    state
        .minimized_mut()
        .insert(PaneId::Terminal(2), FloatingPaneCoordinates::default());
    let mut tiled = floating(2, "b");
    tiled.is_floating = false;
    state.update(panes(80, vec![(0, vec![tiled])]));
    assert!(state.minimized().is_empty());
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
    let mut state = loaded(&[("show_launcher", "false")]);
    state
        .minimized_mut()
        .insert(PaneId::Terminal(3), FloatingPaneCoordinates::default());
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
        " build (EXITED)   notes (MINIMIZED)  \u{2502}  picker (PLUGIN) "
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
fn with_no_formats_the_bar_takes_zellij_s_colors() {
    let mut state = loaded(&[]);
    state
        .minimized_mut()
        .insert(PaneId::Terminal(3), FloatingPaneCoordinates::default());
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
    // The launcher's button on an unselected ribbon, in bold, and then the
    // entries.
    let expected = [
        drawn("#[fg=#040506,bg=#313244,bold] Menu "),
        " ".to_string(),
        drawn("#[fg=#010203,bg=#a6e3a1,bold] a "),
        " ".to_string(),
        drawn("#[fg=#040506,bg=#313244] b "),
        " ".to_string(),
        drawn("#[fg=#070809,bg=#1e1e2e] c (MINIMIZED) "),
    ]
    .concat();
    assert_eq!(state.draw_bar().0, expected);
}

#[test]
fn before_zellij_sends_its_theme_the_entries_are_unstyled() {
    let mut state = loaded(&[]);
    state.update(Event::TabUpdate(tabs(1, 0)));
    state.update(panes(80, vec![(0, vec![floating(1, "a")])]));
    assert_eq!(state.draw_bar().0, " Menu   a ");
}

#[test]
fn a_styled_flag_keeps_the_entry_s_background_under_it() {
    let mut exited = floating(1, "a");
    exited.exited = true;
    let state = bar(
        &[
            ("format_theme", "flat"),
            ("exited_format", "#[fg=$red] X"),
            ("show_launcher", "false"),
        ],
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
    assert_eq!(
        state.draw_bar().0,
        ansi_term::ANSIStrings(&parts).to_string()
    );
}

// Hiding, dimming and the empty bar.

#[test]
fn the_bar_dims_while_its_session_is_not_receiving_input() {
    let by_tab = || vec![(0, vec![floating(1, "a"), floating(2, "b")])];
    let undimmed = bar(&[], 80, tabs(1, 0), by_tab()).draw_bar().0;
    let dimmed_mode = || {
        let mut mode_info = mode();
        mode_info.session_dimmed = Some(true);
        Event::ModeUpdate(mode_info)
    };
    let mut state = bar(&[], 80, tabs(1, 0), by_tab());
    assert!(state.update(dimmed_mode()));
    let dimmed = state.draw_bar().0;
    assert_ne!(dimmed, undimmed);
    assert_eq!(strip_ansi(&dimmed), strip_ansi(&undimmed));

    let mut state = bar(&[("dim_when_unfocused", "false")], 80, tabs(1, 0), by_tab());
    state.update(dimmed_mode());
    assert_eq!(state.draw_bar().0, undimmed);
}

#[test]
fn hidden_draws_nothing() {
    let state = bar(
        &plain_with(&[("hidden", "true")]),
        80,
        tabs(1, 0),
        vec![(0, vec![floating(1, "a")])],
    );
    assert_eq!(state.draw_bar().0, "");
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
    assert_eq!(state.draw_bar().0, "");
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
    let state = bar(
        &[("entry_spacer", "|"), ("show_launcher", "false")],
        20,
        tabs(1, 0),
        letters(10, 5),
    );
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
        &[("show_launcher", "false")],
        6,
        tabs(1, 0),
        vec![(0, vec![focused(floating(1, "a long title"))])],
    );
    let line = state.draw_bar().0;
    assert_eq!(strip_ansi(&line), " a ...");
    assert!(line.ends_with(zjkit::ansi::ANSI_RESET));
}

#[test]
fn every_theme_combination_draws_a_whole_bar_with_nothing_left_raw() {
    let mut exited = floating(2, "cargo build");
    exited.exited = true;
    let mut state_minimized = Minimized::default();
    state_minimized.insert(PaneId::Terminal(3), FloatingPaneCoordinates::default());
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
    let mut mode_info = mode();
    mode_info.session_name = Some("work".to_string());
    for combination in theme::tests::every_combination() {
        // Every tray item, with an offset set so the clock draws at once.
        let mut options = combination.clone();
        options.extend([
            ("tray", "mode session clock"),
            ("clock_utc_offset", "+00:00"),
        ]);
        // Wide enough for every entry, narrow enough for markers, and too
        // narrow for the focused entry beside its markers.
        for width in [400, 30, 8] {
            let mut state = loaded(&options);
            *state.minimized_mut() = state_minimized.clone();
            state.update(Event::ModeUpdate(mode_info.clone()));
            state.update(Event::TabUpdate(tabs(3, 1)));
            state.update(panes(width, by_tab.clone()));
            let line = state.draw_bar().0;
            assert!(!line.is_empty(), "{combination:?}");
            for raw in ["#[", "$", "{"] {
                assert!(!line.contains(raw), "{combination:?} {width}: left {raw}");
            }
            let columns = zjkit::ansi::calculate_visible_length(&line);
            assert!(columns <= width, "{combination:?} {width}: {columns}");
            // The launcher's button leads at every width, the entries giving
            // way to it.
            assert_eq!(
                state.draw_bar().1.value_at(0),
                Some(&Clickable::Launcher),
                "{combination:?} {width}"
            );
            if width == 400 {
                let text = strip_ansi(&line);
                assert!(
                    text.contains(&state.launcher_symbol),
                    "{combination:?}: {text:?}"
                );
                for title in ["htop", "notes", "editor", "picker", "mail"] {
                    assert!(text.contains(title), "{combination:?}: {text:?}");
                }
                // The whole tray, flush with the line's right end, with the
                // mode's name, the session's and the time.
                assert_eq!(columns, width, "{combination:?}");
                assert!(
                    text.trim_end().ends_with("00:00"),
                    "{combination:?}: {text:?}"
                );
                for shown in ["NORMAL", "work"] {
                    assert!(text.contains(shown), "{combination:?}: {text:?}");
                }
                let spans = state.draw_bar().1;
                for item in TrayItem::ALL {
                    assert!(
                        (0..width)
                            .any(|column| spans.value_at(column) == Some(&Clickable::Tray(item))),
                        "{combination:?}: {item:?}"
                    );
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
fn a_mode_change_redraws_only_for_nesting_without_the_mode_or_session_in_the_tray() {
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
            instance: None,
        }]
    );
    assert_eq!(SETTINGS_PANE_TITLE, "zjtaskbar settings");
    assert_eq!(OPEN_SETTINGS_MESSAGE, "zjtaskbar::open_settings");
}

/// A preview from the settings screen of `alias`, as `pipe` gets it, with
/// each change as `(key, value before, value now)`. The screen runs in the
/// bar's own shared instance, as a bar only takes a preview from there.
fn preview(state: &mut State, alias: &str, changes: &[(&str, Option<&str>, Option<&str>)]) -> bool {
    let changes: Vec<live::Change> = changes
        .iter()
        .map(|(key, was, now)| live::Change {
            key: key.to_string(),
            was: was.map(str::to_string),
            now: now.map(str::to_string),
        })
        .collect();
    state.pipe(PipeMessage::new(
        PipeSource::Plugin(Recorder::INSTANCE_ID),
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
fn a_launcher_entry_previewed_from_the_screen_reaches_the_bar_s_menu() {
    let mut state = bar(&[], 80, tabs(1, 0), vec![]);
    state.bar.own_url = Some("zjtaskbar".to_string());
    state.host.clear();

    // The screen sends one key at a time as the entry is added.
    assert!(preview(
        &mut state,
        "zjtaskbar",
        &[("launch_1_name", None, Some("Logs"))]
    ));
    assert_eq!(state.launch_entries, Vec::new());
    let command = [("launch_1_command", None, Some("tail"))];
    assert!(preview(&mut state, "zjtaskbar", &command));
    let names: Vec<&str> = state.launch_entries.iter().map(|e| &*e.name).collect();
    assert_eq!(names, ["Logs"]);
    // Running the entry's command takes a permission the bar asks for then.
    let with_command = bar_permissions(&[
        Feature::Mouse,
        Feature::Launcher,
        Feature::LauncherZellijAction,
        Feature::LauncherCommand,
    ]);
    assert!(
        state
            .host
            .calls()
            .contains(&Call::RequestPermission(with_command)),
        "{:?}",
        state.host.calls()
    );

    let undo = [
        ("launch_1_name", Some("Logs"), None),
        ("launch_1_command", Some("tail"), None),
    ];
    assert!(preview(&mut state, "zjtaskbar", &undo));
    assert_eq!(state.launch_entries, Vec::new());
}

#[test]
fn a_launcher_entry_sent_as_a_preview_by_another_plugin_is_refused() {
    let mut state = bar(&[], 80, tabs(1, 0), vec![]);
    state.bar.own_url = Some("zjtaskbar".to_string());
    state.host.clear();
    let changes: Vec<live::Change> = [
        ("launch_0_name", "New terminal"),
        ("launch_0_command", "sh"),
    ]
    .iter()
    .map(|(key, now)| live::Change {
        key: key.to_string(),
        was: None,
        now: Some(now.to_string()),
    })
    .collect();
    let message = |source| {
        PipeMessage::new(
            source,
            settings::SCHEMA.live_message,
            &None,
            &Some(live::encode("zjtaskbar", &changes)),
            false,
        )
    };
    assert!(!state.pipe(message(PipeSource::Plugin(6))));
    assert!(!state.pipe(message(PipeSource::Cli("pipe-1".to_string()))));
    assert_eq!(state.launch_entries, Vec::new());
    assert_eq!(state.host.calls(), Vec::new());
}

#[test]
fn a_preview_of_another_alias_changes_nothing() {
    let mut state = bar(&PLAIN, 80, tabs(1, 0), vec![(0, vec![floating(1, "a")])]);
    state.bar.own_url = Some("zjtaskbar".to_string());
    let changes = [("entry_spacer", Some("|"), Some("/"))];
    assert!(!preview(&mut state, "another", &changes));
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
        state.draw_bar().0
    );
}

#[test]
fn drawing_for_an_image_takes_a_hidden_pane_as_one_the_bar_minimized() {
    let by_tab = vec![(
        0,
        vec![focused(floating(1, "a")), suppressed(floating(2, "b"))],
    )];
    let Event::PaneUpdate(manifest) = panes(40, by_tab) else {
        unreachable!()
    };
    let line = draw_for_image(cfg(&PLAIN), mode(), &tabs(1, 0), manifest, 40);
    assert_eq!(strip_ansi(&line), "[a]|_b_");
}

// The mouse.

/// Render the bar as Zellij does, at the width of its own pane, or 80
/// columns while it has none.
fn render_in_pane(state: &mut State) {
    let cols = state.bar.pane_width.unwrap_or(80);
    state.render(1, cols);
}

/// What `mouse` on the bar as last drawn makes it ask of Zellij, through
/// `update` as Zellij sends it, at the real time. Two left clicks on one
/// column through this are a double-click's, so a test making both uses
/// `after_mouse_at`.
fn after_mouse(state: &mut State, mouse: Mouse) -> Vec<Call> {
    render_in_pane(state);
    state.host.clear();
    assert!(
        !state.update(Event::Mouse(mouse)),
        "the mouse redrew the bar"
    );
    state.host.calls()
}

/// What `mouse` at `now` makes the bar ask of Zellij, for the clicks of a
/// double-click, which the bar tells apart by their times.
fn after_mouse_at(state: &mut State, mouse: Mouse, now: Instant) -> Vec<Call> {
    render_in_pane(state);
    state.host.clear();
    state.follow_mouse(&mouse, now);
    state.host.calls()
}

/// The one prompt `calls` shows, with its id.
fn the_prompt(calls: &[Call]) -> (u64, PromptRequest) {
    match calls {
        [Call::Prompt {
            request_id,
            request,
        }] => (*request_id, request.clone()),
        other => panic!("expected one prompt, got {other:?}"),
    }
}

/// What the answer `result` to the prompt `request_id` makes the bar ask of
/// Zellij.
fn after_answer(state: &mut State, request_id: u64, result: PromptResult) -> Vec<Call> {
    state.host.clear();
    assert!(!state.update(Event::PromptResult(request_id, result)));
    state.host.calls()
}

fn choose(index: usize) -> PromptResult {
    PromptResult::Answered(PromptValue::Choice(index.to_string()))
}

fn focus_terminal(id: u32) -> Call {
    Call::FocusTerminalPane {
        terminal_pane_id: id,
        should_float_if_hidden: true,
        should_be_in_place_if_hidden: false,
    }
}

fn rename(pane: PaneId) -> Call {
    Call::RunAction {
        action: Action::StartRenamePaneByPaneId { pane_id: pane },
        context: BTreeMap::new(),
    }
}

/// `pane` standing at column `x` and row `y` of the screen, `columns` wide
/// and `rows` high with its frame, and a frame around its content unless
/// `borderless`.
fn placed(
    mut pane: PaneInfo,
    (x, y): (usize, usize),
    (columns, rows): (usize, usize),
    borderless: bool,
) -> PaneInfo {
    let frame = if borderless { 0 } else { 1 };
    pane.pane_x = x;
    pane.pane_y = y;
    pane.pane_columns = columns;
    pane.pane_rows = rows;
    pane.pane_content_x = x + frame;
    pane.pane_content_y = y + frame;
    pane.pane_content_columns = columns - 2 * frame;
    pane.pane_content_rows = rows - 2 * frame;
    pane
}

fn fixed(
    (x, y): (usize, usize),
    (columns, rows): (usize, usize),
    borderless: Option<bool>,
) -> FloatingPaneCoordinates {
    FloatingPaneCoordinates {
        x: Some(PercentOrFixed::Fixed(x)),
        y: Some(PercentOrFixed::Fixed(y)),
        width: Some(PercentOrFixed::Fixed(columns)),
        height: Some(PercentOrFixed::Fixed(rows)),
        borderless,
        ..Default::default()
    }
}

/// Three panes of one tab, `htop` focused: `vim|[htop]|logs`, each title
/// starting at the column after the spacer before it.
fn three_panes() -> State {
    bar(
        &PLAIN,
        80,
        tabs(1, 0),
        vec![(
            0,
            vec![
                floating(1, "vim"),
                focused(placed(floating(2, "htop"), (10, 5), (40, 12), false)),
                floating(3, "logs"),
            ],
        )],
    )
}

#[test]
fn the_click_spans_cover_the_entries_and_markers_and_nothing_between() {
    let mut state = bar(&PLAIN, 13, tabs(1, 0), letters(10, 5));
    state.render(1, 13);
    assert_eq!(shown(&state), "<4|e|[f]|g|3>");
    let at = |column| state.click_spans.value_at(column).copied();
    let terminal = PaneId::Terminal;
    assert_eq!(at(0), Some(Clickable::Marker(terminal(4))));
    assert_eq!(at(1), Some(Clickable::Marker(terminal(4))));
    assert_eq!(at(2), None);
    assert_eq!(at(3), Some(Clickable::Entry(terminal(5))));
    for column in 5..8 {
        assert_eq!(at(column), Some(Clickable::Entry(terminal(6))));
    }
    assert_eq!(at(10), None);
    assert_eq!(at(11), Some(Clickable::Marker(terminal(8))));
    assert_eq!(at(12), Some(Clickable::Marker(terminal(8))));
    assert_eq!(at(13), None);
}

#[test]
fn the_click_spans_measure_a_title_as_zellij_lays_it_out() {
    let spans_of = |pairs: &[(&str, &str)], title: &str| {
        let by_tab = vec![(0, vec![floating(1, title), floating(2, "x")])];
        let mut state = bar(pairs, 80, tabs(1, 0), by_tab);
        state.render(1, 80);
        assert_eq!(shown(&state), format!("{title}|x"));
        let at = |column| state.click_spans.value_at(column).copied();
        (0..5).map(at).collect::<Vec<_>>()
    };
    let (one, two) = (
        Some(Clickable::Entry(PaneId::Terminal(1))),
        Some(Clickable::Entry(PaneId::Terminal(2))),
    );
    // An East Asian Ambiguous character takes the one column Zellij lays it
    // out at, and the deprecated ambiguous_width changes nothing.
    for pairs in [plain_with(&[]), plain_with(&[("ambiguous_width", "2")])] {
        assert_eq!(spans_of(&pairs, "\u{25cf}"), [one, None, two, None, None]);
    }
    // An emoji joined from several characters takes two columns together.
    let joined = spans_of(&PLAIN, "\u{1f469}\u{200d}\u{1f4bb}");
    assert_eq!(joined, [one, one, None, two, None]);
}

#[test]
fn the_separator_between_tabs_and_the_cut_of_a_line_stand_for_nothing() {
    let mut state = bar(
        &PLAIN,
        80,
        tabs(2, 0),
        vec![(0, vec![floating(1, "a")]), (1, vec![floating(2, "b")])],
    );
    state.render(1, 80);
    assert_eq!(shown(&state), "a/b(two)");
    assert_eq!(state.click_spans.value_at(1), None);
    assert_eq!(
        state.click_spans.value_at(2),
        Some(&Clickable::Entry(PaneId::Terminal(2)))
    );

    let mut state = bar(
        &plain_with(&[("overflow_str", "~")]),
        4,
        tabs(1, 0),
        vec![(0, vec![focused(floating(1, "abcdefgh"))])],
    );
    state.render(1, 4);
    assert_eq!(shown(&state), "[ab~");
    assert_eq!(
        state.click_spans.value_at(2),
        Some(&Clickable::Entry(PaneId::Terminal(1)))
    );
    assert_eq!(state.click_spans.value_at(3), None);
}

#[test]
fn a_left_click_focuses_a_pane_that_is_not_focused() {
    let mut state = three_panes();
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 0)),
        vec![focus_terminal(1)]
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 12)),
        vec![focus_terminal(3)]
    );
}

#[test]
fn a_left_click_focuses_a_plugin_pane_and_pulls_one_from_another_tab() {
    let mut state = bar(
        &PLAIN,
        80,
        tabs(2, 0),
        vec![
            (0, vec![plugin(floating(7, "picker"))]),
            (1, vec![floating(2, "far")]),
        ],
    );
    assert_eq!(shown(&state), "picker*/far(two)");
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 0)),
        vec![Call::FocusPluginPane {
            plugin_pane_id: 7,
            should_float_if_hidden: true,
            should_be_in_place_if_hidden: false,
        }]
    );
    // Zellij moves a pane between tabs only as it shows it from hidden, and
    // places it as new, so it is hidden, shown in the active tab and put
    // back where it stood.
    let far = PaneId::Terminal(2);
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 8)),
        vec![
            Call::HidePaneWithId(far),
            Call::ShowPaneWithId {
                pane_id: far,
                should_float_if_hidden: true,
                tab_id: Some(100),
            },
            Call::ChangeFloatingPanesCoordinates(vec![(
                far,
                mouse::geometry_of(&floating(2, "far"))
            )]),
        ]
    );
}

#[test]
fn a_left_click_on_the_focused_entry_minimizes_its_pane_recording_where_it_stood() {
    let mut state = three_panes();
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 5)),
        vec![Call::HidePaneWithId(PaneId::Terminal(2))]
    );
    assert_eq!(
        state.minimized().get(&PaneId::Terminal(2)),
        Some(&fixed((10, 5), (40, 12), Some(false)))
    );
}

#[test]
fn a_left_click_on_a_minimized_entry_restores_its_pane_where_it_stood() {
    let mut state = three_panes();
    let start = Instant::now();
    after_mouse_at(&mut state, Mouse::LeftClick(0, 5), start);
    // Zellij hides the pane, and reports it with its place forgotten.
    let hidden = suppressed(floating(2, "htop"));
    state.update(panes(
        80,
        vec![(0, vec![floating(1, "vim"), hidden, floating(3, "logs")])],
    ));
    assert_eq!(shown(&state), "vim|_htop_|logs");
    // Long enough after the first not to be its double-click.
    let later = start + Duration::from_secs(1);
    assert_eq!(
        after_mouse_at(&mut state, Mouse::LeftClick(0, 5), later),
        vec![
            focus_terminal(2),
            Call::ChangeFloatingPanesCoordinates(vec![(
                PaneId::Terminal(2),
                fixed((10, 5), (40, 12), Some(false)),
            )]),
        ]
    );
    assert!(state.minimized().is_empty());
    // The entry stays as it was until Zellij reports the pane on screen.
    assert_eq!(shown(&state), "vim|_htop_|logs");
}

#[test]
fn the_round_trip_keeps_a_pane_s_place_size_and_frame_exactly() {
    for (pane, expected) in [
        (
            placed(floating(1, "a"), (0, 0), (3, 3), false),
            fixed((0, 0), (3, 3), Some(false)),
        ),
        (
            placed(floating(1, "a"), (117, 40), (2, 1), true),
            fixed((117, 40), (2, 1), Some(true)),
        ),
    ] {
        let mut state = bar(&PLAIN, 80, tabs(1, 0), vec![(0, vec![focused(pane)])]);
        let start = Instant::now();
        after_mouse_at(&mut state, Mouse::LeftClick(0, 0), start);
        state.update(panes(80, vec![(0, vec![suppressed(floating(1, "a"))])]));
        let later = start + Duration::from_secs(1);
        let calls = after_mouse_at(&mut state, Mouse::LeftClick(0, 0), later);
        assert_eq!(
            calls[1],
            Call::ChangeFloatingPanesCoordinates(vec![(PaneId::Terminal(1), expected)])
        );
    }
    // A fullscreen pane fills the screen with or without its frame, so the
    // frame is left as Zellij has it.
    let mut pane = placed(floating(1, "a"), (0, 0), (80, 24), true);
    pane.is_fullscreen = true;
    assert_eq!(mouse::geometry_of(&pane), fixed((0, 0), (80, 24), None));
}

#[test]
fn a_left_click_on_a_marker_brings_up_the_nearest_hidden_entry() {
    let mut state = bar(&PLAIN, 13, tabs(1, 0), letters(10, 5));
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 0)),
        vec![focus_terminal(4)]
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 12)),
        vec![focus_terminal(8)]
    );
    // A minimized one is restored.
    let mut by_tab = letters(10, 5);
    by_tab[0].1[3] = suppressed(floating(4, "d"));
    let mut state = bar(&PLAIN, 13, tabs(1, 0), by_tab.clone());
    let geometry = fixed((1, 2), (30, 10), Some(false));
    state
        .minimized_mut()
        .insert(PaneId::Terminal(4), geometry.clone());
    state.update(panes(13, by_tab));
    assert_eq!(shown(&state), "<4|e|[f]|g|3>");
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 0)),
        vec![
            focus_terminal(4),
            Call::ChangeFloatingPanesCoordinates(vec![(PaneId::Terminal(4), geometry)]),
        ]
    );
}

#[test]
fn only_a_right_click_does_anything_off_an_entry() {
    let mut state = bar(&PLAIN, 13, tabs(1, 0), letters(10, 5));
    // A spacer, and the empty end of the bar.
    for column in [2, 40] {
        for mouse in [
            Mouse::LeftClick(0, column),
            Mouse::DoubleClick(0, column),
            Mouse::MiddleClick(0, column),
        ] {
            assert_eq!(after_mouse(&mut state, mouse), vec![], "{mouse:?}");
        }
    }
    // A marker stands for several entries, so only a left click picks one.
    assert_eq!(after_mouse(&mut state, Mouse::MiddleClick(0, 0)), vec![]);
    assert_eq!(after_mouse(&mut state, Mouse::DoubleClick(0, 0)), vec![]);
    for column in [0, 2, 40] {
        assert_eq!(
            after_mouse(&mut state, Mouse::RightClick(0, column)),
            vec![Call::OpenContextMenu {
                target: ContextMenuTarget::Bar,
                line: 0,
                column,
            }]
        );
    }
}

#[test]
fn the_second_left_click_of_a_double_click_does_nothing_and_the_double_click_renames() {
    let start = Instant::now();
    let after = |millis| start + Duration::from_millis(millis);
    let pane = PaneId::Terminal(1);

    // An entry not focused is focused once, and renamed.
    let mut state = three_panes();
    let left = Mouse::LeftClick(0, 1);
    assert_eq!(
        after_mouse_at(&mut state, left, after(0)),
        vec![focus_terminal(1)]
    );
    assert_eq!(after_mouse_at(&mut state, left, after(150)), vec![]);
    assert_eq!(
        after_mouse_at(&mut state, Mouse::DoubleClick(0, 1), after(150)),
        vec![focus_terminal(1), rename(pane)]
    );

    // The focused entry's first click minimizes it, and the double-click
    // puts it back where it stood before renaming it, whether or not Zellij
    // has reported it hidden yet.
    let pane = PaneId::Terminal(2);
    let restore =
        Call::ChangeFloatingPanesCoordinates(vec![(pane, fixed((10, 5), (40, 12), Some(false)))]);
    for reported_hidden in [false, true] {
        let mut state = three_panes();
        let left = Mouse::LeftClick(0, 5);
        assert_eq!(
            after_mouse_at(&mut state, left, after(0)),
            vec![Call::HidePaneWithId(pane)]
        );
        if reported_hidden {
            let hidden = suppressed(floating(2, "htop"));
            state.update(panes(
                80,
                vec![(0, vec![floating(1, "vim"), hidden, floating(3, "logs")])],
            ));
        }
        assert_eq!(after_mouse_at(&mut state, left, after(399)), vec![]);
        assert_eq!(
            after_mouse_at(&mut state, Mouse::DoubleClick(0, 5), after(399)),
            vec![focus_terminal(2), restore.clone(), rename(pane)],
            "reported hidden: {reported_hidden}"
        );
        assert!(state.minimized().is_empty());
    }
}

#[test]
fn a_left_click_after_the_double_click_time_or_elsewhere_is_one_of_its_own() {
    let start = Instant::now();
    let after = |millis| start + Duration::from_millis(millis);
    let mut state = three_panes();
    after_mouse_at(&mut state, Mouse::LeftClick(0, 0), after(0));
    assert_eq!(
        after_mouse_at(&mut state, Mouse::LeftClick(0, 0), after(400)),
        vec![focus_terminal(1)]
    );
    assert_eq!(
        after_mouse_at(&mut state, Mouse::LeftClick(0, 1), after(450)),
        vec![focus_terminal(1)]
    );
}

#[test]
fn a_double_click_acts_on_what_its_first_click_landed_on() {
    let start = Instant::now();
    let mut state = bar(&PLAIN, 13, tabs(1, 0), letters(10, 5));
    after_mouse_at(&mut state, Mouse::LeftClick(0, 0), start);
    // The click focused `d`, and the window moved: an entry is drawn where
    // the marker was.
    state.update(panes(13, letters(10, 3)));
    assert_eq!(shown(&state), "<2|c|[d]|e|5>");
    state.update(panes(13, letters(10, 1)));
    assert_eq!(shown(&state), "a|[b]|c|d|6>");
    assert_eq!(
        after_mouse_at(&mut state, Mouse::DoubleClick(0, 0), start),
        vec![]
    );
}

#[test]
fn a_double_click_on_a_minimized_entry_restores_and_renames_its_pane() {
    let mut state = three_panes();
    let geometry = fixed((1, 1), (20, 8), Some(false));
    state
        .minimized_mut()
        .insert(PaneId::Terminal(3), geometry.clone());
    state.update(panes(
        80,
        vec![(
            0,
            vec![
                floating(1, "vim"),
                focused(floating(2, "htop")),
                suppressed(floating(3, "logs")),
            ],
        )],
    ));
    assert_eq!(
        after_mouse(&mut state, Mouse::DoubleClick(0, 12)),
        vec![
            focus_terminal(3),
            Call::ChangeFloatingPanesCoordinates(vec![(PaneId::Terminal(3), geometry)]),
            rename(PaneId::Terminal(3)),
        ]
    );
    // The focused one is renamed where it is.
    assert_eq!(
        after_mouse(&mut state, Mouse::DoubleClick(0, 5)),
        vec![rename(PaneId::Terminal(2))]
    );
}

/// The close confirmation, asking `message`.
fn close_prompt(message: &str) -> PromptRequest {
    PromptRequest::confirm(message)
        .title("Close pane?")
        .yes("Close pane")
        .no("Cancel")
        .default("false")
}

#[test]
fn a_middle_click_asks_before_closing_with_cancel_selected() {
    let mut state = three_panes();
    let (request_id, request) = the_prompt(&after_mouse(&mut state, Mouse::MiddleClick(0, 0)));
    assert_eq!(
        request,
        close_prompt("Close pane \"vim\" and end what runs in it?")
    );
    assert_eq!(
        after_answer(&mut state, request_id, PromptResult::Confirmed(true)),
        vec![Call::CloseTerminalPane(1)]
    );
    assert!(!state.pending_prompt.is_waiting());
    // Answered once: the same answer again closes nothing.
    assert_eq!(
        after_answer(&mut state, request_id, PromptResult::Confirmed(true)),
        vec![]
    );
}

#[test]
fn a_confirmed_close_of_a_plugin_pane_closes_it_as_a_plugin() {
    let mut state = bar(
        &PLAIN,
        80,
        tabs(1, 0),
        vec![(0, vec![plugin(floating(7, "picker"))])],
    );
    let (request_id, _) = the_prompt(&after_mouse(&mut state, Mouse::MiddleClick(0, 0)));
    assert_eq!(
        after_answer(&mut state, request_id, PromptResult::Confirmed(true)),
        vec![Call::ClosePluginPane(7)]
    );
}

#[test]
fn the_close_prompt_shows_a_title_s_control_characters_escaped() {
    let mut state = bar(
        &PLAIN,
        80,
        tabs(1, 0),
        vec![(0, vec![floating(1, "a\u{1b}b")])],
    );
    let (_, request) = the_prompt(&after_mouse(&mut state, Mouse::MiddleClick(0, 0)));
    let escaped = zjkit::ansi::show_controls("a\u{1b}b");
    assert!(!escaped.contains('\u{1b}'));
    assert_eq!(
        request,
        close_prompt(&format!(
            "Close pane \"{escaped}\" and end what runs in it?"
        ))
    );
}

#[test]
fn any_answer_but_a_confirmation_closes_nothing() {
    for result in [
        PromptResult::Confirmed(false),
        PromptResult::Cancelled,
        PromptResult::TimedOut(None),
        PromptResult::TimedOut(Some(PromptValue::Bool(true))),
        PromptResult::Error("no popup".to_string()),
        PromptResult::Answered(PromptValue::Bool(true)),
        choose(0),
    ] {
        let mut state = three_panes();
        let (request_id, _) = the_prompt(&after_mouse(&mut state, Mouse::MiddleClick(0, 0)));
        assert_eq!(
            after_answer(&mut state, request_id, result.clone()),
            vec![],
            "{result:?}"
        );
        assert!(!state.pending_prompt.is_waiting(), "{result:?}");
    }
}

#[test]
fn an_answer_to_another_prompt_closes_nothing_and_keeps_waiting() {
    let mut state = three_panes();
    let (request_id, _) = the_prompt(&after_mouse(&mut state, Mouse::MiddleClick(0, 0)));
    for other in [request_id + 1, request_id.wrapping_sub(1), 0] {
        assert_eq!(
            after_answer(&mut state, other, PromptResult::Confirmed(true)),
            vec![]
        );
    }
    assert!(state.pending_prompt.is_waiting());
    assert_eq!(
        after_answer(&mut state, request_id, PromptResult::Confirmed(true)),
        vec![Call::CloseTerminalPane(1)]
    );
}

#[test]
fn an_answer_with_nothing_waiting_does_nothing() {
    let mut state = three_panes();
    assert_eq!(
        after_answer(&mut state, 1, PromptResult::Confirmed(true)),
        vec![]
    );
    assert_eq!(after_answer(&mut state, 1, choose(0)), vec![]);
}

#[test]
fn a_confirmation_for_a_pane_closed_since_closes_nothing() {
    let mut state = three_panes();
    let (request_id, _) = the_prompt(&after_mouse(&mut state, Mouse::MiddleClick(0, 0)));
    state.update(panes(80, vec![(0, vec![focused(floating(2, "htop"))])]));
    assert_eq!(
        after_answer(&mut state, request_id, PromptResult::Confirmed(true)),
        vec![]
    );
}

#[test]
fn a_newer_prompt_replaces_the_one_waited_for() {
    let mut state = three_panes();
    let (first, _) = the_prompt(&after_mouse(&mut state, Mouse::MiddleClick(0, 0)));
    let (second, _) = the_prompt(&after_mouse(&mut state, Mouse::MiddleClick(0, 12)));
    assert_eq!(
        after_answer(&mut state, first, PromptResult::Confirmed(true)),
        vec![]
    );
    assert_eq!(
        after_answer(&mut state, second, PromptResult::Confirmed(true)),
        vec![Call::CloseTerminalPane(3)]
    );
}

#[test]
fn with_mouse_close_confirm_off_a_middle_click_closes_at_once() {
    let mut state = bar(
        &plain_with(&[("mouse_close_confirm", "false")]),
        80,
        tabs(1, 0),
        vec![(0, vec![floating(1, "vim"), plugin(floating(7, "picker"))])],
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::MiddleClick(0, 0)),
        vec![Call::CloseTerminalPane(1)]
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::MiddleClick(0, 4)),
        vec![Call::ClosePluginPane(7)]
    );
}

/// The labels of the menu `calls` shows, with its id.
fn the_menu(calls: &[Call]) -> (u64, Vec<&'static str>) {
    let (request_id, request) = the_prompt(calls);
    for labels in [
        &["Focus", "Minimize", "Rename", "Close"][..],
        &["Minimize", "Rename", "Close"],
        &["Restore", "Rename", "Close"],
    ] {
        if request == labeled_menu_request(labels.iter().map(|label| label.to_string())) {
            return (request_id, labels.to_vec());
        }
    }
    panic!("not a pane menu: {request:?}");
}

/// `three_panes` with `logs` minimized, from where it stood at `geometry`.
fn three_panes_one_minimized(geometry: &FloatingPaneCoordinates) -> State {
    let mut state = three_panes();
    state
        .minimized_mut()
        .insert(PaneId::Terminal(3), geometry.clone());
    state.update(panes(
        80,
        vec![(
            0,
            vec![
                floating(1, "vim"),
                focused(placed(floating(2, "htop"), (10, 5), (40, 12), false)),
                suppressed(floating(3, "logs")),
            ],
        )],
    ));
    state
}

#[test]
fn a_right_click_on_an_entry_opens_the_menu_of_what_applies_to_its_pane() {
    let geometry = fixed((1, 1), (20, 8), Some(true));
    let mut state = three_panes_one_minimized(&geometry);
    assert_eq!(shown(&state), "vim|[htop]|_logs_");
    for (column, labels) in [
        (0, vec!["Focus", "Minimize", "Rename", "Close"]),
        (5, vec!["Minimize", "Rename", "Close"]),
        (12, vec!["Restore", "Rename", "Close"]),
    ] {
        let (_, shown_labels) = the_menu(&after_mouse(&mut state, Mouse::RightClick(0, column)));
        assert_eq!(shown_labels, labels, "column {column}");
    }
    // At the mouse, and closed by a click outside it.
    let (_, request) = the_prompt(&after_mouse(&mut state, Mouse::RightClick(0, 0)));
    assert_eq!(request.placement, Some(PromptPlacement::Mouse));
    assert_eq!(request.outside_click, Some(OutsideClick::Close));
}

#[test]
fn each_item_of_the_pane_menu_does_what_it_says() {
    let geometry = fixed((1, 1), (20, 8), Some(true));
    let vim = PaneId::Terminal(1);
    let htop = PaneId::Terminal(2);
    let logs = PaneId::Terminal(3);
    let restore_logs = vec![
        focus_terminal(3),
        Call::ChangeFloatingPanesCoordinates(vec![(logs, geometry.clone())]),
    ];
    for (column, item, expected) in [
        (0, 0, vec![focus_terminal(1)]),
        (0, 1, vec![Call::HidePaneWithId(vim)]),
        (0, 2, vec![focus_terminal(1), rename(vim)]),
        (5, 0, vec![Call::HidePaneWithId(htop)]),
        (5, 1, vec![rename(htop)]),
        (12, 0, restore_logs.clone()),
        (12, 1, [restore_logs.clone(), vec![rename(logs)]].concat()),
    ] {
        let mut state = three_panes_one_minimized(&geometry);
        let (request_id, labels) = the_menu(&after_mouse(&mut state, Mouse::RightClick(0, column)));
        assert_eq!(
            after_answer(&mut state, request_id, choose(item)),
            expected,
            "{} at column {column}",
            labels[item]
        );
    }
}

/// `three_panes` once a click has minimized `htop`, and a pane update has
/// shown the hide take effect.
fn three_panes_htop_hidden() -> State {
    let mut state = three_panes();
    after_mouse(&mut state, Mouse::LeftClick(0, 5));
    let hidden = vec![(
        0,
        vec![
            focused(floating(1, "vim")),
            suppressed(floating(2, "htop")),
            floating(3, "logs"),
        ],
    )];
    state.update(panes(80, hidden));
    assert_eq!(shown(&state), "[vim]|_htop_|logs");
    state
}

/// The pane update once Zellij has shown `htop` again some other way than
/// through the bar, at `(30, 2)`, 20 by 6, with `vim` keeping the focus.
fn htop_shown_again() -> Event {
    panes(
        80,
        vec![(
            0,
            vec![
                focused(floating(1, "vim")),
                placed(floating(2, "htop"), (30, 2), (20, 6), false),
                floating(3, "logs"),
            ],
        )],
    )
}

#[test]
fn a_pane_is_kept_minimized_through_an_update_sent_before_its_hide() {
    let mut state = three_panes();
    after_mouse(&mut state, Mouse::LeftClick(0, 5));
    // Sent before the hide took effect: the pane is still on screen in it.
    state.update(htop_shown_again());
    assert!(state.minimized().contains_key(&PaneId::Terminal(2)));
    let hidden = vec![(
        0,
        vec![
            focused(floating(1, "vim")),
            suppressed(floating(2, "htop")),
            floating(3, "logs"),
        ],
    )];
    state.update(panes(80, hidden));
    assert_eq!(shown(&state), "[vim]|_htop_|logs");
}

#[test]
fn a_minimized_pane_shown_again_some_other_way_is_forgotten_and_minimizes_again() {
    let mut state = three_panes_htop_hidden();
    state.update(htop_shown_again());
    assert!(!state.minimized().contains_key(&PaneId::Terminal(2)));
    assert_eq!(shown(&state), "[vim]|htop|logs");

    let (request_id, labels) = the_menu(&after_mouse(&mut state, Mouse::RightClick(0, 7)));
    assert_eq!(labels, ["Focus", "Minimize", "Rename", "Close"]);
    assert_eq!(
        after_answer(&mut state, request_id, choose(1)),
        vec![Call::HidePaneWithId(PaneId::Terminal(2))]
    );
    // Where it stands now, not where it stood when first minimized.
    assert_eq!(
        state.minimized().get(&PaneId::Terminal(2)),
        Some(&fixed((30, 2), (20, 6), Some(false)))
    );
}

#[test]
fn focusing_a_minimized_pane_shown_again_some_other_way_leaves_it_where_it_is() {
    let mut state = three_panes_htop_hidden();
    state.update(htop_shown_again());
    let (request_id, _) = the_menu(&after_mouse(&mut state, Mouse::RightClick(0, 7)));
    assert_eq!(
        after_answer(&mut state, request_id, choose(0)),
        vec![focus_terminal(2)]
    );
}

#[test]
fn minimizing_from_the_menu_records_where_the_pane_stood() {
    let mut state = three_panes();
    let (request_id, _) = the_menu(&after_mouse(&mut state, Mouse::RightClick(0, 5)));
    after_answer(&mut state, request_id, choose(0));
    assert_eq!(
        state.minimized().get(&PaneId::Terminal(2)),
        Some(&fixed((10, 5), (40, 12), Some(false)))
    );
}

#[test]
fn closing_from_the_menu_asks_first_as_a_middle_click_does() {
    let mut state = three_panes();
    let (menu_id, labels) = the_menu(&after_mouse(&mut state, Mouse::RightClick(0, 0)));
    assert_eq!(labels[3], "Close");
    let (close_id, request) = the_prompt(&after_answer(&mut state, menu_id, choose(3)));
    assert_eq!(request.title.as_deref(), Some("Close pane?"));
    // The menu has been answered, so its id again does nothing.
    assert_eq!(after_answer(&mut state, menu_id, choose(3)), vec![]);
    assert_eq!(
        after_answer(&mut state, close_id, PromptResult::Confirmed(true)),
        vec![Call::CloseTerminalPane(1)]
    );

    let mut state = bar(
        &plain_with(&[("mouse_close_confirm", "false")]),
        80,
        tabs(1, 0),
        vec![(0, vec![floating(1, "vim")])],
    );
    let (menu_id, _) = the_menu(&after_mouse(&mut state, Mouse::RightClick(0, 0)));
    assert_eq!(
        after_answer(&mut state, menu_id, choose(3)),
        vec![Call::CloseTerminalPane(1)]
    );
}

#[test]
fn a_menu_dismissed_or_answered_out_of_its_items_does_nothing() {
    for result in [
        PromptResult::Cancelled,
        PromptResult::TimedOut(None),
        PromptResult::Error("no popup".to_string()),
        PromptResult::Confirmed(true),
        choose(4),
        PromptResult::Answered(PromptValue::Choice("Focus".to_string())),
    ] {
        let mut state = three_panes();
        let (request_id, _) = the_menu(&after_mouse(&mut state, Mouse::RightClick(0, 0)));
        assert_eq!(
            after_answer(&mut state, request_id, result.clone()),
            vec![],
            "{result:?}"
        );
        assert!(!state.pending_prompt.is_waiting(), "{result:?}");
    }
}

#[test]
fn a_menu_answer_for_another_prompt_or_a_pane_gone_since_does_nothing() {
    let mut state = three_panes();
    let (request_id, _) = the_menu(&after_mouse(&mut state, Mouse::RightClick(0, 0)));
    assert_eq!(after_answer(&mut state, request_id + 1, choose(0)), vec![]);
    assert!(state.pending_prompt.is_waiting());
    state.update(panes(80, vec![(0, vec![focused(floating(2, "htop"))])]));
    assert_eq!(after_answer(&mut state, request_id, choose(0)), vec![]);
}

#[test]
fn a_close_without_asking_leaves_a_pane_the_bar_no_longer_lists_alone() {
    let without_vim = || panes(80, vec![(0, vec![floating(2, "htop")])]);
    let mut state = bar(
        &plain_with(&[("mouse_close_confirm", "false")]),
        80,
        tabs(1, 0),
        vec![(0, vec![floating(1, "vim"), floating(2, "htop")])],
    );
    let (menu_id, labels) = the_menu(&after_mouse(&mut state, Mouse::RightClick(0, 0)));
    assert_eq!(labels[3], "Close");
    state.update(without_vim());
    assert_eq!(after_answer(&mut state, menu_id, choose(3)), vec![]);

    // A middle click on the line as drawn before the pane went.
    let mut state = bar(
        &plain_with(&[("mouse_close_confirm", "false")]),
        80,
        tabs(1, 0),
        vec![(0, vec![floating(1, "vim"), floating(2, "htop")])],
    );
    state.render(1, 80);
    state.update(without_vim());
    state.host.clear();
    state.update(Event::Mouse(Mouse::MiddleClick(0, 0)));
    assert_eq!(state.host.calls(), vec![]);
}

#[test]
fn a_menu_item_acts_on_the_pane_as_it_stands_when_chosen() {
    // Minimized from somewhere else while the menu was open: Focus restores.
    let geometry = fixed((1, 1), (20, 8), Some(false));
    let mut state = three_panes();
    let (request_id, labels) = the_menu(&after_mouse(&mut state, Mouse::RightClick(0, 12)));
    assert_eq!(labels[0], "Focus");
    state
        .minimized_mut()
        .insert(PaneId::Terminal(3), geometry.clone());
    assert_eq!(
        after_answer(&mut state, request_id, choose(0)),
        vec![
            focus_terminal(3),
            Call::ChangeFloatingPanesCoordinates(vec![(PaneId::Terminal(3), geometry)]),
        ]
    );
    // Minimizing a pane already minimized does nothing more.
    let geometry = fixed((1, 1), (20, 8), Some(false));
    let mut state = three_panes();
    let (request_id, _) = the_menu(&after_mouse(&mut state, Mouse::RightClick(0, 12)));
    state.minimized_mut().insert(PaneId::Terminal(3), geometry);
    assert_eq!(after_answer(&mut state, request_id, choose(1)), vec![]);
}

#[test]
fn the_wheel_steps_the_focus_through_the_panes_on_screen() {
    let geometry = fixed((1, 1), (20, 8), Some(false));
    let mut state = bar(
        &PLAIN,
        80,
        tabs(1, 0),
        vec![(
            0,
            vec![
                floating(1, "a"),
                suppressed(floating(2, "b")),
                focused(floating(3, "c")),
                floating(4, "d"),
            ],
        )],
    );
    state.minimized_mut().insert(PaneId::Terminal(2), geometry);
    state.refresh_entries();
    assert_eq!(shown(&state), "a|_b_|[c]|d");
    // Up for the next, as zjtabs and Zellij's own tab bar go, and down for
    // the previous, past the minimized pane rather than restoring it.
    assert_eq!(
        after_mouse(&mut state, Mouse::ScrollUp(1)),
        vec![focus_terminal(4)]
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::ScrollDown(1)),
        vec![focus_terminal(1)]
    );
    // At either end, nothing.
    state.update(panes(
        80,
        vec![(
            0,
            vec![
                floating(1, "a"),
                suppressed(floating(2, "b")),
                floating(3, "c"),
                focused(floating(4, "d")),
            ],
        )],
    ));
    assert_eq!(after_mouse(&mut state, Mouse::ScrollUp(1)), vec![]);
    assert_eq!(
        after_mouse(&mut state, Mouse::ScrollDown(1)),
        vec![focus_terminal(3)]
    );
}

#[test]
fn reversed_the_wheel_steps_the_other_way_and_with_none_focused_starts_at_an_end() {
    let mut state = bar(
        &plain_with(&[("mouse_wheel_reversed", "true")]),
        80,
        tabs(1, 0),
        vec![(
            0,
            vec![
                floating(1, "a"),
                focused(floating(2, "b")),
                floating(3, "c"),
            ],
        )],
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::ScrollUp(1)),
        vec![focus_terminal(1)]
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::ScrollDown(1)),
        vec![focus_terminal(3)]
    );

    let mut infos = tabs(1, 0);
    infos[0].are_floating_panes_visible = false;
    let mut state = bar(&PLAIN, 80, infos, letters(3, 1));
    assert_eq!(shown(&state), "a|b|c");
    assert_eq!(
        after_mouse(&mut state, Mouse::ScrollUp(1)),
        vec![focus_terminal(1)]
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::ScrollDown(1)),
        vec![focus_terminal(3)]
    );
}

#[test]
fn with_the_mouse_off_nothing_happens() {
    let mut state = bar(
        &plain_with(&[("mouse", "false")]),
        80,
        tabs(1, 0),
        vec![(0, vec![floating(1, "a")])],
    );
    for mouse in [
        Mouse::LeftClick(0, 0),
        Mouse::RightClick(0, 0),
        Mouse::MiddleClick(0, 0),
        Mouse::DoubleClick(0, 0),
        Mouse::ScrollUp(1),
    ] {
        assert_eq!(after_mouse(&mut state, mouse), vec![], "{mouse:?}");
    }
}

#[test]
fn a_bar_with_a_blank_line_ignores_the_mouse() {
    let nested = {
        let mut mode_info = mode();
        mode_info.session_ancestry = vec!["host".to_string()];
        Event::ModeUpdate(mode_info)
    };
    let by_tab = || vec![(0, vec![focused(floating(1, "a")), floating(2, "b")])];
    let mut hidden = bar(&plain_with(&[("hidden", "true")]), 80, tabs(1, 0), by_tab());
    let mut nested_bar = bar(&PLAIN, 80, tabs(1, 0), by_tab());
    nested_bar.update(nested);
    let mut empty = bar(&PLAIN, 80, tabs(1, 0), vec![]);
    for (name, state) in [
        ("hidden", &mut hidden),
        ("nested", &mut nested_bar),
        ("empty", &mut empty),
    ] {
        assert_eq!(shown(state), "", "{name}");
        for mouse in [
            Mouse::LeftClick(0, 0),
            Mouse::RightClick(0, 0),
            Mouse::MiddleClick(0, 0),
            Mouse::DoubleClick(0, 0),
            Mouse::ScrollUp(1),
            Mouse::ScrollDown(1),
        ] {
            assert_eq!(after_mouse(state, mouse), vec![], "{name}: {mouse:?}");
        }
    }
    // With `empty_format`, an empty bar draws something, and a right-click
    // on it opens the bar menu.
    let mut state = bar(
        &plain_with(&[("empty_format", "no panes")]),
        80,
        tabs(1, 0),
        vec![],
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::RightClick(0, 3)),
        vec![Call::OpenContextMenu {
            target: ContextMenuTarget::Bar,
            line: 0,
            column: 3,
        }]
    );
    assert_eq!(after_mouse(&mut state, Mouse::LeftClick(0, 3)), vec![]);
}

#[test]
fn hovering_dragging_or_pressing_below_the_bar_s_line_does_nothing() {
    let mut state = three_panes();
    for mouse in [
        Mouse::Hover(0, 0),
        Mouse::Hold(0, 0),
        Mouse::Release(0, 0),
        Mouse::LeftClick(1, 0),
        Mouse::RightClick(1, 0),
        Mouse::MiddleClick(1, 0),
    ] {
        assert_eq!(after_mouse(&mut state, mouse), vec![], "{mouse:?}");
    }
}

// The launcher.

/// `PLAIN` with the launcher's button on and drawn as a bare `+`, so the
/// line reads as plain text, and `pairs` added.
fn launcher_with(pairs: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
    let mut all = plain_with(&[
        ("show_launcher", "true"),
        ("launcher_format", "{symbol}"),
        ("launcher_symbol", "+"),
    ]);
    all.extend_from_slice(pairs);
    all
}

/// Two entries of the user's own: a command, then a plugin.
const LAUNCH_ENTRIES: [(&str, &str); 7] = [
    ("launch_1_name", "Top"),
    ("launch_1_command", "htop"),
    ("launch_1_args", "-d 10"),
    ("launch_1_cwd", "/srv"),
    ("launch_2_name", "Files"),
    ("launch_2_plugin", "zellij:strider"),
    ("launch_2_config_theme", "dark"),
];

/// The launcher's menu with every group on and `LAUNCH_ENTRIES`.
const FULL_LAUNCHER_MENU: [&str; 6] = [
    "New terminal",
    "Top",
    "Files",
    "Session manager",
    "Settings",
    "Plugin manager",
];

/// A bar with the launcher on, `LAUNCH_ENTRIES`, `pairs`, and the panes of
/// `three_panes`, drawn `+|vim|[htop]|logs`.
fn launcher_bar(pairs: &[(&'static str, &'static str)]) -> State {
    let mut options = launcher_with(&LAUNCH_ENTRIES);
    options.extend_from_slice(pairs);
    bar(
        &options,
        80,
        tabs(1, 0),
        vec![(
            0,
            vec![
                floating(1, "vim"),
                focused(floating(2, "htop")),
                floating(3, "logs"),
            ],
        )],
    )
}

/// The menu prompt that lists `labels`.
fn launcher_menu(labels: &[&str]) -> PromptRequest {
    labeled_menu_request(labels.iter().map(|label| label.to_string()))
}

/// The id of the launcher's menu a left click on its button opens.
fn open_launcher(state: &mut State) -> u64 {
    the_prompt(&after_mouse(state, Mouse::LeftClick(0, 0))).0
}

#[test]
fn the_launcher_s_button_leads_the_bar_and_a_left_click_opens_its_menu() {
    let mut state = launcher_bar(&[]);
    assert_eq!(shown(&state), "+|vim|[htop]|logs");
    let (_, request) = the_prompt(&after_mouse(&mut state, Mouse::LeftClick(0, 0)));
    assert_eq!(request, launcher_menu(&FULL_LAUNCHER_MENU));
    // The entries' columns start after the button and its spacer.
    let at = |column| state.click_spans.value_at(column).copied();
    assert_eq!(at(0), Some(Clickable::Launcher));
    assert_eq!(at(1), None);
    for column in 2..5 {
        assert_eq!(at(column), Some(Clickable::Entry(PaneId::Terminal(1))));
    }
    assert_eq!(at(5), None);
    assert_eq!(at(6), Some(Clickable::Entry(PaneId::Terminal(2))));
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 2)),
        vec![focus_terminal(1)]
    );
}

#[test]
fn the_launcher_s_menu_lists_only_the_groups_turned_on() {
    for (options, labels) in [
        (vec![], FULL_LAUNCHER_MENU.to_vec()),
        (
            vec![("launcher_new_terminal", "false")],
            vec![
                "Top",
                "Files",
                "Session manager",
                "Settings",
                "Plugin manager",
            ],
        ),
        (
            vec![("launcher_zellij_actions", "false")],
            vec!["New terminal", "Top", "Files"],
        ),
        (
            vec![
                ("launcher_new_terminal", "false"),
                ("launcher_zellij_actions", "false"),
            ],
            vec!["Top", "Files"],
        ),
    ] {
        let mut state = launcher_bar(&options);
        let (_, request) = the_prompt(&after_mouse(&mut state, Mouse::LeftClick(0, 0)));
        assert_eq!(request, launcher_menu(&labels), "{options:?}");
    }
    // With no entries of the user's either, the menu would be empty, so the
    // button is not drawn and the entries start at column 0.
    let mut state = bar(
        &launcher_with(&[
            ("launcher_new_terminal", "false"),
            ("launcher_zellij_actions", "false"),
        ]),
        80,
        tabs(1, 0),
        vec![(0, vec![floating(1, "vim")])],
    );
    assert_eq!(shown(&state), "vim");
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 0)),
        vec![focus_terminal(1)]
    );
}

#[test]
fn each_item_of_the_launcher_s_menu_opens_what_it_names() {
    let run = |action| Call::RunAction {
        action,
        context: BTreeMap::new(),
    };
    let expected = [
        run(zjkit::launch::new_floating_terminal()),
        Call::OpenCommandPaneFloating {
            path: PathBuf::from("htop"),
            args: vec!["-d".to_string(), "10".to_string()],
            cwd: Some(PathBuf::from("/srv")),
            coordinates: None,
            context: BTreeMap::new(),
        },
        Call::OpenPluginPaneFloating {
            url: "zellij:strider".to_string(),
            configuration: cfg(&[("theme", "dark")]),
            coordinates: None,
            context: BTreeMap::new(),
        },
        run(zjkit::launch::launch_or_focus_floating(
            zjkit::launch::SESSION_MANAGER_ALIAS,
        )),
        run(zjkit::launch::launch_or_focus_floating(
            zjkit::launch::CONFIGURATION_ALIAS,
        )),
        run(zjkit::launch::launch_or_focus_floating(
            zjkit::launch::PLUGIN_MANAGER_ALIAS,
        )),
    ];
    for (index, call) in expected.into_iter().enumerate() {
        let mut state = launcher_bar(&[]);
        let request_id = open_launcher(&mut state);
        assert_eq!(
            after_answer(&mut state, request_id, choose(index)),
            vec![call],
            "{}",
            FULL_LAUNCHER_MENU[index]
        );
        assert!(!state.pending_prompt.is_waiting());
    }
}

#[test]
fn a_launcher_menu_dismissed_or_answered_out_of_its_items_opens_nothing() {
    for result in [
        PromptResult::Cancelled,
        PromptResult::TimedOut(None),
        PromptResult::Error("no popup".to_string()),
        PromptResult::Confirmed(true),
        choose(FULL_LAUNCHER_MENU.len()),
        PromptResult::Answered(PromptValue::Choice("-1".to_string())),
        PromptResult::Answered(PromptValue::Choice("Top".to_string())),
    ] {
        let mut state = launcher_bar(&[]);
        let request_id = open_launcher(&mut state);
        assert_eq!(
            after_answer(&mut state, request_id, result.clone()),
            vec![],
            "{result:?}"
        );
        assert!(!state.pending_prompt.is_waiting(), "{result:?}");
    }
}

#[test]
fn a_launcher_menu_answer_for_another_prompt_or_one_replaced_opens_nothing() {
    let mut state = launcher_bar(&[]);
    let request_id = open_launcher(&mut state);
    assert_eq!(after_answer(&mut state, request_id + 1, choose(0)), vec![]);
    assert!(state.pending_prompt.is_waiting());

    // A pane menu opened since takes the launcher's place.
    let (pane_menu_id, labels) = the_menu(&after_mouse(&mut state, Mouse::RightClick(0, 2)));
    assert_eq!(labels[0], "Focus");
    assert_eq!(after_answer(&mut state, request_id, choose(0)), vec![]);
    assert_eq!(
        after_answer(&mut state, pane_menu_id, choose(0)),
        vec![focus_terminal(1)]
    );
}

#[test]
fn a_launcher_menu_opens_the_item_it_showed_when_it_opened() {
    let mut state = launcher_bar(&[]);
    let request_id = open_launcher(&mut state);
    let mut options = launcher_with(&LAUNCH_ENTRIES);
    options.push(("launcher_new_terminal", "false"));
    BarCore::follow(&mut state, &BAR, Handled::Reconfigure(cfg(&options)));
    assert_eq!(
        after_answer(&mut state, request_id, choose(0)),
        vec![Call::RunAction {
            action: zjkit::launch::new_floating_terminal(),
            context: BTreeMap::new(),
        }]
    );
}

#[test]
fn a_right_click_on_the_launcher_s_button_opens_the_bar_menu_and_other_clicks_do_nothing() {
    let mut state = launcher_bar(&[]);
    assert_eq!(
        after_mouse(&mut state, Mouse::RightClick(0, 0)),
        vec![Call::OpenContextMenu {
            target: ContextMenuTarget::Bar,
            line: 0,
            column: 0,
        }]
    );
    for mouse in [Mouse::MiddleClick(0, 0), Mouse::DoubleClick(0, 0)] {
        assert_eq!(after_mouse(&mut state, mouse), vec![], "{mouse:?}");
    }
    // The second click of a double-click opens no second menu.
    let start = Instant::now();
    the_prompt(&after_mouse_at(&mut state, Mouse::LeftClick(0, 0), start));
    let soon = start + Duration::from_millis(100);
    assert_eq!(
        after_mouse_at(&mut state, Mouse::LeftClick(0, 0), soon),
        vec![]
    );
}

#[test]
fn with_the_mouse_or_show_launcher_off_no_button_is_drawn() {
    for options in [vec![("mouse", "false")], vec![("show_launcher", "false")]] {
        let state = launcher_bar(&options);
        assert_eq!(shown(&state), "vim|[htop]|logs", "{options:?}");
        assert_ne!(
            state.draw_bar().1.value_at(0),
            Some(&Clickable::Launcher),
            "{options:?}"
        );
    }
}

#[test]
fn the_entries_give_way_before_the_launcher_s_button() {
    for (width, expected) in [
        (1, "+"),
        // Too narrow for anything after the spacer.
        (2, "+"),
        (3, "+|."),
        (6, "+|[f]"),
        (15, "+|<4|e|[f]|g|3>"),
        (40, "+|a|b|c|d|e|[f]|g|h|i|j"),
    ] {
        let state = bar(&launcher_with(&[]), width, tabs(1, 0), letters(10, 5));
        let line = shown(&state);
        assert_eq!(line, expected, "{width}");
        assert!(line.chars().count() <= width, "{width}: {line:?}");
        assert_eq!(
            state.draw_bar().1.value_at(0),
            Some(&Clickable::Launcher),
            "{width}"
        );
    }
    // A button wider than the bar is cut to it with `overflow_str`, as an
    // entry is, and what is left of it still opens the menu. The cut stands
    // for nothing, as an entry's does.
    let mut state = bar(
        &launcher_with(&[("launcher_symbol", "Launch")]),
        4,
        tabs(1, 0),
        letters(3, 1),
    );
    assert_eq!(shown(&state), "L...");
    the_prompt(&after_mouse(&mut state, Mouse::LeftClick(0, 0)));
    assert_eq!(state.click_spans.value_at(1), None);
}

#[test]
fn the_launcher_s_button_keeps_an_empty_bar_s_row_and_its_click() {
    let mut state = bar(&launcher_with(&[]), 80, tabs(1, 0), vec![]);
    assert_eq!(shown(&state), "+");
    state.host.clear();
    state.render(1, 80);
    assert_eq!(state.host.calls(), vec![]);
    the_prompt(&after_mouse(&mut state, Mouse::LeftClick(0, 0)));
    // With `empty_format`, it is drawn after the button.
    let state = bar(
        &launcher_with(&[("empty_format", "no panes")]),
        80,
        tabs(1, 0),
        vec![],
    );
    assert_eq!(shown(&state), "+|no panes");
}

#[test]
fn the_launcher_s_button_dims_with_the_bar() {
    let undimmed = bar(&[], 80, tabs(1, 0), vec![]).draw_bar().0;
    assert_eq!(strip_ansi(&undimmed), " Menu ");
    let mut state = bar(&[], 80, tabs(1, 0), vec![]);
    let mut mode_info = mode();
    mode_info.session_dimmed = Some(true);
    state.update(Event::ModeUpdate(mode_info));
    let dimmed = state.draw_bar().0;
    assert_ne!(dimmed, undimmed);
    assert_eq!(strip_ansi(&dimmed), " Menu ");
}

#[test]
fn the_launcher_asks_for_what_its_menu_can_open_and_nothing_more() {
    let commands = &LAUNCH_ENTRIES[..4];
    let plugins = &LAUNCH_ENTRIES[4..];
    let groups_off = [
        ("launcher_new_terminal", "false"),
        ("launcher_zellij_actions", "false"),
    ];
    let mouse = Feature::Mouse;
    let launcher = Feature::Launcher;
    let zellij_action = Feature::LauncherZellijAction;
    let command = Feature::LauncherCommand;
    for (options, features) in [
        (vec![], vec![mouse, launcher, zellij_action]),
        (
            commands.to_vec(),
            vec![mouse, launcher, zellij_action, command],
        ),
        (plugins.to_vec(), vec![mouse, launcher, zellij_action]),
        ([plugins, &groups_off].concat(), vec![mouse, launcher]),
        (
            [commands, &groups_off].concat(),
            vec![mouse, launcher, command],
        ),
        // An empty menu draws no button, so asks for nothing.
        (groups_off.to_vec(), vec![mouse]),
        // Neither does a button not drawn.
        (
            [commands, &[("show_launcher", "false")]].concat(),
            vec![mouse],
        ),
        ([commands, &[("mouse", "false")]].concat(), vec![]),
        // An entry left off the menu runs no command.
        (
            vec![("launch_1_command", "htop")],
            vec![mouse, launcher, zellij_action],
        ),
    ] {
        // The frame buttons' close confirmation is a feature of its own.
        let state = loaded(&[options.as_slice(), &[("frame_buttons", "false")]].concat());
        assert_eq!(state.features(), features, "{options:?}");
        assert_eq!(
            BarPlugin::permissions(&state, &BAR),
            bar_permissions(&features),
            "{options:?}"
        );
    }
    // By default, the launcher asks for nothing the mouse has not.
    assert_eq!(
        bar_permissions(&[mouse, launcher, zellij_action]),
        bar_permissions(&[mouse])
    );
    // A command added later asks to run commands then.
    let mut state = loaded(&[]);
    state.host.clear();
    BarCore::follow(&mut state, &BAR, Handled::Reconfigure(cfg(commands)));
    assert!(
        state
            .host
            .calls()
            .contains(&Call::RequestPermission(bar_permissions(&[
                mouse,
                launcher,
                zellij_action,
                command
            ]))),
        "{:?}",
        state.host.calls()
    );
}

// The tray.

use std::time::{SystemTime, UNIX_EPOCH};
use zjkit::launch::{launch_or_focus_floating, SESSION_MANAGER_ALIAS};

/// 2026-11-03 at `hour:minute:second` UTC.
fn utc(hour: u64, minute: u64, second: u64) -> SystemTime {
    const MIDNIGHT: u64 = 1_793_664_000;
    UNIX_EPOCH + Duration::from_secs(MIDNIGHT + hour * 3600 + minute * 60 + second)
}

/// `PLAIN` with the tray named `tray`, each item drawn as bare text with a
/// `~` between them, and the clock at UTC, so it draws at once.
fn tray_with(
    tray: &'static str,
    pairs: &[(&'static str, &'static str)],
) -> Vec<(&'static str, &'static str)> {
    let mut all = plain_with(&[
        ("tray", tray),
        ("tray_spacer", "~"),
        ("clock_item_format", "{clock}"),
        ("session_format", "{session}"),
        ("mode_format", "{desc}"),
        ("clock_utc_offset", "+00:00"),
    ]);
    all.extend_from_slice(pairs);
    all
}

/// The mode update of a session named `work` in normal mode.
fn mode_of_work() -> ModeInfo {
    let mut mode_info = mode();
    mode_info.session_name = Some("work".to_string());
    mode_info
}

/// A bar with `pairs`, `width` columns wide, in the session `work`, listing
/// `a`, focused, and `b`, loaded when the system clock reads `now`.
fn tray_bar(pairs: &[(&str, &str)], width: usize, now: SystemTime) -> State {
    let mut state = State::default();
    state.host.set_system_time(now);
    state.load(cfg(pairs));
    state.update(Event::ModeUpdate(mode_of_work()));
    state.update(panes(
        width,
        vec![(0, vec![focused(floating(1, "a")), floating(2, "b")])],
    ));
    state.update(Event::TabUpdate(tabs(1, 0)));
    state
}

/// What each column of the line the bar draws stands for, as the tray item
/// under it, for the columns with one.
fn tray_columns(state: &State, width: usize) -> Vec<(usize, TrayItem)> {
    let spans = state.draw_bar().1;
    (0..width)
        .filter_map(|column| match spans.value_at(column) {
            Some(Clickable::Tray(item)) => Some((column, *item)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_tray_is_empty_by_default_and_asks_for_nothing() {
    let state = loaded(&[]);
    assert_eq!(state.tray, []);
    assert_eq!(state.clock.format, DEFAULT_CLOCK_FORMAT);
    assert_eq!(state.tray_spacer, DEFAULT_TRAY_SPACER);
    assert!(!state.features().contains(&Feature::ClockCommand));
}

#[test]
fn the_tray_stands_flush_with_the_right_end_after_a_blank() {
    let state = tray_bar(&tray_with("mode session clock", &[]), 40, utc(12, 0, 0));
    let line = shown(&state);
    assert_eq!(line, format!("[a]|b{}~NORMAL~work~12:00", " ".repeat(17)));
    assert_eq!(line.chars().count(), 40);
    let columns = tray_columns(&state, 40);
    let at = |item: TrayItem| -> Vec<usize> {
        columns
            .iter()
            .filter(|(_, of)| *of == item)
            .map(|(column, _)| *column)
            .collect()
    };
    assert_eq!(at(TrayItem::Mode), (23..29).collect::<Vec<_>>());
    assert_eq!(at(TrayItem::Session), (30..34).collect::<Vec<_>>());
    assert_eq!(at(TrayItem::Clock), (35..40).collect::<Vec<_>>());
}

#[test]
fn a_huge_max_length_pads_the_tray_only_to_the_pane_s_edge() {
    let options = tray_with(
        "mode session clock",
        &[("auto_width", "false"), ("max_length", "4000000000")],
    );
    let state = tray_bar(&options, 40, utc(12, 0, 0));
    assert_eq!(
        shown(&state),
        format!("[a]|b{}~NORMAL~work~12:00", " ".repeat(17))
    );
    assert_eq!(
        tray_columns(&state, 40).last(),
        Some(&(39, TrayItem::Clock))
    );

    // With no pane seen there is no edge to stand against, and the tray
    // follows the entries.
    let mut state = State::default();
    state.host.set_system_time(utc(12, 0, 0));
    state.load(cfg(&options));
    state.update(Event::ModeUpdate(mode_of_work()));
    state.update(Event::PaneUpdate(PaneManifest {
        panes: HashMap::from([(0, vec![focused(floating(1, "a")), floating(2, "b")])]),
    }));
    state.update(Event::TabUpdate(tabs(1, 0)));
    assert_eq!(shown(&state), "[a]|b~NORMAL~work~12:00");
}

/// The text of `line` drawn on `columns`, measured by grapheme cluster as
/// Zellij lays the line out.
fn text_at(line: &str, columns: std::ops::Range<usize>) -> String {
    let plain = strip_ansi(line);
    let (before, _) = zjkit::ansi::prefix_within(&plain, columns.start);
    let (through, _) = zjkit::ansi::prefix_within(&plain, columns.end);
    through[before.len()..].to_string()
}

/// The tray's items in the line `state` draws, left to right, each with its
/// text, from its click span, and whether the line ends with it on the edge
/// `BarCore::edge` reports.
fn tray_drawn(state: &State) -> Vec<(TrayItem, String, bool)> {
    let (line, spans) = state.draw_bar();
    let width = zjkit::ansi::calculate_visible_length(&line);
    let mut drawn: Vec<(TrayItem, std::ops::Range<usize>)> = Vec::new();
    for span in (0..width).filter_map(|column| spans.span_at(column)) {
        if let Clickable::Tray(item) = span.value {
            if drawn.last().map(|(_, columns)| columns) != Some(&span.columns) {
                drawn.push((item, span.columns.clone()));
            }
        }
    }
    drawn
        .into_iter()
        .map(|(item, columns)| {
            let flush = Some(columns.end) == state.bar.edge() && width == columns.end;
            (item, text_at(&line, columns), flush)
        })
        .collect()
}

/// Whether `text`, a tray item drawn by the pills theme, is the whole pill
/// rather than the half one that closes the pane.
fn whole_pill(text: &str) -> bool {
    text.starts_with(zjkit::theme::PILL_LEFT) && text.ends_with(zjkit::theme::PILL_RIGHT)
}

/// The tray `mode session clock` under the format theme `theme`, with the
/// clock at UTC, and `pairs`.
fn themed_tray(
    theme: &'static str,
    pairs: &[(&'static str, &'static str)],
) -> Vec<(&'static str, &'static str)> {
    let mut all = vec![
        ("format_theme", theme),
        ("tray", "mode session clock"),
        ("clock_utc_offset", "+00:00"),
    ];
    all.extend_from_slice(pairs);
    all
}

#[test]
fn a_pill_tray_that_ends_the_pane_draws_only_its_last_item_as_the_half_pill() {
    let state = tray_bar(&themed_tray("pills", &[]), 80, utc(12, 0, 0));
    let line = state.draw_bar().0;
    assert_eq!(zjkit::ansi::calculate_visible_length(&line), 80);
    let drawn = tray_drawn(&state);
    let pill_left = zjkit::theme::PILL_LEFT;
    let pill_right = zjkit::theme::PILL_RIGHT;
    assert_eq!(
        drawn,
        vec![
            (
                TrayItem::Mode,
                format!("{pill_left} NORMAL {pill_right}"),
                false
            ),
            (
                TrayItem::Session,
                format!("{pill_left} work {pill_right}"),
                false
            ),
            (TrayItem::Clock, format!("{pill_left} 12:00 "), true),
        ],
        "{line:?}"
    );
    // The last item left alone is the one at the edge.
    let options = themed_tray("pills", &[("tray", "clock session")]);
    let state = tray_bar(&options, 80, utc(12, 0, 0));
    let drawn = tray_drawn(&state);
    assert!(whole_pill(&drawn[0].1), "{drawn:?}");
    assert_eq!(
        drawn[1],
        (TrayItem::Session, format!("{pill_left} work "), true)
    );
}

#[test]
fn a_pill_tray_short_of_the_pane_s_edge_is_drawn_whole() {
    for options in [
        themed_tray("pills", &[("max_length", "60")]),
        themed_tray("pills", &[("auto_width", "false")]),
        themed_tray("pills", &[("auto_width", "false"), ("max_length", "60")]),
    ] {
        let state = tray_bar(&options, 80, utc(12, 0, 0));
        let drawn = tray_drawn(&state);
        assert_eq!(drawn.len(), 3, "{options:?}");
        for (item, text, flush) in drawn {
            assert!(!flush, "{options:?} {item:?}");
            assert!(whole_pill(&text), "{options:?} {item:?}: {text:?}");
        }
    }
    // Before Zellij has reported the pane, there is no edge to end at.
    let mut state = State::default();
    state.host.set_system_time(utc(12, 0, 0));
    state.load(cfg(&themed_tray("pills", &[("max_length", "60")])));
    state.update(Event::ModeUpdate(mode_of_work()));
    state.update(Event::TabUpdate(tabs(1, 0)));
    let drawn = tray_drawn(&state);
    assert_eq!(drawn.len(), 3);
    assert!(
        drawn.iter().all(|(_, text, _)| whole_pill(text)),
        "{drawn:?}"
    );
}

#[test]
fn the_tray_ends_where_zellij_renders_it_before_the_pane_update_says_so() {
    // Resized from 80 to 84 columns: Zellij renders at 84 before the pane
    // update reporting the new width arrives.
    let mut state = tray_bar(&themed_tray("pills", &[]), 80, utc(12, 0, 0));
    state.render(1, 84);
    let line = state.draw_bar().0;
    assert_eq!(zjkit::ansi::calculate_visible_length(&line), 84);
    let drawn = tray_drawn(&state);
    let (item, text, flush) = drawn.last().expect("the tray is drawn");
    assert_eq!(*item, TrayItem::Clock);
    assert!(flush);
    assert!(!whole_pill(text), "{text:?}");
}

#[test]
fn a_huge_max_length_sets_the_tray_at_the_pane_s_edge_as_the_half_pill() {
    let options = themed_tray(
        "pills",
        &[("auto_width", "false"), ("max_length", "4000000000")],
    );
    let state = tray_bar(&options, 50, utc(12, 0, 0));
    let line = state.draw_bar().0;
    assert_eq!(zjkit::ansi::calculate_visible_length(&line), 50);
    let (_, text, flush) = tray_drawn(&state).pop().expect("the tray is drawn");
    assert!(flush);
    assert!(!whole_pill(&text), "{text:?}");
}

#[test]
fn a_tray_format_of_the_user_s_own_is_drawn_as_written_at_the_edge_or_not() {
    let pill_left = zjkit::theme::PILL_LEFT;
    let pill_right = zjkit::theme::PILL_RIGHT;
    let half = "#[fg=$surface]\u{e0b6}#[fg=$fg,bg=$surface] {clock} ";
    let whole = "#[fg=$surface]\u{e0b6}#[fg=$fg,bg=$surface] {clock} #[fg=$surface]\u{e0b4}";
    for (format, drawn) in [
        (half, format!("{pill_left} 12:00 ")),
        (whole, format!("{pill_left} 12:00 {pill_right}")),
    ] {
        for max_length in ["0", "60"] {
            let options = themed_tray(
                "pills",
                &[
                    ("max_length", max_length),
                    ("clock_item_format", format),
                    ("session_format", "[{session}]"),
                    ("mode_format", "<{desc}>"),
                ],
            );
            let state = tray_bar(&options, 80, utc(12, 0, 0));
            let texts: Vec<String> = tray_drawn(&state)
                .into_iter()
                .map(|(_, text, _)| text)
                .collect();
            assert_eq!(
                texts,
                ["<NORMAL>".to_string(), "[work]".to_string(), drawn.clone()],
                "{format:?} {max_length}"
            );
        }
    }
}

#[test]
fn the_tray_s_edge_look_is_drawn_only_on_a_line_that_ends_at_the_edge() {
    // How many lines ended in the half pill, which some must.
    let mut halves = 0;
    for (theme, _) in zjkit::theme::FORMAT_THEMES {
        for (auto_width, max_length) in [
            ("true", "0"),
            ("true", "50"),
            ("false", "50"),
            ("false", "0"),
            ("false", "4000000000"),
        ] {
            for launcher in ["false", "true"] {
                let options = themed_tray(
                    theme,
                    &[
                        ("auto_width", auto_width),
                        ("max_length", max_length),
                        ("show_launcher", launcher),
                    ],
                );
                for width in 1..=120 {
                    for rendered in [width, width + 3] {
                        let mut state = tray_bar(&options, width, utc(12, 0, 0));
                        state.render(1, rendered);
                        let line = state.draw_bar().0;
                        let drawn = zjkit::ansi::calculate_visible_length(&line);
                        let case = format!("{options:?} {width} {rendered}: {line:?}");
                        let tray = tray_drawn(&state);
                        for (index, (_, text, flush)) in tray.iter().enumerate() {
                            // A line that happens to end at the edge may
                            // still end in a whole pill, but a half one only
                            // ever ends there, and only as the last item.
                            if theme == "pills" && !whole_pill(text) {
                                assert_eq!(index, tray.len() - 1, "{case}");
                                assert!(*flush, "{case}");
                                assert_eq!(drawn, rendered, "{case}");
                                halves += 1;
                            }
                        }
                        if auto_width == "true" {
                            assert!(drawn <= rendered, "{case}");
                        }
                        if !tray.is_empty() && max_length == "4000000000" {
                            assert!(drawn <= rendered, "{case}");
                        }
                    }
                }
            }
        }
    }
    assert!(halves > 0);
}

#[test]
fn the_entries_give_way_to_the_tray_and_the_tray_drops_items_from_its_left() {
    let options = tray_with("mode session clock", &[]);
    for (width, line) in [
        // The entries fit beside the whole tray, with the gap between them.
        (23, "[a]|b~NORMAL~work~12:00".to_string()),
        // They go first, whole, and the tray stands alone at the right end.
        (18, " NORMAL~work~12:00".to_string()),
        (17, "NORMAL~work~12:00".to_string()),
        // Then the tray drops the item it lists first, and the entries take
        // what that frees as far as they fit.
        (16, "[a]|b~work~12:00".to_string()),
        (14, "[a]~work~12:00".to_string()),
        (10, "work~12:00".to_string()),
        (9, "[a]~12:00".to_string()),
        (6, " 12:00".to_string()),
        (5, "12:00".to_string()),
    ] {
        let state = tray_bar(&options, width, utc(12, 0, 0));
        assert_eq!(shown(&state), line, "{width}");
    }
    // Too narrow for even the clock, the tray is gone, and the entries take
    // the line again.
    let state = tray_bar(&options, 4, utc(12, 0, 0));
    assert!(tray_columns(&state, 4).is_empty());
    assert!(shown(&state).starts_with("[a]"), "{:?}", shown(&state));
}

#[test]
fn the_launcher_s_button_outranks_the_tray() {
    let options = [
        &tray_with("mode session clock", &[])[..],
        &[
            ("show_launcher", "true"),
            ("launcher_format", "{symbol}"),
            ("launcher_symbol", "+"),
        ],
    ]
    .concat();
    for (width, line) in [
        (40, format!("+|[a]|b{}~NORMAL~work~12:00", " ".repeat(15))),
        // Beside the button, the tray keeps a spacer before it too.
        (19, "+~NORMAL~work~12:00".to_string()),
        (18, "+|[a]|b~work~12:00".to_string()),
        (7, "+~12:00".to_string()),
        // With no room for even the clock, the button stands alone.
        (2, "+".to_string()),
    ] {
        let state = tray_bar(&options, width, utc(12, 0, 0));
        let drawn = shown(&state);
        assert_eq!(drawn.trim_end(), line.trim_end(), "{width}");
        assert_eq!(state.draw_bar().1.value_at(0), Some(&Clickable::Launcher));
    }
}

#[test]
fn with_no_limit_to_the_line_the_tray_follows_the_entries() {
    let state = tray_bar(
        &tray_with("clock session", &[("auto_width", "false")]),
        10,
        utc(9, 5, 0),
    );
    assert_eq!(shown(&state), "[a]|b~09:05~work");
}

#[test]
fn a_tray_alone_on_the_bar_is_drawn_and_takes_the_mouse() {
    let mut state = tray_bar(&tray_with("session", &[]), 20, utc(12, 0, 0));
    state.update(panes(20, vec![]));
    assert!(state.entries.is_empty());
    assert!(!state.draws_blank_line());
    assert_eq!(shown(&state), format!("{}work", " ".repeat(16)));
    let calls = after_mouse(&mut state, Mouse::LeftClick(0, 17));
    assert_eq!(
        calls,
        vec![Call::RunAction {
            action: launch_or_focus_floating(SESSION_MANAGER_ALIAS),
            context: BTreeMap::new(),
        }]
    );
}

#[test]
fn unknown_tray_items_are_skipped_and_an_empty_tray_draws_nothing() {
    let state = tray_with("weather clock clock", &[]);
    let state = tray_bar(&state, 20, utc(12, 0, 0));
    assert_eq!(state.tray, [TrayItem::Clock]);
    assert_eq!(shown(&state), format!("[a]|b{}~12:00", " ".repeat(9)));
    for tray in ["", "weather"] {
        let state = tray_bar(&tray_with(tray, &[]), 20, utc(12, 0, 0));
        assert_eq!(shown(&state), "[a]|b", "{tray:?}");
        assert!(state.tray_is_empty());
    }
}

#[test]
fn the_session_item_follows_zellij_s_hide_session_name_unless_show_session_is_set() {
    let mut hidden = mode_of_work();
    hidden.style.hide_session_name = true;
    let drawn = |pairs: &[(&'static str, &'static str)], mode_info: &ModeInfo| {
        let mut state = tray_bar(&tray_with("session", pairs), 20, utc(12, 0, 0));
        state.update(Event::ModeUpdate(mode_info.clone()));
        shown(&state)
    };
    assert!(drawn(&[], &mode_of_work()).ends_with("work"));
    assert_eq!(drawn(&[], &hidden), "[a]|b");
    assert!(drawn(&[("show_session", "true")], &hidden).ends_with("work"));
    assert_eq!(
        drawn(&[("show_session", "false")], &mode_of_work()),
        "[a]|b"
    );
    // Renaming the session shows the name being typed.
    let mut renaming = hidden.clone();
    renaming.mode = InputMode::RenameSession;
    assert!(drawn(&[], &renaming).ends_with("work"));
    // An empty name, or none yet, draws no item.
    let mut nameless = mode();
    nameless.session_name = Some(String::new());
    assert_eq!(drawn(&[], &nameless), "[a]|b");
}

#[test]
fn the_tray_s_names_are_drawn_with_their_control_characters_escaped() {
    let mut state = tray_bar(
        &tray_with("session clock", &[("clock_format", "%H\u{1b}[31m")]),
        40,
        utc(12, 0, 0),
    );
    let mut mode_info = mode_of_work();
    mode_info.session_name = Some("w\u{7}{x}#[bold]".to_string());
    state.update(Event::ModeUpdate(mode_info));
    let line = state.draw_bar().0;
    assert!(!line.contains('\u{7}'), "{line:?}");
    assert!(line.contains("{x}#[bold]"), "{line:?}");
    assert!(!line.contains("12\u{1b}[31m"), "{line:?}");
}

#[test]
fn the_mode_item_is_drawn_from_the_per_mode_formats_and_labels() {
    let options = tray_with(
        "mode",
        &[
            ("mode_format_locked", "<{desc}>"),
            ("mode_label_pane", "P"),
            ("mode_key_tab", "T"),
            ("mode_format_tab", "{key}{desc}"),
        ],
    );
    for (input_mode, drawn) in [
        (InputMode::Normal, "NORMAL"),
        (InputMode::Locked, "<LOCKED>"),
        (InputMode::Pane, "P"),
        (InputMode::Tab, "TTAB"),
    ] {
        let mut state = tray_bar(&options, 20, utc(12, 0, 0));
        let mut mode_info = mode_of_work();
        mode_info.mode = input_mode;
        state.update(Event::ModeUpdate(mode_info));
        assert!(
            shown(&state).ends_with(&format!("~{drawn}")),
            "{input_mode:?}: {:?}",
            shown(&state)
        );
    }
    // In Zellij's own look, the mode is a block of its own.
    let state = tray_bar(
        &plain_with(&[("tray", "mode"), ("clock_utc_offset", "+00:00")]),
        30,
        utc(12, 0, 0),
    );
    assert!(shown(&state).trim_end().ends_with("NORMAL"));
    assert_ne!(shown(&state), state.draw_bar().0);
}

#[test]
fn the_unset_tray_formats_draw_in_zellij_s_colors() {
    let state = tray_bar(
        &plain_with(&[("tray", "session clock"), ("clock_utc_offset", "+00:00")]),
        40,
        utc(12, 0, 0),
    );
    let line = state.draw_bar().0;
    assert!(strip_ansi(&line).ends_with(" work   12:00 "), "{line:?}");
    // The session's name on the selected ribbon, the time on the other.
    let selected = palette().ribbon_selected.background;
    let unselected = palette().ribbon_unselected.background;
    for color in [selected, unselected] {
        let PaletteColor::Rgb((red, green, blue)) = color else {
            unreachable!()
        };
        assert!(
            line.contains(&format!("48;2;{red};{green};{blue}")),
            "{line:?}"
        );
    }
}

// The tray and the mouse.

#[test]
fn a_left_click_on_the_session_opens_the_session_manager_and_on_the_mode_the_settings() {
    let mut state = tray_bar(&tray_with("mode session clock", &[]), 40, utc(12, 0, 0));
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 31)),
        vec![Call::RunAction {
            action: launch_or_focus_floating(SESSION_MANAGER_ALIAS),
            context: BTreeMap::new(),
        }]
    );
    assert!(matches!(
        after_mouse(&mut state, Mouse::LeftClick(0, 24))[..],
        [Call::OpenSettings { .. }]
    ));
    // The clock opens nothing, and nor does the spacer between items.
    assert_eq!(after_mouse(&mut state, Mouse::LeftClick(0, 37)), vec![]);
    assert_eq!(after_mouse(&mut state, Mouse::LeftClick(0, 34)), vec![]);
}

#[test]
fn a_right_click_on_any_tray_item_opens_zellij_s_bar_menu() {
    let mut state = tray_bar(&tray_with("mode session clock", &[]), 40, utc(12, 0, 0));
    for column in [24, 31, 37] {
        assert_eq!(
            after_mouse(&mut state, Mouse::RightClick(0, column)),
            vec![Call::OpenContextMenu {
                target: ContextMenuTarget::Bar,
                line: 0,
                column,
            }],
            "{column}"
        );
    }
}

// The clock.

/// The context the clock's command is run with.
fn clock_context() -> BTreeMap<String, String> {
    BTreeMap::from([("zjtaskbar".to_string(), "clock_utc_offset".to_string())])
}

/// The call that runs the clock's command.
fn clock_command() -> Call {
    Call::RunCommand {
        command: vec!["date".to_string(), "+%z".to_string()],
        context: clock_context(),
    }
}

/// The answer of the clock's command, printing `stdout` and ending with
/// `exit_code`.
fn clock_answer(exit_code: Option<i32>, stdout: &str) -> Event {
    Event::RunCommandResult(
        exit_code,
        stdout.as_bytes().to_vec(),
        vec![],
        clock_context(),
    )
}

/// A bar with the clock alone in its tray and `pairs`, with no offset set
/// unless `pairs` sets one.
fn clock_bar(pairs: &[(&'static str, &'static str)], now: SystemTime) -> State {
    let mut options = tray_with("clock", &[("clock_utc_offset", "")]);
    options.extend_from_slice(pairs);
    tray_bar(&options, 40, now)
}

fn granted() -> Event {
    Event::PermissionRequestResult(PermissionStatus::Granted)
}

/// The timers `calls` arms.
fn timers(calls: &[Call]) -> Vec<f64> {
    calls
        .iter()
        .filter_map(|call| match call {
            Call::SetTimeout(seconds) => Some(*seconds),
            _ => None,
        })
        .collect()
}

#[test]
fn the_system_s_offset_is_asked_once_permitted_and_the_clock_waits_for_it() {
    let mut state = clock_bar(&[], utc(9, 41, 27));
    assert!(state.features().contains(&Feature::ClockCommand));
    // Nothing is run before the permission, and nothing drawn.
    assert!(!state.host.calls().contains(&clock_command()));
    assert_eq!(shown(&state), "[a]|b");
    state.host.clear();
    state.update(granted());
    assert!(state.host.calls().contains(&clock_command()));
    assert_eq!(shown(&state), "[a]|b");
    // The answer redraws, at the offset it printed.
    assert!(state.update(clock_answer(Some(0), "+0200\n")));
    assert!(shown(&state).ends_with("~11:41"), "{:?}", shown(&state));
    assert_eq!(state.clock.system_offset, clock::UtcOffset::parse("+02:00"));
}

#[test]
fn a_denied_permission_leaves_the_clock_in_utc() {
    let mut state = clock_bar(&[], utc(9, 41, 27));
    state.host.clear();
    assert!(state.update(Event::PermissionRequestResult(PermissionStatus::Denied)));
    assert!(!state.host.calls().contains(&clock_command()));
    assert!(shown(&state).ends_with("~09:41"));
}

#[test]
fn an_answer_that_does_not_parse_keeps_the_offset_known_or_utc() {
    let mut state = clock_bar(&[], utc(9, 41, 27));
    state.update(granted());
    // A failure with none known yet: UTC.
    state.update(clock_answer(Some(1), "+0200"));
    assert!(shown(&state).ends_with("~09:41"));
    state.update(clock_answer(Some(0), "-0100"));
    assert!(shown(&state).ends_with("~08:41"));
    // With one known, every kind of bad answer keeps it.
    for answer in [
        clock_answer(Some(0), "CEST"),
        clock_answer(Some(0), ""),
        clock_answer(Some(2), ""),
        clock_answer(None, "+0300"),
        Event::RunCommandResult(Some(0), vec![0xff, 0xfe], vec![], clock_context()),
    ] {
        state.update(answer);
        assert!(shown(&state).ends_with("~08:41"));
    }
    // Another command's answer is not the clock's.
    state.update(Event::RunCommandResult(
        Some(0),
        b"+0500".to_vec(),
        vec![],
        BTreeMap::new(),
    ));
    assert!(shown(&state).ends_with("~08:41"));
}

#[test]
fn an_offset_the_user_sets_is_used_and_no_command_is_run() {
    for (offset, drawn) in [("+05:30", "~15:11"), ("-0800", "~01:41")] {
        let mut state = clock_bar(&[("clock_utc_offset", offset)], utc(9, 41, 27));
        assert!(!state.features().contains(&Feature::ClockCommand));
        assert!(shown(&state).ends_with(drawn), "{offset}");
        state.update(granted());
        assert!(!state.host.calls().contains(&clock_command()), "{offset}");
    }
    // One that does not parse is ignored, and the clock keeps UTC.
    for offset in ["Europe/Paris", "+2", "+25:00"] {
        let mut state = clock_bar(&[("clock_utc_offset", offset)], utc(9, 41, 27));
        assert!(!state.features().contains(&Feature::ClockCommand));
        state.update(granted());
        assert!(!state.host.calls().contains(&clock_command()), "{offset}");
        assert!(shown(&state).ends_with("~09:41"), "{offset}");
    }
}

#[test]
fn a_grant_from_before_the_clock_needed_its_command_runs_none() {
    let mut state = clock_bar(&[("clock_utc_offset", "+01:00")], utc(9, 41, 27));
    state.update(granted());
    state.bar.own_url = Some("zjtaskbar".to_string());
    state.host.clear();
    // The offset is taken out on the settings screen, so the clock is to ask
    // the system, which the grant the bar holds does not allow.
    preview(
        &mut state,
        "zjtaskbar",
        &[("clock_utc_offset", Some("+01:00"), None)],
    );
    let asked_to_run_commands = state.host.calls().iter().any(|call| {
        matches!(call, Call::RequestPermission(asked) if asked.contains(&PermissionType::RunCommands))
    });
    assert!(asked_to_run_commands, "{:?}", state.host.calls());
    assert!(!state.host.calls().contains(&clock_command()));
    // The clock waits for the answer, drawing nothing rather than UTC.
    assert_eq!(shown(&state), "[a]|b");

    state.host.clear();
    state.update(granted());
    assert!(state.host.calls().contains(&clock_command()));
}

#[test]
fn the_offset_is_asked_again_in_each_new_hour() {
    let mut state = clock_bar(&[], utc(9, 41, 27));
    state.update(granted());
    state.update(clock_answer(Some(0), "+0100"));
    state.host.clear();
    state.host.set_system_time(utc(9, 59, 0));
    state.update(Event::Timer(60.0));
    assert!(!state.host.calls().contains(&clock_command()));
    state.host.set_system_time(utc(10, 0, 0));
    state.update(Event::Timer(60.0));
    assert!(state.host.calls().contains(&clock_command()));
    state.host.clear();
    state.host.set_system_time(utc(10, 1, 0));
    state.update(Event::Timer(60.0));
    assert!(!state.host.calls().contains(&clock_command()));
}

#[test]
fn the_clock_s_timer_is_armed_once_for_the_next_minute_and_again_only_when_it_fires() {
    let mut state = clock_bar(&[("clock_utc_offset", "+00:00")], utc(9, 41, 27));
    let armed = timers(&state.host.calls());
    assert_eq!(armed.len(), 1);
    assert!((armed[0] - 33.01).abs() < 1e-6, "{armed:?}");
    // Nothing else the bar is sent arms another.
    state.host.clear();
    state.update(Event::ModeUpdate(mode()));
    state.update(panes(40, vec![(0, vec![floating(1, "a")])]));
    state.update(granted());
    let other_format = cfg(&tray_with("clock", &[("clock_format", "%H.%M")]));
    BarCore::follow(&mut state, &BAR, Handled::Reconfigure(other_format));
    state.follow_clock();
    assert!(timers(&state.host.calls()).is_empty());
    // Firing arms the next.
    state
        .host
        .set_system_time(utc(9, 42, 0) + Duration::from_millis(10));
    state.update(Event::Timer(33.01));
    let armed = timers(&state.host.calls());
    assert_eq!(armed.len(), 1);
    assert!((armed[0] - 60.0).abs() < 1e-6, "{armed:?}");
}

#[test]
fn seconds_added_to_the_format_tick_from_the_next_second() {
    let mut state = clock_bar(&[("clock_utc_offset", "+00:00")], utc(9, 41, 27));
    state.host.clear();
    let with_seconds = cfg(&tray_with("clock", &[("clock_format", "%H:%M:%S")]));
    BarCore::follow(&mut state, &BAR, Handled::Reconfigure(with_seconds));
    state.follow_clock();
    let armed = timers(&state.host.calls());
    assert_eq!(armed.len(), 1);
    assert!((armed[0] - 1.01).abs() < 1e-6, "{armed:?}");
    // Following again before it fires arms no more.
    state.host.clear();
    state.follow_clock();
    assert!(timers(&state.host.calls()).is_empty());
}

/// Fire the timers `state` arms, as Zellij would, earliest first, with the
/// system time at each one's moment, until `until`. `due` holds those armed
/// before the call and keeps those still waiting after it. Returns how many
/// timers were waiting after each fire.
fn fire_timers_until(
    state: &mut State,
    due: &mut Vec<SystemTime>,
    until: SystemTime,
) -> Vec<usize> {
    let mut waiting = Vec::new();
    while let Some(index) = (0..due.len()).min_by_key(|index| due[*index]) {
        if due[index] > until {
            break;
        }
        let now = due.remove(index);
        state.host.set_system_time(now);
        state.host.clear();
        state.update(Event::Timer(0.0));
        due.extend(
            timers(&state.host.calls())
                .into_iter()
                .map(|seconds| now + Duration::from_secs_f64(seconds)),
        );
        waiting.push(due.len());
    }
    waiting
}

#[test]
fn timers_armed_for_a_format_that_changed_never_multiply() {
    let start = utc(9, 41, 27);
    let mut state = clock_bar(&[("clock_utc_offset", "+00:00")], start);
    let mut due: Vec<SystemTime> = timers(&state.host.calls())
        .into_iter()
        .map(|seconds| start + Duration::from_secs_f64(seconds))
        .collect();
    // Seconds are added, beside the minute's timer still waiting, then
    // taken out and added again in the same second.
    state.host.clear();
    for format in ["%H:%M:%S", "%H:%M", "%H:%M:%S"] {
        let options = cfg(&tray_with("clock", &[("clock_format", format)]));
        BarCore::follow(&mut state, &BAR, Handled::Reconfigure(options));
        state.follow_clock();
    }
    due.extend(
        timers(&state.host.calls())
            .into_iter()
            .map(|seconds| start + Duration::from_secs_f64(seconds)),
    );
    assert_eq!(due.len(), 2, "{due:?}");
    let waiting = fire_timers_until(&mut state, &mut due, utc(9, 44, 0));
    // Two timers wait, the second's and the minute's, until the minute's
    // fires at 09:42, and one ever after.
    let minute_fired = waiting.iter().position(|count| *count == 1).unwrap();
    assert!(
        waiting[..minute_fired].iter().all(|count| *count == 2),
        "{waiting:?}"
    );
    assert!(
        waiting[minute_fired..].iter().all(|count| *count == 1),
        "{waiting:?}"
    );
    // A tick each second from 09:41:28 to 09:43:59, the minute's timer
    // standing for the one at 09:42:00.
    assert_eq!(waiting.len(), 32 + 60 + 60);
    assert!(shown(&state).ends_with("~09:43:59"), "{:?}", shown(&state));

    // Taken out again, the clock goes back to a tick a minute, the second's
    // timer waiting standing for the one at 09:44.
    let options = cfg(&tray_with("clock", &[("clock_format", "%H:%M")]));
    BarCore::follow(&mut state, &BAR, Handled::Reconfigure(options));
    state.follow_clock();
    let waiting = fire_timers_until(&mut state, &mut due, utc(9, 50, 0));
    assert!(waiting.iter().all(|count| *count == 1), "{waiting:?}");
    assert_eq!(waiting.len(), 6);
}

#[test]
fn a_clock_taken_out_of_the_tray_is_not_armed_again() {
    let mut state = clock_bar(&[("clock_utc_offset", "+00:00")], utc(9, 41, 27));
    let without = cfg(&tray_with("session", &[]));
    BarCore::follow(&mut state, &BAR, Handled::Reconfigure(without));
    state.host.clear();
    state.update(Event::Timer(33.01));
    assert!(timers(&state.host.calls()).is_empty());
    assert_eq!(state.clock.text, None);
    // Put back, it is armed again at once, as no timer is waiting.
    let with = cfg(&tray_with("clock", &[]));
    BarCore::follow(&mut state, &BAR, Handled::Reconfigure(with));
    state.follow_clock();
    assert_eq!(timers(&state.host.calls()).len(), 1);
}

#[test]
fn a_tick_redraws_only_when_the_drawn_time_changes() {
    let mut state = clock_bar(&[("clock_utc_offset", "+00:00")], utc(9, 41, 27));
    // Early, still in the same minute.
    state.host.set_system_time(utc(9, 41, 59));
    assert!(!state.update(Event::Timer(33.01)));
    state.host.set_system_time(utc(9, 42, 0));
    assert!(state.update(Event::Timer(1.0)));
    assert!(shown(&state).ends_with("~09:42"));
    // With seconds shown, every second draws differently.
    let mut state = clock_bar(
        &[("clock_utc_offset", "+00:00"), ("clock_format", "%M:%S")],
        utc(9, 41, 27),
    );
    state.host.set_system_time(utc(9, 41, 28));
    assert!(state.update(Event::Timer(1.0)));
    // A date alone ticks each minute and redraws only when the day changes.
    let mut state = clock_bar(
        &[("clock_utc_offset", "+00:00"), ("clock_format", "%a %e")],
        utc(23, 59, 0),
    );
    assert!(shown(&state).ends_with("~Tue  3"));
    state.host.set_system_time(utc(23, 59, 30));
    assert!(!state.update(Event::Timer(60.0)));
    state.host.set_system_time(utc(24, 0, 0));
    assert!(state.update(Event::Timer(60.0)));
    assert!(shown(&state).ends_with("~Wed  4"));
}

// The tray and redrawing.

#[test]
fn a_mode_or_session_change_redraws_only_when_that_item_is_in_the_tray() {
    let new_mode = || {
        let mut mode_info = mode_of_work();
        mode_info.mode = InputMode::Locked;
        Event::ModeUpdate(mode_info)
    };
    let new_session = || {
        let mut mode_info = mode_of_work();
        mode_info.session_name = Some("play".to_string());
        Event::ModeUpdate(mode_info)
    };
    for (tray, redraws_for_mode, redraws_for_session) in [
        ("mode", true, false),
        ("session", false, true),
        ("mode session", true, true),
        ("clock", false, false),
        ("", false, false),
    ] {
        let mut state = tray_bar(&tray_with(tray, &[]), 40, utc(12, 0, 0));
        assert_eq!(state.update(new_mode()), redraws_for_mode, "{tray:?}");
        let mut state = tray_bar(&tray_with(tray, &[]), 40, utc(12, 0, 0));
        assert_eq!(state.update(new_session()), redraws_for_session, "{tray:?}");
    }
    // A session name the tray hides changes nothing on screen either.
    let mut state = tray_bar(
        &tray_with("session", &[("show_session", "false")]),
        40,
        utc(12, 0, 0),
    );
    assert!(!state.update(new_session()));
    // Nor does an empty name after none, as the tray shows neither.
    let mut state = tray_bar(&tray_with("session", &[]), 40, utc(12, 0, 0));
    let mut nameless = mode();
    assert!(state.update(Event::ModeUpdate(nameless.clone())));
    nameless.session_name = Some(String::new());
    assert!(!state.update(Event::ModeUpdate(nameless)));
}

// The tray in the docs images.

#[test]
fn an_image_shows_the_sample_time() {
    let options = tray_with("clock", &[("clock_utc_offset", "")]);
    let Event::PaneUpdate(manifest) = panes(30, vec![(0, vec![floating(1, "a")])]) else {
        unreachable!()
    };
    let line = draw_for_image(cfg(&options), mode(), &tabs(1, 0), manifest, 30);
    assert!(strip_ansi(&line).ends_with("~09:41"), "{line:?}");
}

// Frame buttons.

/// The buttons a frame gets by default, with the restore glyph in place of
/// the maximize one while `fullscreen`.
fn default_frame_items(fullscreen: bool) -> Vec<FrameItem> {
    let maximize = if fullscreen {
        DEFAULT_FRAME_RESTORE_TEXT
    } else {
        DEFAULT_FRAME_MAXIMIZE_TEXT
    };
    vec![
        FrameItem::new("minimize", DEFAULT_FRAME_MINIMIZE_TEXT).clickable(),
        FrameItem::new("maximize", maximize).clickable(),
        FrameItem::new("close", DEFAULT_FRAME_CLOSE_TEXT).clickable(),
    ]
}

/// The frame items `calls` places, in order.
fn frame_items(calls: &[Call]) -> Vec<(PaneId, Vec<FrameItem>)> {
    calls
        .iter()
        .filter_map(|call| match call {
            Call::SetPaneFrameItems(pane, items) => Some((*pane, items.clone())),
            _ => None,
        })
        .collect()
}

/// What `event` makes the bar ask of Zellij.
fn after_event(state: &mut State, event: Event) -> Vec<Call> {
    state.host.clear();
    state.update(event);
    state.host.calls()
}

/// A click with `button` on the frame button `item_id` of `pane`.
fn frame_click(pane: PaneId, item_id: &str, button: MouseButton) -> Event {
    Event::FrameItemClicked(FrameItemClick {
        pane_id: pane,
        item_id: item_id.to_string(),
        button,
        modifiers: Default::default(),
    })
}

/// The URL the bars in these tests run.
const BAR_URL: &str = "zjtaskbar";

/// The pane of another bar of the instance, with plugin id `id`, as it
/// stands once it has its permissions: tiled and taking no focus. A test of
/// one `State` tells it of the others with `join_other_bars`.
fn other_bar(id: u32) -> PaneInfo {
    PaneInfo {
        id,
        is_plugin: true,
        plugin_url: Some(BAR_URL.to_string()),
        ..Default::default()
    }
}

/// Tell `state` that every pane of `by_tab` running `BAR_URL` is another
/// bar of its instance, as those bars' slots would once loaded.
fn join_other_bars(state: &State, by_tab: &[(usize, Vec<PaneInfo>)]) {
    let bars = by_tab
        .iter()
        .flat_map(|(_, panes)| panes)
        .filter(|pane| pane.plugin_url.as_deref() == Some(BAR_URL))
        .map(|pane| pane.id);
    state.instance.get_mut().bars.extend(bars);
}

/// A tiled terminal pane, `id`, titled `title`.
fn tiled(id: u32, title: &str) -> PaneInfo {
    let mut pane = floating(id, title);
    pane.is_floating = false;
    pane
}

/// A bar with `pairs` for its configuration, given `by_tab` for the panes,
/// with the other bars there, and its permissions.
fn granted_bar(
    pairs: &[(&str, &str)],
    tab_count: usize,
    by_tab: Vec<(usize, Vec<PaneInfo>)>,
) -> State {
    let mut state = bar(pairs, 80, tabs(tab_count, 0), by_tab.clone());
    join_other_bars(&state, &by_tab);
    state.update(granted());
    state
}

/// Two floating panes in the bar's tab, one filling it, and one in a
/// second tab with a bar of its own, with the bar's permissions granted.
fn framed_bar(pairs: &[(&str, &str)]) -> State {
    let mut fullscreen = floating(2, "htop");
    fullscreen.is_fullscreen = true;
    granted_bar(
        pairs,
        2,
        vec![
            (0, vec![floating(1, "vim"), fullscreen]),
            (1, vec![other_bar(50), floating(3, "far")]),
        ],
    )
}

#[test]
fn frame_buttons_go_on_every_pane_of_the_bar_s_tab_and_of_tabs_without_a_bar_once_granted() {
    let by_tab = vec![
        (
            0,
            vec![floating(1, "vim"), tiled(4, "shell"), tiled(5, "logs")],
        ),
        (
            1,
            vec![other_bar(50), floating(3, "far"), tiled(8, "far shell")],
        ),
        (2, vec![tiled(6, "alone"), floating(7, "top")]),
    ];
    let mut state = bar(&PLAIN, 80, tabs(3, 0), by_tab.clone());
    join_other_bars(&state, &by_tab);
    state.update(Event::PaneUpdate(state.panes.clone()));
    assert_eq!(frame_items(&state.host.calls()), vec![]);
    // The second tab's panes are its own bar's to frame. The third tab has
    // no bar, so this one, the first, frames it. Its only tiled pane has no
    // minimize button, as Zellij will not float it.
    let without_minimize = default_frame_items(false)[1..].to_vec();
    assert_eq!(
        frame_items(&after_event(&mut state, granted())),
        vec![
            (PaneId::Terminal(1), default_frame_items(false)),
            (PaneId::Terminal(4), default_frame_items(false)),
            (PaneId::Terminal(5), default_frame_items(false)),
            (PaneId::Terminal(6), without_minimize),
            (PaneId::Terminal(7), default_frame_items(false)),
        ]
    );
}

#[test]
fn a_tab_without_a_bar_is_framed_by_the_first_bar_only() {
    let state = granted_bar(
        &PLAIN,
        3,
        vec![
            (1, vec![other_bar(0)]),
            (2, vec![tiled(6, "alone"), floating(7, "top")]),
        ],
    );
    assert_eq!(frame_items(&state.host.calls()), vec![]);
}

#[test]
fn listing_only_the_active_tab_the_first_bar_minimizes_nothing_in_a_tab_without_a_bar() {
    let state = granted_bar(
        &[PLAIN.as_slice(), &[("pane_scope", "tab")]].concat(),
        2,
        vec![(1, vec![floating(7, "top")])],
    );
    assert_eq!(
        frame_items(&state.host.calls()),
        vec![(
            PaneId::Terminal(7),
            default_frame_items(false)[1..].to_vec()
        )]
    );
}

#[test]
fn minimizing_a_tiled_pane_floats_it_and_restoring_lets_zellij_place_it() {
    let mut state = granted_bar(
        &PLAIN,
        1,
        vec![(0, vec![tiled(4, "shell"), tiled(5, "logs")])],
    );
    let shell = PaneId::Terminal(4);
    assert_eq!(
        after_event(
            &mut state,
            frame_click(shell, "minimize", MouseButton::Left)
        ),
        vec![
            Call::TogglePaneEmbedOrEjectForPaneId(shell),
            Call::HidePaneWithId(shell),
        ]
    );
    // Zellij reports a hidden floating pane as it does a hidden tiled one.
    state.update(panes(
        80,
        vec![(0, vec![suppressed(tiled(4, "shell")), tiled(5, "logs")])],
    ));
    assert_eq!(shown(&state), "_shell_");
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 0)),
        vec![focus_terminal(4)]
    );
}

#[test]
fn the_close_button_closes_a_tiled_pane_the_bar_does_not_list() {
    let mut state = granted_bar(
        &PLAIN,
        1,
        vec![(0, vec![tiled(4, "shell"), tiled(5, "logs")])],
    );
    let (request_id, request) = the_prompt(&after_event(
        &mut state,
        frame_click(PaneId::Terminal(4), "close", MouseButton::Left),
    ));
    assert_eq!(
        request,
        close_prompt("Close pane \"shell\" and end what runs in it?")
    );
    assert_eq!(
        after_answer(&mut state, request_id, PromptResult::Confirmed(true)),
        vec![Call::CloseTerminalPane(4)]
    );
}

#[test]
fn a_pane_filling_its_tab_gets_the_restore_button() {
    let state = framed_bar(&PLAIN);
    assert_eq!(
        frame_items(&state.host.calls()),
        vec![
            (PaneId::Terminal(1), default_frame_items(false)),
            (PaneId::Terminal(2), default_frame_items(true)),
        ]
    );
}

#[test]
fn frame_buttons_are_sent_again_only_when_they_change() {
    let mut state = framed_bar(&PLAIN);
    let same = panes(
        80,
        vec![
            (
                0,
                vec![floating(1, "vim"), {
                    let mut pane = floating(2, "htop");
                    pane.is_fullscreen = true;
                    pane
                }],
            ),
            (1, vec![other_bar(50), floating(3, "far")]),
        ],
    );
    assert_eq!(frame_items(&after_event(&mut state, same)), vec![]);
    // htop gives the tab back and moves to the second tab, where this bar
    // takes its buttons off; vim's stay as they are.
    let moved = panes(
        80,
        vec![
            (0, vec![floating(1, "vim")]),
            (
                1,
                vec![other_bar(50), floating(2, "htop"), floating(3, "far")],
            ),
        ],
    );
    assert_eq!(
        frame_items(&after_event(&mut state, moved)),
        vec![(PaneId::Terminal(2), vec![])]
    );
}

#[test]
fn turning_frame_buttons_off_takes_them_off_and_an_empty_text_leaves_one_off() {
    let mut state = framed_bar(&PLAIN);
    state.host.clear();
    BarCore::follow(
        &mut state,
        &BAR,
        Handled::Reconfigure(cfg(
            &[PLAIN.as_slice(), &[("frame_buttons", "false")]].concat()
        )),
    );
    state.sync_frame_buttons();
    assert_eq!(
        frame_items(&state.host.calls()),
        vec![(PaneId::Terminal(1), vec![]), (PaneId::Terminal(2), vec![])]
    );
    let state = framed_bar(&[PLAIN.as_slice(), &[("frame_buttons", "false")]].concat());
    assert_eq!(frame_items(&state.host.calls()), vec![]);
    let state = framed_bar(
        &[
            PLAIN.as_slice(),
            &[("frame_minimize_text", ""), ("frame_close_text", "x")],
        ]
        .concat(),
    );
    assert_eq!(
        frame_items(&state.host.calls())[0],
        (
            PaneId::Terminal(1),
            vec![
                FrameItem::new("maximize", DEFAULT_FRAME_MAXIMIZE_TEXT).clickable(),
                FrameItem::new("close", "x").clickable(),
            ]
        )
    );
}

#[test]
fn the_frame_buttons_minimize_maximize_and_close_their_pane() {
    let mut state = framed_bar(&PLAIN);
    let vim = PaneId::Terminal(1);
    assert_eq!(
        after_event(&mut state, frame_click(vim, "maximize", MouseButton::Left)),
        vec![Call::TogglePaneIdFullscreen(vim)]
    );
    let (request_id, request) = the_prompt(&after_event(
        &mut state,
        frame_click(vim, "close", MouseButton::Left),
    ));
    assert_eq!(
        request,
        close_prompt("Close pane \"vim\" and end what runs in it?")
    );
    assert_eq!(
        after_answer(&mut state, request_id, PromptResult::Confirmed(true)),
        vec![Call::CloseTerminalPane(1)]
    );
    assert_eq!(
        after_event(&mut state, frame_click(vim, "minimize", MouseButton::Left)),
        vec![Call::HidePaneWithId(vim)]
    );
    assert!(state.minimized().contains_key(&vim));
}

#[test]
fn a_minimized_pane_keeps_its_buttons_and_the_close_button_needs_no_mouse() {
    let mut state = framed_bar(&[PLAIN.as_slice(), &[("mouse", "false")]].concat());
    let vim = PaneId::Terminal(1);
    state.update(frame_click(vim, "minimize", MouseButton::Left));
    let hidden = panes(
        80,
        vec![
            (
                0,
                vec![suppressed(floating(1, "vim")), {
                    let mut pane = floating(2, "htop");
                    pane.is_fullscreen = true;
                    pane
                }],
            ),
            (1, vec![other_bar(50), floating(3, "far")]),
        ],
    );
    assert_eq!(frame_items(&after_event(&mut state, hidden)), vec![]);
    assert_eq!(
        after_event(&mut state, frame_click(vim, "close", MouseButton::Left)).len(),
        1
    );
}

#[test]
fn only_a_left_click_on_a_button_the_bar_placed_acts() {
    let mut state = framed_bar(&PLAIN);
    for event in [
        frame_click(PaneId::Terminal(1), "close", MouseButton::Right),
        frame_click(PaneId::Terminal(1), "close", MouseButton::Middle),
        frame_click(PaneId::Terminal(1), "unknown", MouseButton::Left),
        // The other tab's bar placed this pane's buttons.
        frame_click(PaneId::Terminal(3), "close", MouseButton::Left),
    ] {
        assert_eq!(after_event(&mut state, event), vec![]);
    }
}

#[test]
fn the_close_button_s_confirmation_asks_for_its_prompt_with_the_mouse_off() {
    let state = loaded(&[("mouse", "false")]);
    assert_eq!(state.features(), vec![Feature::FrameCloseConfirm]);
    let state = loaded(&[("mouse", "false"), ("mouse_close_confirm", "false")]);
    assert_eq!(state.features(), vec![]);
}

// Pulling a pane from another tab.

#[test]
fn restoring_a_minimized_pane_of_another_tab_shows_it_in_this_one_where_it_stood() {
    let stood = placed(floating(2, "far"), (10, 5), (40, 12), false);
    let mut state = bar(
        &PLAIN,
        80,
        tabs(2, 0),
        vec![(1, vec![focused(stood.clone())])],
    );
    state.render(1, 80);
    state.minimize(PaneId::Terminal(2));
    state.update(panes(80, vec![(1, vec![suppressed(stood.clone())])]));
    assert_eq!(shown(&state), "_far(two)_");
    let far = PaneId::Terminal(2);
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 0)),
        vec![
            Call::ShowPaneWithId {
                pane_id: far,
                should_float_if_hidden: true,
                tab_id: Some(100),
            },
            Call::ChangeFloatingPanesCoordinates(vec![(far, mouse::geometry_of(&stood))]),
        ]
    );
}

#[test]
fn the_wheel_steps_to_a_pane_in_another_tab_without_pulling_it() {
    let mut state = bar(
        &PLAIN,
        80,
        tabs(2, 0),
        vec![
            (0, vec![focused(floating(1, "vim"))]),
            (1, vec![floating(2, "far")]),
        ],
    );
    let calls = after_mouse(&mut state, Mouse::ScrollDown(0));
    assert!(
        !calls
            .iter()
            .any(|call| matches!(call, Call::ShowPaneWithId { .. } | Call::HidePaneWithId(_))),
        "{calls:?}"
    );
}

// Several bars of one instance, run as Zellij runs them: each in a slot of
// `zjkit::shared::Shared`, sharing what `instance` keeps.

use zjkit::shared::{Effect, Outcome, Shared};

/// The pane of the bar in slot `id`, 80 columns wide.
fn bar_pane(id: u32) -> PaneInfo {
    let mut pane = other_bar(id);
    pane.pane_content_columns = 80;
    pane
}

fn context(slot_id: Option<u32>, client_id: Option<ClientId>) -> EventContext {
    EventContext { slot_id, client_id }
}

/// The panes of each tab, by position, as Zellij reports them to every
/// slot.
fn manifest(by_tab: Vec<Vec<PaneInfo>>) -> Event {
    Event::PaneUpdate(PaneManifest {
        panes: by_tab.into_iter().enumerate().collect(),
    })
}

/// An instance with a bar loaded with `PLAIN` and `pairs` in a slot for
/// each of `ids`, shown `by_tab` for the panes and two tabs, the one at
/// `active` on screen for client 1, and granted its permissions.
fn bars(
    ids: &[u32],
    pairs: &[(&'static str, &'static str)],
    active: usize,
    by_tab: Vec<Vec<PaneInfo>>,
) -> Shared<State> {
    // Whatever an instance of an earlier test on this thread kept.
    instance::reset();
    let mut shared = Shared::<State>::default();
    for id in ids {
        shared.add_slot(Slot {
            id: *id,
            kind: SlotKind::Pane,
            configuration: cfg(&plain_with(pairs)),
        });
    }
    shared.take_event(Event::ModeUpdate(mode()), context(None, Some(1)));
    shared.take_event(manifest(by_tab), context(None, None));
    shared.take_event(Event::TabUpdate(tabs(2, active)), context(None, Some(1)));
    shared.take_event(granted(), context(Some(ids[0]), Some(1)));
    shared
}

/// `bars`, with each bar loaded with `PLAIN` and its own pairs, by id.
fn bars_each(
    each: &[(u32, &[(&'static str, &'static str)])],
    active: usize,
    by_tab: Vec<Vec<PaneInfo>>,
) -> Shared<State> {
    instance::reset();
    let mut shared = Shared::<State>::default();
    for (id, pairs) in each {
        shared.add_slot(Slot {
            id: *id,
            kind: SlotKind::Pane,
            configuration: cfg(&plain_with(pairs)),
        });
    }
    shared.take_event(Event::ModeUpdate(mode()), context(None, Some(1)));
    shared.take_event(manifest(by_tab), context(None, None));
    shared.take_event(Event::TabUpdate(tabs(2, active)), context(None, Some(1)));
    shared.take_event(granted(), context(Some(each[0].0), Some(1)));
    shared
}

/// The frame items an `Outcome` placed, in order.
fn placed_items(outcome: &Outcome) -> Vec<(PaneId, Vec<FrameItem>)> {
    outcome
        .effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::SetPaneFrameItems(pane, items) => Some((*pane, items.clone())),
            _ => None,
        })
        .collect()
}

/// Draw slot `id` for client 1, and return the frame items placed as it
/// did.
fn draw(shared: &mut Shared<State>, id: u32) -> Vec<(PaneId, Vec<FrameItem>)> {
    placed_items(&shared.render_slot(1, 80, id, 1))
}

/// What slot `id` asks of Zellij for `event`, which client 1 sent through
/// `slot`, if any.
fn slot_after(shared: &mut Shared<State>, id: u32, slot: Option<u32>, event: Event) -> Vec<Call> {
    shared.plugin(id).unwrap().host.clear();
    shared.take_event(event, context(slot, Some(1)));
    shared.plugin(id).unwrap().host.calls()
}

/// Two tabs, each with its bar, slots 10 and 11, and `vim` floating in the
/// first.
fn two_tabs() -> Vec<Vec<PaneInfo>> {
    vec![vec![bar_pane(10), floating(1, "vim")], vec![bar_pane(11)]]
}

/// The same with `vim` pulled into the second tab.
fn vim_pulled() -> Event {
    manifest(vec![
        vec![bar_pane(10)],
        vec![bar_pane(11), floating(1, "vim")],
    ])
}

#[test]
fn each_tab_s_panes_get_the_buttons_of_that_tab_s_bar_alone() {
    let mut shared = bars(&[10, 11], &[], 0, two_tabs());
    let vim = PaneId::Terminal(1);
    assert_eq!(draw(&mut shared, 11), vec![]);
    assert_eq!(
        draw(&mut shared, 10),
        vec![(vim, default_frame_items(false))]
    );
    assert_eq!(draw(&mut shared, 11), vec![]);
}

#[test]
fn a_pane_pulled_from_a_tab_not_on_screen_keeps_one_bar_s_buttons_until_that_bar_draws() {
    let mut shared = bars(&[10, 11], &[], 0, two_tabs());
    let vim = PaneId::Terminal(1);
    draw(&mut shared, 10);
    // Client 1 goes to the second tab and pulls vim in from the first,
    // whose bar is not drawn now and so cannot take its buttons off.
    shared.take_event(Event::TabUpdate(tabs(2, 1)), context(None, Some(1)));
    shared.take_event(vim_pulled(), context(None, None));
    assert_eq!(draw(&mut shared, 11), vec![]);
    assert_eq!(draw(&mut shared, 11), vec![]);
    // The first bar's buttons still act, from that bar.
    assert_eq!(
        slot_after(
            &mut shared,
            10,
            None,
            frame_click(vim, "maximize", MouseButton::Left)
        ),
        vec![Call::TogglePaneIdFullscreen(vim)]
    );
    // Once the first tab is shown, its bar takes them off, and the second
    // bar puts its own on as it draws next.
    shared.take_event(Event::TabUpdate(tabs(2, 0)), context(None, Some(1)));
    assert_eq!(draw(&mut shared, 10), vec![(vim, vec![])]);
    shared.take_event(Event::TabUpdate(tabs(2, 1)), context(None, Some(1)));
    assert_eq!(
        draw(&mut shared, 11),
        vec![(vim, default_frame_items(false))]
    );
    // A click on them now goes to the second bar.
    assert_eq!(
        slot_after(
            &mut shared,
            11,
            None,
            frame_click(vim, "maximize", MouseButton::Left)
        ),
        vec![Call::TogglePaneIdFullscreen(vim)]
    );
    assert_eq!(draw(&mut shared, 10), vec![]);
}

#[test]
fn buttons_a_bar_never_placed_leave_the_pane_free_at_once() {
    let mut shared = bars(&[10, 11], &[], 1, two_tabs());
    let vim = PaneId::Terminal(1);
    // The first bar wanted vim's frame but was never drawn to place them.
    shared.take_event(vim_pulled(), context(None, None));
    assert_eq!(
        draw(&mut shared, 11),
        vec![(vim, default_frame_items(false))]
    );
    // What it drew later takes nothing from the second bar.
    draw(&mut shared, 10);
    assert_eq!(
        slot_after(
            &mut shared,
            11,
            None,
            frame_click(vim, "maximize", MouseButton::Left)
        ),
        vec![Call::TogglePaneIdFullscreen(vim)]
    );
}

#[test]
fn a_bar_that_closes_leaves_its_tab_s_panes_to_the_first_bar_left() {
    let mut shared = bars(&[10, 11], &[], 0, two_tabs());
    let vim = PaneId::Terminal(1);
    draw(&mut shared, 10);
    // Zellij takes the closing bar's buttons off every frame itself.
    shared.remove_slot(10);
    shared.take_event(
        manifest(vec![vec![floating(1, "vim")], vec![bar_pane(11)]]),
        context(None, None),
    );
    assert_eq!(
        draw(&mut shared, 11),
        vec![(vim, default_frame_items(false))]
    );
}

#[test]
fn a_bar_reported_gone_before_it_closes_leaves_its_panes_as_it_closes() {
    let mut shared = bars(&[10, 11], &[], 0, two_tabs());
    let vim = PaneId::Terminal(1);
    draw(&mut shared, 10);
    // Zellij reports the bar's pane gone before the slot closes, while vim
    // is still the closing bar's.
    shared.take_event(
        manifest(vec![vec![floating(1, "vim")], vec![bar_pane(11)]]),
        context(None, None),
    );
    let outcome = shared.remove_slot(10);
    assert_eq!(outcome.render, RenderResponse::Slots(vec![11]));
    assert_eq!(
        draw(&mut shared, 11),
        vec![(vim, default_frame_items(false))]
    );
}

#[test]
fn a_close_button_left_from_a_bar_that_does_not_ask_asks_while_another_bar_would() {
    let quiet: &[(&str, &str)] = &[("mouse_close_confirm", "false")];
    let mut shared = bars_each(&[(10, quiet), (11, &[])], 0, two_tabs());
    let vim = PaneId::Terminal(1);
    draw(&mut shared, 10);
    // vim goes to the second tab, whose bar asks before closing, with the
    // first bar's buttons still on its frame.
    shared.take_event(Event::TabUpdate(tabs(2, 1)), context(None, Some(1)));
    shared.take_event(vim_pulled(), context(None, None));
    let close = || frame_click(vim, "close", MouseButton::Left);
    let calls = slot_after(&mut shared, 10, None, close());
    let (_, request) = the_prompt(&calls);
    assert_eq!(
        request,
        close_prompt("Close pane \"vim\" and end what runs in it?")
    );

    // With no bar left that asks, it closes at once.
    shared.remove_slot(11);
    shared.plugin_mut(10).unwrap().pending_prompt = Default::default();
    assert_eq!(
        slot_after(&mut shared, 10, None, close()),
        vec![Call::CloseTerminalPane(1)]
    );
}

#[test]
fn a_bar_that_stops_asking_before_closing_leaves_the_close_buttons_alone() {
    let quiet: &[(&str, &str)] = &[("mouse_close_confirm", "false")];
    let mut shared = bars_each(&[(10, quiet), (11, &[])], 0, two_tabs());
    let vim = PaneId::Terminal(1);
    draw(&mut shared, 10);
    // The second bar is set not to ask either, from its settings.
    let configuration = cfg(&plain_with(quiet));
    shared
        .plugin_mut(11)
        .unwrap()
        .apply_configuration(configuration);
    assert_eq!(
        slot_after(
            &mut shared,
            10,
            None,
            frame_click(vim, "close", MouseButton::Left)
        ),
        vec![Call::CloseTerminalPane(1)]
    );
}

#[test]
fn a_pane_minimized_from_one_bar_is_listed_by_every_bar_its_scope_takes_in() {
    let mut shared = bars(&[10, 11], &[], 0, two_tabs());
    let vim = PaneId::Terminal(1);
    draw(&mut shared, 10);
    assert_eq!(
        slot_after(
            &mut shared,
            10,
            None,
            frame_click(vim, "minimize", MouseButton::Left)
        ),
        vec![Call::HidePaneWithId(vim)]
    );
    shared.take_event(
        manifest(vec![
            vec![bar_pane(10), suppressed(floating(1, "vim"))],
            vec![bar_pane(11)],
        ]),
        context(None, None),
    );
    assert_eq!(shown(shared.plugin(10).unwrap()), "_vim_");
    assert_eq!(shown(shared.plugin(11).unwrap()), "_vim_");

    // A bar listing only the tab on screen lists it only from there.
    let mut shared = bars(&[10, 11], &[("pane_scope", "tab")], 1, two_tabs());
    shared.plugin_mut(10).unwrap().minimize(vim);
    shared.take_event(
        manifest(vec![
            vec![bar_pane(10), suppressed(floating(1, "vim"))],
            vec![bar_pane(11)],
        ]),
        context(None, None),
    );
    assert_eq!(shown(shared.plugin(11).unwrap()), "");
    shared.take_event(Event::TabUpdate(tabs(2, 0)), context(None, Some(1)));
    assert_eq!(shown(shared.plugin(11).unwrap()), "_vim_");
}

#[test]
fn any_bar_restores_a_pane_another_minimized_where_it_stood() {
    let stood = placed(floating(1, "vim"), (10, 5), (40, 12), false);
    let mut shared = bars(
        &[10, 11],
        &[],
        0,
        vec![
            vec![bar_pane(10), focused(stood.clone())],
            vec![bar_pane(11)],
        ],
    );
    let vim = PaneId::Terminal(1);
    draw(&mut shared, 10);
    shared.take_event(
        frame_click(vim, "minimize", MouseButton::Left),
        context(None, Some(1)),
    );
    shared.take_event(
        manifest(vec![
            vec![bar_pane(10), suppressed(stood.clone())],
            vec![bar_pane(11)],
        ]),
        context(None, None),
    );
    // From the second tab, its bar shows vim again there, where it stood.
    shared.take_event(Event::TabUpdate(tabs(2, 1)), context(None, Some(1)));
    draw(&mut shared, 11);
    assert_eq!(shown(shared.plugin(11).unwrap()), "_vim(one)_");
    assert_eq!(
        slot_after(
            &mut shared,
            11,
            Some(11),
            Event::Mouse(Mouse::LeftClick(0, 0))
        ),
        vec![
            Call::ShowPaneWithId {
                pane_id: vim,
                should_float_if_hidden: true,
                tab_id: Some(101),
            },
            Call::ChangeFloatingPanesCoordinates(vec![(vim, mouse::geometry_of(&stood))]),
        ]
    );
    // It is no longer minimized for either bar.
    assert!(!shared.plugin(10).unwrap().minimized().contains_key(&vim));
}

#[test]
fn a_bar_never_lists_another_bar_of_its_instance() {
    let mut floating_bar = bar_pane(11);
    floating_bar.is_floating = true;
    floating_bar.is_selectable = true;
    let shared = bars(
        &[10, 11],
        &[],
        0,
        vec![vec![bar_pane(10), floating(1, "vim"), floating_bar], vec![]],
    );
    assert_eq!(shown(shared.plugin(10).unwrap()), "vim");
    assert_eq!(shown(shared.plugin(11).unwrap()), "vim");
}

#[test]
fn each_bar_s_clock_ticks_on_its_own_timer() {
    let mut shared = bars(
        &[10, 11],
        &[("tray", "clock"), ("clock_utc_offset", "+00:00")],
        0,
        two_tabs(),
    );
    for id in [10, 11] {
        shared.plugin(id).unwrap().host.clear();
    }
    // Both armed one for the next minute as they loaded, the first first,
    // so the first timer to fire is its, and it arms the next one alone.
    let armed = |shared: &Shared<State>, id| timers(&shared.plugin(id).unwrap().host.calls()).len();
    shared.take_event(Event::Timer(60.0), context(None, None));
    assert_eq!((armed(&shared, 10), armed(&shared, 11)), (1, 0));
    shared.take_event(Event::Timer(60.0), context(None, None));
    assert_eq!((armed(&shared, 10), armed(&shared, 11)), (1, 1));
}

// Anyone who can launch a plugin can add a pane with any options to a
// running instance, and a panic in one slot takes every bar down. See
// `zjkit::junk`.

#[test]
fn no_options_at_all_make_an_instance_panic() {
    zjkit::junk::shake::<State>(&BAR, 0x60d20a5b, 400, instance::reset);
}
