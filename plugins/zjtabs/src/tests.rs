use super::*;
use zjkit::bar::Handled;
use zjkit::host::Call;

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

#[test]
fn loading_the_bar_asks_for_its_permissions_and_events() {
    let state = loaded(&[]);
    let calls = state.host.calls();
    // `mouse` is on by default.
    assert!(calls.contains(&Call::RequestPermission(bar_permissions(true))));
    assert!(calls.contains(&Call::Subscribe(BAR_EVENTS.to_vec())));
    assert!(calls.contains(&Call::Subscribe(MOUSE_EVENTS.to_vec())));
}

#[test]
fn a_bar_without_mouse_asks_for_nothing_the_mouse_needs() {
    let state = loaded(&[("mouse", "false")]);
    let calls = state.host.calls();
    assert!(calls.contains(&Call::RequestPermission(BAR_PERMISSIONS.to_vec())));
    assert!(!calls.contains(&Call::Subscribe(MOUSE_EVENTS.to_vec())));
}

#[test]
fn asking_for_mouse_keeps_the_permissions_the_bar_always_needs() {
    assert_eq!(bar_permissions(false), BAR_PERMISSIONS.to_vec());
    let permissions = bar_permissions(true);
    for permission in BAR_PERMISSIONS.iter().chain(MOUSE_PERMISSIONS.iter()) {
        assert!(permissions.contains(permission), "{permission:?}");
    }
    assert!(!bar_permissions(false).contains(&PermissionType::RunActionsAsUser));
}

#[test]
fn the_settings_screen_asks_for_the_permissions_of_a_bar_with_mouse() {
    assert_eq!((settings::SCHEMA.permissions)(), bar_permissions(true));
}

#[test]
fn turning_mouse_on_later_asks_for_what_the_mouse_needs() {
    let mut state = loaded(&[("mouse", "false")]);
    state.host.clear();
    assert!(BarCore::follow(
        &mut state,
        &BAR,
        Handled::Reconfigure(cfg(&[]))
    ));
    let calls = state.host.calls();
    assert!(calls.contains(&Call::Subscribe(MOUSE_EVENTS.to_vec())));
    assert!(calls.contains(&Call::RequestPermission(bar_permissions(true))));
}

#[test]
fn unset_options_take_their_defaults() {
    let state = loaded(&[]);
    assert_eq!(state.tab_format, None);
    assert_eq!(state.active_tab_format, None);
    assert_eq!(state.tab_spacer, DEFAULT_TAB_SPACER);
    assert_eq!(
        state.flag_formats,
        [
            DEFAULT_FULLSCREEN_FORMAT,
            DEFAULT_SYNC_FORMAT,
            DEFAULT_FLOATING_FORMAT,
            DEFAULT_BELL_FORMAT,
        ]
    );
    assert_eq!(state.overflow_left_format, None);
    assert_eq!(state.overflow_right_format, None);
    assert_eq!(state.index_from_zero, DEFAULT_INDEX_FROM_ZERO);
    assert_eq!(state.rename_placeholder, DEFAULT_RENAME_PLACEHOLDER);
    assert_eq!(state.overflow_str, DEFAULT_OVERFLOW_STR);
    assert_eq!(state.auto_width, DEFAULT_AUTO_WIDTH);
    assert_eq!(state.max_length, DEFAULT_MAX_LENGTH);
    assert!(!state.wide_ambiguous);
    assert_eq!(state.hide_when_nested, DEFAULT_HIDE_WHEN_NESTED);
    assert_eq!(state.collapse_when_empty, DEFAULT_COLLAPSE_WHEN_EMPTY);
    assert_eq!(state.dim_when_unfocused, DEFAULT_DIM_WHEN_UNFOCUSED);
    assert_eq!(state.dim_strength, DEFAULT_DIM_STRENGTH);
    assert_eq!(state.mouse, DEFAULT_MOUSE);
    assert_eq!(state.mouse_wheel_reversed, DEFAULT_MOUSE_WHEEL_REVERSED);
}

#[test]
fn every_option_is_read_from_its_key() {
    let state = loaded(&[
        ("tab_format", "{name}"),
        ("active_tab_format", "[{name}]"),
        ("tab_spacer", "|"),
        ("fullscreen_format", "F"),
        ("sync_format", "S"),
        ("floating_format", "W"),
        ("bell_format", "B"),
        ("overflow_left_format", "<{count}"),
        ("overflow_right_format", "{count}>"),
        ("index_from_zero", "true"),
        ("rename_placeholder", "?"),
        ("overflow_str", "~"),
        ("auto_width", "false"),
        ("max_length", "40"),
        ("ambiguous_width", "2"),
        ("hide_when_nested", "false"),
        ("collapse_when_empty", "false"),
        ("dim_when_unfocused", "false"),
        ("dim_strength", "0.25"),
        ("mouse", "false"),
        ("mouse_wheel_reversed", "true"),
    ]);
    assert_eq!(state.tab_format.as_deref(), Some("{name}"));
    assert_eq!(state.active_tab_format.as_deref(), Some("[{name}]"));
    assert_eq!(state.tab_spacer, "|");
    assert_eq!(state.flag_formats, ["F", "S", "W", "B"]);
    assert_eq!(state.overflow_left_format.as_deref(), Some("<{count}"));
    assert_eq!(state.overflow_right_format.as_deref(), Some("{count}>"));
    assert!(state.index_from_zero);
    assert_eq!(state.rename_placeholder, "?");
    assert_eq!(state.overflow_str, "~");
    assert!(!state.auto_width);
    assert_eq!(state.max_length, 40);
    assert!(state.wide_ambiguous);
    assert!(!state.hide_when_nested);
    assert!(!state.collapse_when_empty);
    assert!(!state.dim_when_unfocused);
    assert_eq!(state.dim_strength, 0.25);
    assert!(!state.mouse);
    assert!(state.mouse_wheel_reversed);
}

#[test]
fn an_empty_format_means_zellij_s_look_but_an_empty_spacer_or_flag_means_none() {
    let state = loaded(&[
        ("format_theme", "flat"),
        ("tab_format", ""),
        ("tab_spacer", ""),
        ("sync_format", ""),
    ]);
    assert_eq!(state.tab_format, None);
    assert!(state.active_tab_format.is_some());
    assert_eq!(state.tab_spacer, "");
    assert_eq!(state.flag_formats[1], "");
}

#[test]
fn the_themes_are_laid_under_the_user_s_options() {
    let state = loaded(&[
        ("format_theme", "pills"),
        ("alias_theme", "ascii"),
        ("active_tab_format", "*{name}*"),
    ]);
    assert_eq!(state.active_tab_format.as_deref(), Some("*{name}*"));
    assert!(state
        .tab_format
        .as_deref()
        .is_some_and(|format| format.contains(zjkit::theme::PILL_LEFT)));
    assert_eq!(state.flag_formats, [" Z", " S", " F", " !"]);
    assert_ne!(state.overflow_str, DEFAULT_OVERFLOW_STR);
}

#[test]
fn hidden_hides_the_bar() {
    let state = loaded(&[("hidden", "true")]);
    assert!(state.bar.visibility.is_hidden());
}

// Drawing the tabs.

use std::collections::HashMap;
use std::ops::Range;
use tabs::Clickable;
use zellij_tile::prelude::{AttentionStatus, TabAttention};
use zjkit::ansi::{char_width, AnsiParser, AnsiSegment};
use zjkit::host::Recorder;

/// Formats with no styling, so a test can read the line as plain text.
const PLAIN: [(&str, &str); 5] = [
    ("tab_format", "{index}:{name}{flags}"),
    ("active_tab_format", "[{index}:{name}{flags}]"),
    ("tab_spacer", "|"),
    ("overflow_left_format", "<{count}"),
    ("overflow_right_format", "{count}>"),
];

fn tab(position: usize, name: &str) -> TabInfo {
    TabInfo {
        position,
        name: name.to_string(),
        // An id unlike the position, so a test can tell them apart.
        tab_id: 100 + position,
        ..Default::default()
    }
}

fn active(mut info: TabInfo) -> TabInfo {
    info.active = true;
    info
}

/// Tabs named `a`, `b`, `c` and so on, the one at `active_position` active.
fn tabs(count: usize, active_position: usize) -> Vec<TabInfo> {
    (0..count)
        .map(|position| {
            let name = ((b'a' + position as u8) as char).to_string();
            let info = tab(position, &name);
            if position == active_position {
                active(info)
            } else {
                info
            }
        })
        .collect()
}

/// A Zellij palette whose ribbons can be told apart in the output.
fn palette() -> Styling {
    let mut styling = Styling::default();
    styling.ribbon_selected.base = PaletteColor::Rgb((1, 2, 3));
    styling.ribbon_selected.background = PaletteColor::Rgb((0xa6, 0xe3, 0xa1));
    styling.ribbon_unselected.base = PaletteColor::Rgb((4, 5, 6));
    styling.ribbon_unselected.background = PaletteColor::Rgb((0x31, 0x32, 0x44));
    styling
}

fn mode(mode: InputMode) -> ModeInfo {
    let mut mode_info = ModeInfo {
        mode,
        ..Default::default()
    };
    mode_info.style.colors = palette();
    mode_info
}

/// A pane update in which the bar's own pane is `width` columns wide.
fn own_pane(width: usize) -> Event {
    let pane = PaneInfo {
        id: Recorder::PLUGIN_ID,
        is_plugin: true,
        pane_content_columns: width,
        ..Default::default()
    };
    Event::PaneUpdate(PaneManifest {
        panes: HashMap::from([(0, vec![pane])]),
    })
}

/// A bar with `pairs` for its configuration, `width` columns wide, in normal
/// mode and showing `infos`.
fn bar(pairs: &[(&str, &str)], width: usize, infos: Vec<TabInfo>) -> State {
    let mut state = loaded(pairs);
    state.update(Event::ModeUpdate(mode(InputMode::Normal)));
    state.update(own_pane(width));
    state.update(Event::TabUpdate(infos));
    state
}

/// `PLAIN` with `pairs` added.
fn plain_with(pairs: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
    let mut all = PLAIN.to_vec();
    all.extend_from_slice(pairs);
    all
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

/// The text of `line` drawn on `columns`.
fn text_at(line: &str, columns: Range<usize>, wide_ambiguous: bool) -> String {
    let mut column = 0;
    let mut text = String::new();
    for ch in strip_ansi(line).chars() {
        if columns.contains(&column) {
            text.push(ch);
        }
        column += char_width(ch, wide_ambiguous);
    }
    text
}

/// Each clickable stretch of the line the bar draws, as the text drawn on
/// it and what it stands for, from left to right.
fn clickable(state: &State) -> Vec<(String, Clickable)> {
    let (line, spans) = state.draw_bar();
    let width = zjkit::ansi::calculate_visible_length(&line, state.wide_ambiguous);
    let mut stretches: Vec<(Range<usize>, Clickable)> = vec![];
    for column in 0..width + 2 {
        let Some(span) = spans.span_at(column) else {
            continue;
        };
        if stretches.last().map(|(columns, _)| columns) != Some(&span.columns) {
            stretches.push((span.columns.clone(), span.value));
        }
    }
    stretches
        .into_iter()
        .map(|(columns, value)| (text_at(&line, columns, state.wide_ambiguous), value))
        .collect()
}

#[test]
fn every_tab_is_drawn_in_position_order() {
    let mut infos = tabs(3, 1);
    infos.reverse();
    let state = bar(&PLAIN, 80, infos);
    assert_eq!(shown(&state), "1:a|[2:b]|3:c");
}

#[test]
fn tabs_can_be_numbered_from_zero() {
    let state = bar(&plain_with(&[("index_from_zero", "true")]), 80, tabs(2, 0));
    assert_eq!(shown(&state), "[0:a]|1:b");
}

#[test]
fn each_flag_shows_in_its_own_format_in_a_fixed_order() {
    let mut info = tab(0, "a");
    info.is_fullscreen_active = true;
    info.is_sync_panes_active = true;
    info.are_floating_panes_visible = true;
    info.has_bell_notification = true;
    let state = bar(&PLAIN, 80, vec![info.clone()]);
    assert_eq!(shown(&state), "1:a (FULLSCREEN) (SYNC) (FLOATING) \u{25cf}");

    let own = plain_with(&[
        ("fullscreen_format", "F"),
        ("sync_format", ""),
        ("floating_format", "W"),
        ("bell_format", "!"),
    ]);
    let state = bar(&own, 80, vec![info]);
    assert_eq!(shown(&state), "1:aFW!");
}

#[test]
fn the_bell_flag_follows_bells_and_the_attention_a_tab_needs() {
    let flagged = |info: TabInfo| {
        let state = bar(&plain_with(&[("bell_format", "!")]), 80, vec![info]);
        shown(&state).ends_with('!')
    };
    let attention = |status: AttentionStatus, muted: bool| {
        let mut info = tab(0, "a");
        info.attention = TabAttention {
            status,
            muted,
            ..Default::default()
        };
        info
    };
    let mut flashing = tab(0, "a");
    flashing.is_flashing_bell = true;
    assert!(flagged(flashing));
    for status in [
        AttentionStatus::NeedsYou,
        AttentionStatus::Failed,
        AttentionStatus::Done,
    ] {
        assert!(flagged(attention(status, false)), "{status:?}");
        assert!(!flagged(attention(status, true)), "muted {status:?}");
    }
    for status in [
        AttentionStatus::Idle,
        AttentionStatus::Working,
        AttentionStatus::Quiet,
        AttentionStatus::NewOutput,
    ] {
        assert!(!flagged(attention(status, false)), "{status:?}");
    }
}

#[test]
fn renaming_shows_the_name_being_typed_or_a_placeholder_while_it_is_empty() {
    let mut state = bar(&PLAIN, 80, vec![active(tab(0, "")), tab(1, "")]);
    // Outside rename mode an empty name is drawn as it is.
    assert_eq!(shown(&state), "[1:]|2:");
    assert!(state.update(Event::ModeUpdate(mode(InputMode::RenameTab))));
    assert_eq!(shown(&state), "[1:Enter name...]|2:");
    state.update(Event::TabUpdate(vec![active(tab(0, "ed")), tab(1, "")]));
    assert_eq!(shown(&state), "[1:ed]|2:");

    let mut state = bar(&plain_with(&[("rename_placeholder", "?")]), 80, tabs(1, 0));
    state.update(Event::ModeUpdate(mode(InputMode::RenameTab)));
    state.update(Event::TabUpdate(vec![active(tab(0, ""))]));
    assert_eq!(shown(&state), "[1:?]");
}

#[test]
fn a_tab_name_is_drawn_as_text_and_its_control_characters_escaped() {
    let state = bar(
        &PLAIN,
        80,
        vec![active(tab(0, "#[fg=red]{index}$x\u{1b}[31m\n"))],
    );
    let (line, _) = state.draw_bar();
    assert_eq!(line, "[1:#[fg=red]{index}$x\\u{1b}[31m\\n]");
}

#[test]
fn with_no_formats_the_tabs_take_zellij_s_ribbon_colors() {
    let state = bar(&[], 80, tabs(2, 0));
    let (line, _) = state.draw_bar();
    assert_eq!(strip_ansi(&line), " 1 a   2 b ");
    // The active tab on `ribbon_selected`, the other on `ribbon_unselected`.
    let active_tab = line.find(" 1 a ").unwrap();
    let other_tab = line.find(" 2 b ").unwrap();
    let selected = line.find("48;2;166;227;161").unwrap();
    let unselected = line.find("48;2;49;50;68").unwrap();
    assert!(selected < active_tab && active_tab < unselected && unselected < other_tab);
}

#[test]
fn before_zellij_sends_its_theme_the_tabs_are_unstyled() {
    let mut state = loaded(&[]);
    state.update(own_pane(80));
    state.update(Event::TabUpdate(tabs(2, 0)));
    assert_eq!(state.draw_bar().0, " 1 a   2 b ");
}

#[test]
fn a_flag_with_styles_of_its_own_is_drawn_over_the_tab_s_style() {
    let mut info = active(tab(0, "a"));
    info.is_sync_panes_active = true;
    let state = bar(
        &[
            ("active_tab_format", "#[bold]<{flags}>"),
            ("sync_format", "#[fg=red]S"),
        ],
        80,
        vec![info],
    );
    let (line, _) = state.draw_bar();
    let bold = ansi_term::Style::new().bold();
    let expected = format!(
        "{}",
        ansi_term::ANSIStrings(&[
            bold.paint("<"),
            bold.fg(ansi_term::Colour::Red).paint("S"),
            bold.paint(">"),
        ])
    );
    assert_eq!(line, expected);
}

#[test]
fn a_styled_flag_keeps_the_tab_s_background_and_bold_under_it() {
    let mut belled = active(tab(0, "a"));
    belled.has_bell_notification = true;
    let mut other = tab(1, "b");
    other.has_bell_notification = true;
    let state = bar(
        &[
            ("format_theme", "flat"),
            ("bell_format", "#[fg=$red,bold] \u{25cf}"),
        ],
        80,
        vec![belled, other],
    );
    let configuration = &state.bar.configuration;
    assert!(configuration.contains_key("color_red"));
    // Each tab as its format would draw it with the bell's colors written
    // into the tab's own block: the same background, and for the active
    // tab the same bold.
    let drawn = |template: &str| {
        ansi_term::ANSIStrings(&zjkit::format::render_template(
            template,
            &[],
            configuration,
            0.0,
        ))
        .to_string()
    };
    let expected = [
        drawn("#[fg=$bg,bg=$accent,bold] 1 a#[fg=$red,bg=$accent,bold] \u{25cf}#[fg=$bg,bg=$accent,bold] "),
        " ".to_string(),
        drawn("#[fg=$fg,bg=$surface] 2 b#[fg=$red,bg=$surface,bold] \u{25cf}#[fg=$fg,bg=$surface] "),
    ]
    .concat();
    assert_eq!(state.draw_bar().0, expected);
}

#[test]
fn the_bar_dims_while_its_session_is_not_receiving_input() {
    let undimmed = bar(&[], 80, tabs(2, 0)).draw_bar().0;
    let dimmed_mode = || {
        let mut mode_info = mode(InputMode::Normal);
        mode_info.session_dimmed = Some(true);
        Event::ModeUpdate(mode_info)
    };
    let mut state = bar(&[], 80, tabs(2, 0));
    assert!(state.update(dimmed_mode()));
    let dimmed = state.draw_bar().0;
    assert_ne!(dimmed, undimmed);
    assert_eq!(strip_ansi(&dimmed), strip_ansi(&undimmed));

    let mut state = bar(&[("dim_when_unfocused", "false")], 80, tabs(2, 0));
    state.update(dimmed_mode());
    assert_eq!(state.draw_bar().0, undimmed);
}

#[test]
fn hidden_draws_nothing() {
    let state = bar(&plain_with(&[("hidden", "true")]), 80, tabs(2, 0));
    assert_eq!(state.draw_bar().0, "");
}

#[test]
fn a_nested_session_draws_nothing_unless_its_host_s_bars_are_covered() {
    let nested = |host_fullscreen: Option<bool>| {
        let mut mode_info = mode(InputMode::Normal);
        mode_info.session_ancestry = vec!["host".to_string()];
        mode_info.host_fullscreen = host_fullscreen;
        Event::ModeUpdate(mode_info)
    };
    let mut state = bar(&PLAIN, 80, tabs(1, 0));
    assert!(state.update(nested(None)));
    assert_eq!(shown(&state), "");
    assert!(state.update(nested(Some(true))));
    assert_eq!(shown(&state), "[1:a]");

    let mut state = bar(
        &plain_with(&[("hide_when_nested", "false")]),
        80,
        tabs(1, 0),
    );
    state.update(nested(None));
    assert_eq!(shown(&state), "[1:a]");
}

#[test]
fn an_empty_bar_gives_its_row_back_unless_told_to_keep_it() {
    let mut state = bar(&PLAIN, 80, vec![]);
    state.host.clear();
    state.render(1, 80);
    assert_eq!(state.host.calls(), vec![Call::SetCollapsed(true)]);
    state.update(Event::TabUpdate(tabs(1, 0)));
    state.host.clear();
    state.render(1, 80);
    assert_eq!(state.host.calls(), vec![Call::SetCollapsed(false)]);

    let mut state = bar(&plain_with(&[("collapse_when_empty", "false")]), 80, vec![]);
    state.host.clear();
    state.render(1, 80);
    assert_eq!(state.host.calls(), vec![]);
}

#[test]
fn tabs_that_do_not_fit_are_counted_by_markers_around_the_active_one() {
    // Each tab is 3 columns ("1:a"), the active one 5, so 19 columns take
    // three tabs and both markers.
    let state = bar(&PLAIN, 19, tabs(10, 5));
    assert_eq!(shown(&state), "<4|5:e|[6:f]|7:g|3>");
    // Two columns fewer, and the tab before the active one gives way.
    assert_eq!(shown(&bar(&PLAIN, 17, tabs(10, 5))), "<5|[6:f]|7:g|3>");
    // At the ends, only one side has a marker.
    assert_eq!(shown(&bar(&PLAIN, 17, tabs(10, 0))), "[1:a]|2:b|3:c|7>");
    assert_eq!(shown(&bar(&PLAIN, 17, tabs(10, 9))), "<7|8:h|9:i|[10:j]");
}

#[test]
fn the_markers_are_drawn_in_zellij_s_colors_with_no_format() {
    let state = bar(&[("tab_spacer", "|")], 20, tabs(10, 5));
    let line = shown(&state);
    assert!(line.starts_with(" \u{2190} +"), "{line:?}");
    assert!(line.ends_with(" \u{2192} "), "{line:?}");
}

#[test]
fn max_length_and_the_pane_width_both_limit_the_line() {
    assert_eq!(bar(&PLAIN, 17, tabs(10, 5)).length_limit(), Some(17));
    let capped = plain_with(&[("max_length", "9")]);
    assert_eq!(bar(&capped, 17, tabs(10, 5)).length_limit(), Some(9));
    let unfitted = plain_with(&[("auto_width", "false")]);
    let state = bar(&unfitted, 17, tabs(10, 5));
    assert_eq!(state.length_limit(), None);
    assert_eq!(shown(&state).matches(':').count(), 10);
}

#[test]
fn a_lone_tab_too_wide_for_the_bar_is_cut_with_overflow_str() {
    let state = bar(
        &plain_with(&[("overflow_str", "~")]),
        8,
        vec![active(tab(0, "a long name"))],
    );
    assert_eq!(shown(&state), "[1:a lo~");
    assert_eq!(
        clickable(&state),
        vec![("[1:a lo".to_string(), Clickable::Tab(0))]
    );
}

#[test]
fn an_active_tab_too_wide_with_its_markers_is_cut_alone_with_no_markers() {
    let overflow_str = plain_with(&[("overflow_str", "~")]);
    // Hidden tabs on the left only, and on both sides.
    let mut on_the_left = tabs(13, 12);
    on_the_left[12].name = "a long name".to_string();
    let mut on_both_sides = tabs(3, 1);
    on_both_sides[1].name = "a long name".to_string();
    for (infos, expected, position) in [
        (on_the_left, "[13:a l~", 12),
        (on_both_sides, "[2:a lo~", 1),
    ] {
        let state = bar(&overflow_str, 8, infos);
        assert_eq!(shown(&state), expected);
        // Only what is drawn of the tab stands for it.
        assert_eq!(
            clickable(&state),
            vec![(
                expected.trim_end_matches('~').to_string(),
                Clickable::Tab(position)
            )]
        );
    }
    // In Zellij's look, whose markers are wider.
    let mut infos = tabs(13, 12);
    infos[12].name = "editor".to_string();
    assert_eq!(shown(&bar(&[], 10, infos)), " 13 edi...");
}

#[test]
fn a_cut_line_keeps_its_styles_closed() {
    let state = bar(&[], 6, vec![active(tab(0, "a long name"))]);
    let (line, _) = state.draw_bar();
    assert_eq!(strip_ansi(&line), " 1 ...");
    assert!(line.ends_with(zjkit::ansi::ANSI_RESET));
}

#[test]
fn the_click_spans_match_the_columns_each_tab_and_marker_was_drawn_on() {
    let state = bar(&PLAIN, 19, tabs(10, 5));
    assert_eq!(
        clickable(&state),
        vec![
            ("<4".to_string(), Clickable::Marker(3)),
            ("5:e".to_string(), Clickable::Tab(4)),
            ("[6:f]".to_string(), Clickable::Tab(5)),
            ("7:g".to_string(), Clickable::Tab(6)),
            ("3>".to_string(), Clickable::Marker(7)),
        ]
    );
}

#[test]
fn the_click_spans_count_wide_characters_and_skip_styling() {
    let infos = vec![active(tab(0, "\u{65e5}\u{672c}")), tab(1, "x")];
    // Styled by a format theme, with a CJK name two columns per character.
    let state = bar(&[("format_theme", "flat")], 80, infos.clone());
    assert_eq!(
        clickable(&state),
        vec![
            (" 1 \u{65e5}\u{672c} ".to_string(), Clickable::Tab(0)),
            (" 2 x ".to_string(), Clickable::Tab(1)),
        ]
    );
    let (line, spans) = state.draw_bar();
    assert!(line.contains('\u{1b}'));
    assert_eq!(
        spans.span_at(0).map(|span| span.columns.clone()),
        Some(0..8)
    );
    // An ambiguous character counts as two columns when told to.
    let infos = vec![active(tab(0, "\u{25cf}")), tab(1, "x")];
    let wide = bar(&plain_with(&[("ambiguous_width", "2")]), 80, infos);
    let (_, spans) = wide.draw_bar();
    // "[1:" then the two columns of the name, "]", and the spacer at 6.
    assert_eq!(spans.value_at(5), Some(&Clickable::Tab(0)));
    assert_eq!(spans.value_at(6), None);
    assert_eq!(spans.value_at(7), Some(&Clickable::Tab(1)));
}

#[test]
fn every_theme_combination_draws_a_whole_bar_with_nothing_left_raw() {
    let mut info = tab(0, "a");
    info.is_fullscreen_active = true;
    info.is_sync_panes_active = true;
    info.are_floating_panes_visible = true;
    info.has_bell_notification = true;
    let mut infos = tabs(12, 6);
    infos[0] = info;
    for combination in theme::tests::every_combination() {
        // Wide enough for every tab, and narrow enough for markers.
        for width in [400, 30] {
            let state = bar(&combination, width, infos.clone());
            let (line, _) = state.draw_bar();
            assert!(!line.is_empty(), "{combination:?}");
            for raw in ["#[", "$", "{"] {
                assert!(!line.contains(raw), "{combination:?} {width}: left {raw}");
            }
        }
    }
}

// Redrawing only when something shown changed.

#[test]
fn the_same_tabs_again_do_not_redraw_but_a_change_to_what_is_shown_does() {
    let mut state = bar(&PLAIN, 80, tabs(3, 0));
    assert!(!state.update(Event::TabUpdate(tabs(3, 0))));
    // Nothing the bar shows.
    let mut unseen = tabs(3, 0);
    unseen[1].viewport_rows = 40;
    unseen[1].other_focused_clients = vec![2];
    assert!(!state.update(Event::TabUpdate(unseen)));
    assert!(state.update(Event::TabUpdate(tabs(3, 1))));
    let mut renamed = tabs(3, 1);
    renamed[2].name = "z".to_string();
    assert!(state.update(Event::TabUpdate(renamed.clone())));
    renamed[0].is_sync_panes_active = true;
    assert!(state.update(Event::TabUpdate(renamed)));
}

#[test]
fn a_mode_change_redraws_only_for_renaming_or_nesting() {
    let mut state = bar(&PLAIN, 80, tabs(1, 0));
    assert!(!state.update(Event::ModeUpdate(mode(InputMode::Pane))));
    assert!(!state.update(Event::ModeUpdate(mode(InputMode::Tab))));
    assert!(state.update(Event::ModeUpdate(mode(InputMode::RenameTab))));
    assert!(state.update(Event::ModeUpdate(mode(InputMode::Normal))));
    let mut nested = mode(InputMode::Normal);
    nested.session_ancestry = vec!["host".to_string()];
    assert!(state.update(Event::ModeUpdate(nested)));
    // A new Zellij theme redraws, as `BarCore` lays the themes again.
    let mut themed = mode(InputMode::Normal);
    themed.style.colors.ribbon_selected.base = PaletteColor::Rgb((9, 9, 9));
    assert!(state.update(Event::ModeUpdate(themed)));
}

#[test]
fn a_pane_update_redraws_only_when_the_bar_s_width_changes() {
    let mut state = bar(&PLAIN, 80, tabs(1, 0));
    assert!(!state.update(own_pane(80)));
    assert!(state.update(own_pane(60)));
}

#[test]
fn hiding_and_showing_redraw() {
    let mut state = bar(&PLAIN, 80, tabs(1, 0));
    let message = |name: &str| PipeMessage::new(PipeSource::Keybind, name, &None, &None, false);
    assert!(state.pipe(message(VISIBILITY.hide)));
    assert_eq!(shown(&state), "");
    assert!(!state.pipe(message(VISIBILITY.hide)));
    assert!(state.pipe(message(VISIBILITY.toggle)));
    assert_eq!(shown(&state), "[1:a]");
}

// The mouse.

/// What the bar asks of Zellij for `mouse`, once it has drawn itself.
fn after_mouse(state: &mut State, mouse: Mouse) -> Vec<Call> {
    state.render(1, 80);
    state.host.clear();
    assert!(!state.update(Event::Mouse(mouse)));
    state.host.calls()
}

#[test]
fn a_click_on_a_tab_switches_to_it() {
    // "1:a|[2:b]|3:c"
    let mut state = bar(&PLAIN, 80, tabs(3, 1));
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 0)),
        vec![Call::SwitchTabTo(1)]
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 12)),
        vec![Call::SwitchTabTo(3)]
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 4)),
        vec![Call::SwitchTabTo(2)]
    );
    // The spacer, past the end, and a line below the bar are not tabs.
    assert_eq!(after_mouse(&mut state, Mouse::LeftClick(0, 3)), vec![]);
    assert_eq!(after_mouse(&mut state, Mouse::LeftClick(0, 40)), vec![]);
    assert_eq!(after_mouse(&mut state, Mouse::LeftClick(1, 0)), vec![]);
}

#[test]
fn a_click_on_a_tab_numbered_from_zero_still_switches_to_it() {
    let mut state = bar(&plain_with(&[("index_from_zero", "true")]), 80, tabs(3, 1));
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 0)),
        vec![Call::SwitchTabTo(1)]
    );
}

#[test]
fn a_click_on_a_marker_switches_to_the_nearest_hidden_tab() {
    // "<4|5:e|[6:f]|7:g|3>"
    let mut state = bar(&PLAIN, 19, tabs(10, 5));
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 0)),
        vec![Call::SwitchTabTo(4)]
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 18)),
        vec![Call::SwitchTabTo(8)]
    );
}

#[test]
fn a_click_on_what_the_cut_left_of_a_tab_switches_to_it_but_not_on_overflow_str() {
    // "[2:a long nam~", the marker for the tab before it left out
    let infos = vec![tab(0, "a"), active(tab(1, "a long name"))];
    let mut state = bar(&plain_with(&[("overflow_str", "~")]), 14, infos);
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 12)),
        vec![Call::SwitchTabTo(2)]
    );
    assert_eq!(after_mouse(&mut state, Mouse::LeftClick(0, 13)), vec![]);
}

#[test]
fn the_wheel_goes_to_the_next_tab_on_scroll_up_like_zellij_s_own_bar_without_wrapping() {
    let mut state = bar(&PLAIN, 80, tabs(3, 1));
    assert_eq!(
        after_mouse(&mut state, Mouse::ScrollUp(1)),
        vec![Call::SwitchTabTo(3)]
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::ScrollDown(1)),
        vec![Call::SwitchTabTo(1)]
    );

    state.update(Event::TabUpdate(tabs(3, 2)));
    assert_eq!(after_mouse(&mut state, Mouse::ScrollUp(1)), vec![]);
    state.update(Event::TabUpdate(tabs(3, 0)));
    assert_eq!(after_mouse(&mut state, Mouse::ScrollDown(1)), vec![]);
}

#[test]
fn with_the_wheel_reversed_scroll_up_goes_to_the_previous_tab_without_wrapping() {
    let mut state = bar(
        &plain_with(&[("mouse_wheel_reversed", "true")]),
        80,
        tabs(3, 1),
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::ScrollUp(1)),
        vec![Call::SwitchTabTo(1)]
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::ScrollDown(1)),
        vec![Call::SwitchTabTo(3)]
    );

    state.update(Event::TabUpdate(tabs(3, 0)));
    assert_eq!(after_mouse(&mut state, Mouse::ScrollUp(1)), vec![]);
    state.update(Event::TabUpdate(tabs(3, 2)));
    assert_eq!(after_mouse(&mut state, Mouse::ScrollDown(1)), vec![]);
}

#[test]
fn the_wheel_direction_can_be_reversed_by_a_live_update() {
    let mut state = bar(&PLAIN, 80, tabs(3, 1));
    assert!(BarCore::follow(
        &mut state,
        &BAR,
        Handled::Reconfigure(cfg(&[("mouse_wheel_reversed", "true")]))
    ));
    assert_eq!(
        after_mouse(&mut state, Mouse::ScrollUp(1)),
        vec![Call::SwitchTabTo(1)]
    );
}

#[test]
fn the_wheel_also_steps_to_tabs_hidden_behind_a_marker() {
    let mut state = bar(&PLAIN, 19, tabs(10, 5));
    state.update(Event::TabUpdate(tabs(10, 6)));
    assert_eq!(
        after_mouse(&mut state, Mouse::ScrollUp(1)),
        vec![Call::SwitchTabTo(8)]
    );
}

#[test]
fn a_double_click_renames_the_tab_switching_to_it_first() {
    let rename = |id: u64| Call::RunAction {
        action: zellij_tile::prelude::actions::Action::StartRenameTabByTabId { id },
        context: BTreeMap::new(),
    };
    let mut state = bar(&PLAIN, 80, tabs(3, 1));
    assert_eq!(
        after_mouse(&mut state, Mouse::DoubleClick(0, 5)),
        vec![rename(101)]
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::DoubleClick(0, 12)),
        vec![Call::SwitchTabTo(3), rename(102)]
    );
    // Not on a marker or a spacer.
    assert_eq!(after_mouse(&mut state, Mouse::DoubleClick(0, 3)), vec![]);
    let mut state = bar(&PLAIN, 19, tabs(10, 5));
    assert_eq!(after_mouse(&mut state, Mouse::DoubleClick(0, 0)), vec![]);
}

#[test]
fn a_right_click_opens_the_tab_s_context_menu_or_else_the_bar_s() {
    let menu = |target: ContextMenuTarget, column: usize| Call::OpenContextMenu {
        target,
        line: 0,
        column,
    };
    // "<4|5:e|[6:f]|7:g|3>"
    let mut state = bar(&PLAIN, 19, tabs(10, 5));
    assert_eq!(
        after_mouse(&mut state, Mouse::RightClick(0, 4)),
        vec![menu(ContextMenuTarget::Tab(4), 4)]
    );
    for column in [0, 2, 30] {
        assert_eq!(
            after_mouse(&mut state, Mouse::RightClick(0, column)),
            vec![menu(ContextMenuTarget::Bar, column)]
        );
    }
}

#[test]
fn with_mouse_off_the_mouse_does_nothing() {
    let mut state = bar(&plain_with(&[("mouse", "false")]), 80, tabs(3, 1));
    for mouse in [
        Mouse::LeftClick(0, 0),
        Mouse::DoubleClick(0, 0),
        Mouse::RightClick(0, 0),
        Mouse::ScrollUp(1),
    ] {
        assert_eq!(after_mouse(&mut state, mouse), vec![]);
    }
}

#[test]
fn the_mouse_does_nothing_while_the_bar_draws_nothing() {
    let every_mouse = [
        Mouse::LeftClick(0, 0),
        Mouse::DoubleClick(0, 0),
        Mouse::RightClick(0, 0),
        Mouse::ScrollUp(1),
        Mouse::ScrollDown(1),
    ];
    // The row stays on screen, blank, rather than collapsing.
    let kept_row = plain_with(&[("collapse_when_empty", "false")]);
    let mut hidden_by_option = kept_row.clone();
    hidden_by_option.push(("hidden", "true"));
    let hidden_by_message = || {
        let mut state = bar(&kept_row, 80, tabs(3, 1));
        let hide = PipeMessage::new(PipeSource::Keybind, VISIBILITY.hide, &None, &None, false);
        assert!(state.pipe(hide));
        state
    };
    let hidden_by_nesting = || {
        let mut state = bar(&kept_row, 80, tabs(3, 1));
        let mut nested = mode(InputMode::Normal);
        nested.session_ancestry = vec!["host".to_string()];
        assert!(state.update(Event::ModeUpdate(nested)));
        state
    };
    for mut state in [
        bar(&hidden_by_option, 80, tabs(3, 1)),
        hidden_by_message(),
        hidden_by_nesting(),
        bar(&kept_row, 80, vec![]),
    ] {
        assert_eq!(shown(&state), "");
        for mouse in every_mouse {
            assert_eq!(after_mouse(&mut state, mouse), vec![], "{mouse:?}");
        }
    }
}

#[test]
fn hover_and_horizontal_scrolling_do_nothing() {
    let mut state = bar(&PLAIN, 80, tabs(3, 1));
    for mouse in [
        Mouse::Hover(0, 0),
        Mouse::Hold(0, 0),
        Mouse::Release(0, 0),
        Mouse::ScrollLeft(1),
        Mouse::ScrollRight(1),
    ] {
        assert_eq!(after_mouse(&mut state, mouse), vec![]);
    }
}
