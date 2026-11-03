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
    // `mouse` and `mouse_close_confirm` are on by default.
    assert!(calls.contains(&Call::RequestPermission(bar_permissions(true, true))));
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
fn a_bar_with_mouse_but_no_close_confirmation_asks_for_no_prompt() {
    let state = loaded(&[("mouse_close_confirm", "false")]);
    let calls = state.host.calls();
    assert!(calls.contains(&Call::RequestPermission(bar_permissions(true, false))));
    assert!(calls.contains(&Call::Subscribe(MOUSE_EVENTS.to_vec())));
}

#[test]
fn the_permission_sets_grow_with_mouse_and_then_the_close_confirmation() {
    // Without the mouse there is no middle click to confirm, so
    // `mouse_close_confirm` alone adds nothing.
    for mouse_close_confirm in [false, true] {
        assert_eq!(
            bar_permissions(false, mouse_close_confirm),
            BAR_PERMISSIONS.to_vec()
        );
    }
    let mut with_mouse = BAR_PERMISSIONS.to_vec();
    with_mouse.extend(MOUSE_PERMISSIONS);
    assert_eq!(bar_permissions(true, false), with_mouse);
    let mut with_confirmation = with_mouse.clone();
    with_confirmation.extend(CLOSE_CONFIRM_PERMISSIONS);
    assert_eq!(bar_permissions(true, true), with_confirmation);
    assert!(!with_mouse.contains(&PermissionType::OpenTerminalsOrPlugins));
    assert!(with_confirmation.contains(&PermissionType::OpenTerminalsOrPlugins));
}

#[test]
fn the_settings_screen_asks_for_the_permissions_of_a_bar_with_every_option_on() {
    assert_eq!(
        (settings::SCHEMA.permissions)(),
        bar_permissions(true, true)
    );
    assert_eq!((BAR.permissions)(true), bar_permissions(true, true));
    assert_eq!((BAR.permissions)(false), bar_permissions(false, false));
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
    assert!(calls.contains(&Call::RequestPermission(bar_permissions(true, true))));
}

#[test]
fn turning_the_close_confirmation_on_later_asks_for_the_larger_set() {
    let mut state = loaded(&[("mouse_close_confirm", "false")]);
    state.host.clear();
    assert!(BarCore::follow(
        &mut state,
        &BAR,
        Handled::Reconfigure(cfg(&[]))
    ));
    // The prompt's answer comes with the mouse's events, subscribed already.
    assert_eq!(
        state.host.calls(),
        vec![
            Call::SetSelectable(true),
            Call::RequestPermission(bar_permissions(true, true)),
        ]
    );
}

#[test]
fn turning_the_close_confirmation_off_asks_for_nothing() {
    let mut state = loaded(&[]);
    state.host.clear();
    assert!(BarCore::follow(
        &mut state,
        &BAR,
        Handled::Reconfigure(cfg(&[("mouse_close_confirm", "false")]))
    ));
    assert_eq!(state.host.calls(), vec![]);
}

#[test]
fn the_close_confirmation_alone_asks_for_nothing_while_mouse_is_off() {
    let mut state = loaded(&[("mouse", "false"), ("mouse_close_confirm", "false")]);
    state.host.clear();
    assert!(BarCore::follow(
        &mut state,
        &BAR,
        Handled::Reconfigure(cfg(&[("mouse", "false")]))
    ));
    assert_eq!(state.host.calls(), vec![]);
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
    assert_eq!(state.show_mode, DEFAULT_SHOW_MODE);
    assert_eq!(state.mode_format, None);
    assert_eq!(state.mouse, DEFAULT_MOUSE);
    assert_eq!(state.mouse_wheel_reversed, DEFAULT_MOUSE_WHEEL_REVERSED);
    assert_eq!(state.mouse_close_confirm, DEFAULT_MOUSE_CLOSE_CONFIRM);
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
        ("show_mode", "false"),
        ("mode_format", "<{mode}>"),
        ("mouse", "false"),
        ("mouse_wheel_reversed", "true"),
        ("mouse_close_confirm", "false"),
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
    assert!(!state.show_mode);
    assert_eq!(state.mode_format.as_deref(), Some("<{mode}>"));
    assert!(!state.mouse);
    assert!(state.mouse_wheel_reversed);
    assert!(!state.mouse_close_confirm);
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

/// Formats with no styling, so a test can read the line as plain text, and
/// no mode prefix, so the line holds only the tabs. A test about the prefix
/// turns `show_mode` back on.
const PLAIN: [(&str, &str); 6] = [
    ("show_mode", "false"),
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

/// `pairs` with the mode prefix turned off, for a test about the tabs in a
/// look other than `PLAIN`'s.
fn without_prefix(pairs: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
    let mut all = vec![("show_mode", "false")];
    all.extend_from_slice(pairs);
    all
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

/// What the click spans hold for the tab at `position` of `tabs`, whose id
/// `tab` makes 100 more than its position.
fn drawn_tab(position: usize) -> Clickable {
    Clickable::Tab {
        position,
        tab_id: 100 + position,
    }
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
    let state = bar(&without_prefix(&[]), 80, tabs(2, 0));
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
    let mut state = loaded(&without_prefix(&[]));
    state.update(own_pane(80));
    state.update(Event::TabUpdate(tabs(2, 0)));
    assert_eq!(state.draw_bar().0, " 1 a   2 b ");
}

#[test]
fn a_flag_with_styles_of_its_own_is_drawn_over_the_tab_s_style() {
    let mut info = active(tab(0, "a"));
    info.is_sync_panes_active = true;
    let state = bar(
        &without_prefix(&[
            ("active_tab_format", "#[bold]<{flags}>"),
            ("sync_format", "#[fg=red]S"),
        ]),
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
        &without_prefix(&[
            ("format_theme", "flat"),
            ("bell_format", "#[fg=$red,bold] \u{25cf}"),
        ]),
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
    let state = bar(&without_prefix(&[("tab_spacer", "|")]), 20, tabs(10, 5));
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
        vec![("[1:a lo".to_string(), drawn_tab(0))]
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
                drawn_tab(position)
            )]
        );
    }
    // In Zellij's look, whose markers are wider.
    let mut infos = tabs(13, 12);
    infos[12].name = "editor".to_string();
    assert_eq!(shown(&bar(&without_prefix(&[]), 10, infos)), " 13 edi...");
}

#[test]
fn a_cut_line_keeps_its_styles_closed() {
    let state = bar(&without_prefix(&[]), 6, vec![active(tab(0, "a long name"))]);
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
            ("5:e".to_string(), drawn_tab(4)),
            ("[6:f]".to_string(), drawn_tab(5)),
            ("7:g".to_string(), drawn_tab(6)),
            ("3>".to_string(), Clickable::Marker(7)),
        ]
    );
}

#[test]
fn the_click_spans_count_wide_characters_and_skip_styling() {
    let infos = vec![active(tab(0, "\u{65e5}\u{672c}")), tab(1, "x")];
    // Styled by a format theme, with a CJK name two columns per character.
    let state = bar(
        &without_prefix(&[("format_theme", "flat")]),
        80,
        infos.clone(),
    );
    assert_eq!(
        clickable(&state),
        vec![
            (" 1 \u{65e5}\u{672c} ".to_string(), drawn_tab(0)),
            (" 2 x ".to_string(), drawn_tab(1)),
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
    assert_eq!(spans.value_at(5), Some(&drawn_tab(0)));
    assert_eq!(spans.value_at(6), None);
    assert_eq!(spans.value_at(7), Some(&drawn_tab(1)));
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
        // The mode prefix too, in modes with and without a symbol of their
        // own in some alias theme, and cut when it alone is too wide.
        let mut with_prefix = combination.clone();
        with_prefix.push(("show_mode", "true"));
        for input_mode in [InputMode::Normal, InputMode::Locked, InputMode::RenameTab] {
            for width in [400, 30, 4] {
                let mut state = bar(&with_prefix, width, infos.clone());
                state.update(Event::ModeUpdate(mode(input_mode)));
                let (line, _) = state.draw_bar();
                let text = strip_ansi(&line);
                let name = format!("{input_mode:?}").to_uppercase();
                assert!(
                    text.contains(&name) || width == 4,
                    "{combination:?} {input_mode:?} {width}: {text:?}"
                );
                for raw in ["#[", "$", "{"] {
                    assert!(
                        !line.contains(raw),
                        "{combination:?} {input_mode:?} {width}: left {raw}"
                    );
                }
            }
        }
    }
}

// The mode prefix.

/// `PLAIN` with the mode prefix shown as `<` and the mode's name and `>`.
fn plain_with_mode(pairs: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
    let mut all = plain_with(&[("show_mode", "true"), ("mode_format", "<{desc}>")]);
    all.extend_from_slice(pairs);
    all
}

#[test]
fn the_mode_prefix_is_drawn_before_the_tabs_only_with_show_mode_on() {
    assert_eq!(shown(&bar(&PLAIN, 80, tabs(2, 0))), "[1:a]|2:b");
    assert_eq!(
        shown(&bar(&plain_with_mode(&[]), 80, tabs(2, 0))),
        "<NORMAL>|[1:a]|2:b"
    );
    // A format of its own outranks it, but `mode_format` alone sets nothing.
    let unshown = plain_with(&[("mode_format", "<{desc}>")]);
    assert_eq!(shown(&bar(&unshown, 80, tabs(2, 0))), "[1:a]|2:b");
}

#[test]
fn the_mode_prefix_takes_each_mode_s_label_symbol_and_format() {
    let options = plain_with_mode(&[
        ("mode_format", "<{key}{desc}:{mode}>"),
        ("mode_label_normal", "N"),
        ("mode_key_normal", "*"),
        ("mode_format_locked", "LOCKED!"),
    ]);
    let mut state = bar(&options, 80, tabs(1, 0));
    assert_eq!(shown(&state), "<*N:NORMAL>|[1:a]");
    state.update(Event::ModeUpdate(mode(InputMode::Locked)));
    assert_eq!(shown(&state), "LOCKED!|[1:a]");
    state.update(Event::ModeUpdate(mode(InputMode::EnterSearch)));
    assert_eq!(shown(&state), "<ENTERSEARCH:ENTERSEARCH>|[1:a]");
}

#[test]
fn rename_mode_shows_as_rename_tab() {
    let mut state = bar(&plain_with_mode(&[]), 80, tabs(1, 0));
    state.update(Event::ModeUpdate(mode(InputMode::RenameTab)));
    assert_eq!(shown(&state), "<RENAMETAB>|[1:a]");
    let mut state = bar(
        &plain_with_mode(&[("mode_label_rename_tab", "RENAME")]),
        80,
        tabs(1, 0),
    );
    state.update(Event::ModeUpdate(mode(InputMode::RenameTab)));
    assert_eq!(shown(&state), "<RENAME>|[1:a]");
}

#[test]
fn with_no_format_the_mode_prefix_is_a_bold_block_in_the_selected_ribbon_colors() {
    // Shown with no options at all, as `show_mode` is on by default.
    let state = bar(&[], 80, tabs(1, 0));
    let (line, _) = state.draw_bar();
    assert_eq!(strip_ansi(&line), " NORMAL   1 a ");
    // Bold, on `ribbon_selected`'s background in its base color.
    assert!(
        line.starts_with("\u{1b}[1;48;2;166;227;161;38;2;1;2;3m NORMAL "),
        "{line:?}"
    );
}

#[test]
fn the_tabs_are_fitted_into_what_the_mode_prefix_leaves() {
    // "<NORMAL>|" takes 9 columns, so 28 leave the 19 that take three tabs
    // and both markers, and 26 the 17 that take two.
    let options = plain_with_mode(&[]);
    assert_eq!(
        shown(&bar(&options, 28, tabs(10, 5))),
        "<NORMAL>|<4|5:e|[6:f]|7:g|3>"
    );
    assert_eq!(
        shown(&bar(&options, 26, tabs(10, 5))),
        "<NORMAL>|<5|[6:f]|7:g|3>"
    );
    // The tabs alone are cut, never the prefix while it fits.
    let options = plain_with_mode(&[("overflow_str", "~")]);
    let state = bar(&options, 14, vec![active(tab(0, "a long name"))]);
    assert_eq!(shown(&state), "<NORMAL>|[1:a~");
}

#[test]
fn a_mode_prefix_that_fills_the_bar_draws_no_tabs_and_one_too_wide_is_cut() {
    let options = plain_with_mode(&[("overflow_str", "~")]);
    for width in [8, 9] {
        let state = bar(&options, width, tabs(3, 1));
        assert_eq!(shown(&state), "<NORMAL>", "{width}");
        assert_eq!(clickable(&state), vec![], "{width}");
    }
    let state = bar(&options, 5, tabs(3, 1));
    assert_eq!(shown(&state), "<NOR~");
    assert_eq!(clickable(&state), vec![]);
}

#[test]
fn the_mode_prefix_dims_with_the_tabs() {
    let dimmed_mode = || {
        let mut mode_info = mode(InputMode::Normal);
        mode_info.session_dimmed = Some(true);
        Event::ModeUpdate(mode_info)
    };
    // The tabs are unstyled, so only the prefix can change with the dimming:
    // in the Zellij theme's colors, and in a format's own.
    for options in [
        plain_with(&[("show_mode", "true")]),
        plain_with(&[
            ("show_mode", "true"),
            ("mode_format", "#[fg=#ff0000]<{desc}>"),
        ]),
    ] {
        let undimmed = bar(&options, 80, tabs(1, 0)).draw_bar().0;
        let mut state = bar(&options, 80, tabs(1, 0));
        state.update(dimmed_mode());
        let dimmed = state.draw_bar().0;
        assert_ne!(dimmed, undimmed, "{options:?}");
        assert_eq!(strip_ansi(&dimmed), strip_ansi(&undimmed));

        let mut undimming = options.clone();
        undimming.push(("dim_when_unfocused", "false"));
        let mut state = bar(&undimming, 80, tabs(1, 0));
        state.update(dimmed_mode());
        assert_eq!(state.draw_bar().0, undimmed, "{options:?}");
    }
}

#[test]
fn a_click_on_the_first_tab_still_hits_it_with_the_mode_prefix_shown() {
    // "<NORMAL>|1:a|[2:b]|3:c"
    let mut state = bar(&plain_with_mode(&[]), 80, tabs(3, 1));
    assert_eq!(
        clickable(&state),
        vec![
            ("1:a".to_string(), drawn_tab(0)),
            ("[2:b]".to_string(), drawn_tab(1)),
            ("3:c".to_string(), drawn_tab(2)),
        ]
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 9)),
        vec![Call::SwitchTabTo(1)]
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::LeftClick(0, 21)),
        vec![Call::SwitchTabTo(3)]
    );
}

#[test]
fn a_click_on_the_mode_prefix_does_nothing_but_a_right_click_opens_the_bar_s_menu() {
    // "<NORMAL>|1:a|[2:b]|3:c", closing at once so a middle click would show.
    let options = plain_with_mode(&[("mouse_close_confirm", "false")]);
    let mut state = bar(&options, 80, tabs(3, 1));
    for column in [0, 7, 8] {
        for mouse in [Mouse::LeftClick(0, column), Mouse::MiddleClick(0, column)] {
            assert_eq!(after_mouse(&mut state, mouse), vec![], "{mouse:?}");
        }
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
fn a_mode_change_redraws_for_every_mode_while_the_mode_prefix_shows_it() {
    let mut state = bar(&plain_with_mode(&[]), 80, tabs(1, 0));
    assert!(!state.update(Event::ModeUpdate(mode(InputMode::Normal))));
    assert!(state.update(Event::ModeUpdate(mode(InputMode::Pane))));
    assert!(!state.update(Event::ModeUpdate(mode(InputMode::Pane))));
    assert!(state.update(Event::ModeUpdate(mode(InputMode::Tab))));
    assert_eq!(shown(&state), "<TAB>|[1:a]");
}

#[test]
fn turning_show_mode_on_or_changing_its_format_redraws() {
    let mut state = bar(&PLAIN, 80, tabs(1, 0));
    for (options, drawn) in [
        (plain_with(&[("show_mode", "true")]), " NORMAL |[1:a]"),
        (plain_with_mode(&[]), "<NORMAL>|[1:a]"),
        (plain_with_mode(&[("mode_label_normal", "N")]), "<N>|[1:a]"),
    ] {
        assert!(BarCore::follow(
            &mut state,
            &BAR,
            Handled::Reconfigure(cfg(&options))
        ));
        assert_eq!(shown(&state), drawn);
    }
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

/// `PLAIN` with `mouse_close_confirm` off, so a middle click closes at once.
fn plain_closing_at_once() -> Vec<(&'static str, &'static str)> {
    plain_with(&[("mouse_close_confirm", "false")])
}

#[test]
fn a_middle_click_closes_the_tab_under_it() {
    // "1:a|[2:b]|3:c", the ids 100, 101 and 102.
    let mut state = bar(&plain_closing_at_once(), 80, tabs(3, 1));
    assert_eq!(
        after_mouse(&mut state, Mouse::MiddleClick(0, 0)),
        vec![Call::CloseTabWithId(100)]
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::MiddleClick(0, 4)),
        vec![Call::CloseTabWithId(101)]
    );
    assert_eq!(
        after_mouse(&mut state, Mouse::MiddleClick(0, 12)),
        vec![Call::CloseTabWithId(102)]
    );
}

#[test]
fn a_middle_click_off_a_tab_or_a_middle_drag_closes_nothing() {
    // "1:a|[2:b]|3:c": the spacer, past the end, and a line below the bar.
    let mut state = bar(&PLAIN, 80, tabs(3, 1));
    for mouse in [
        Mouse::MiddleClick(0, 3),
        Mouse::MiddleClick(0, 40),
        Mouse::MiddleClick(1, 0),
        Mouse::MiddleHold(0, 0),
    ] {
        assert_eq!(after_mouse(&mut state, mouse), vec![], "{mouse:?}");
    }
    // "<4|5:e|[6:f]|7:g|3>": neither marker, as each stands for several tabs.
    let mut state = bar(&PLAIN, 19, tabs(10, 5));
    for column in [0, 18] {
        assert_eq!(
            after_mouse(&mut state, Mouse::MiddleClick(0, column)),
            vec![]
        );
    }
}

#[test]
fn a_middle_click_closes_the_tab_drawn_under_it_even_after_the_tabs_move() {
    // "1:a|[2:b]|3:c" is drawn, then a `TabUpdate` puts a new tab first
    // before the bar draws again.
    let mut state = bar(&plain_closing_at_once(), 80, tabs(3, 1));
    state.render(1, 80);
    let mut moved = vec![tab(0, "new")];
    moved[0].tab_id = 200;
    moved.extend(tabs(3, 1).into_iter().map(|mut info| {
        info.position += 1;
        info
    }));
    assert!(state.update(Event::TabUpdate(moved)));
    state.host.clear();
    assert!(!state.update(Event::Mouse(Mouse::MiddleClick(0, 0))));
    assert_eq!(state.host.calls(), vec![Call::CloseTabWithId(100)]);
}

/// The one call of `calls`, which has to be a prompt: its id and request.
fn the_prompt(calls: &[Call]) -> (u64, PromptRequest) {
    match calls {
        [Call::Prompt {
            request_id,
            request,
        }] => (*request_id, request.clone()),
        _ => panic!("expected one prompt, got {calls:?}"),
    }
}

/// The calls the bar makes on Zellij's answer `result` to the prompt
/// `request_id`.
fn after_answer(state: &mut State, request_id: u64, result: PromptResult) -> Vec<Call> {
    state.host.clear();
    assert!(!state.update(Event::PromptResult(request_id, result)));
    state.host.calls()
}

#[test]
fn a_middle_click_asks_before_closing_the_tab_with_cancel_selected() {
    // "1:a|[2:b]|3:c"
    let mut state = bar(&PLAIN, 80, tabs(3, 1));
    let (_, request) = the_prompt(&after_mouse(&mut state, Mouse::MiddleClick(0, 12)));
    let PromptSpec::Confirm { message, yes, no } = &request.spec else {
        panic!("expected a confirmation, got {:?}", request.spec);
    };
    assert_eq!(message.text, "Close tab 3 \"c\" and every pane in it?");
    assert_eq!((yes.as_str(), no.as_str()), ("Close tab", "Cancel"));
    assert_eq!(request.title.as_deref(), Some("Close tab?"));
    // Zellij selects the button the default names, so `Enter` cancels.
    assert_eq!(request.default.as_deref(), Some("false"));
}

#[test]
fn the_close_confirmation_numbers_and_names_the_tab_as_the_bar_draws_it() {
    let mut infos = tabs(2, 0);
    infos[1].name = "esc\u{1b}[2J".to_string();
    let mut state = bar(&plain_with(&[("index_from_zero", "true")]), 80, infos);
    let line = shown(&state);
    let column = line.find("1:").unwrap();
    let (_, request) = the_prompt(&after_mouse(&mut state, Mouse::MiddleClick(0, column)));
    let PromptSpec::Confirm { message, .. } = &request.spec else {
        panic!("expected a confirmation, got {:?}", request.spec);
    };
    assert_eq!(
        message.text,
        "Close tab 1 \"esc\\u{1b}[2J\" and every pane in it?"
    );
}

#[test]
fn confirming_closes_the_tab_that_was_clicked() {
    // "1:a|[2:b]|3:c"
    let mut state = bar(&PLAIN, 80, tabs(3, 1));
    let (request_id, _) = the_prompt(&after_mouse(&mut state, Mouse::MiddleClick(0, 0)));
    assert_eq!(
        after_answer(&mut state, request_id, PromptResult::Confirmed(true)),
        vec![Call::CloseTabWithId(100)]
    );
    // Answered once, it is no longer waiting.
    assert_eq!(
        after_answer(&mut state, request_id, PromptResult::Confirmed(true)),
        vec![]
    );
}

#[test]
fn any_other_answer_to_the_close_confirmation_closes_nothing() {
    for result in [
        PromptResult::Confirmed(false),
        PromptResult::Cancelled,
        PromptResult::TimedOut(None),
        PromptResult::TimedOut(Some(PromptValue::Bool(true))),
        PromptResult::Error("no popup".to_string()),
        PromptResult::Answered(PromptValue::Bool(true)),
    ] {
        let mut state = bar(&PLAIN, 80, tabs(3, 1));
        let (request_id, _) = the_prompt(&after_mouse(&mut state, Mouse::MiddleClick(0, 0)));
        assert_eq!(
            after_answer(&mut state, request_id, result.clone()),
            vec![],
            "{result:?}"
        );
        assert_eq!(state.pending_close, None, "{result:?}");
    }
}

#[test]
fn an_answer_to_another_prompt_closes_nothing_and_keeps_waiting() {
    let mut state = bar(&PLAIN, 80, tabs(3, 1));
    let (request_id, _) = the_prompt(&after_mouse(&mut state, Mouse::MiddleClick(0, 0)));
    assert_eq!(
        after_answer(&mut state, request_id + 1, PromptResult::Confirmed(true)),
        vec![]
    );
    assert_eq!(
        after_answer(&mut state, request_id, PromptResult::Confirmed(true)),
        vec![Call::CloseTabWithId(100)]
    );
}

#[test]
fn an_answer_with_nothing_waiting_closes_nothing() {
    let mut state = bar(&PLAIN, 80, tabs(3, 1));
    assert_eq!(
        after_answer(&mut state, 1, PromptResult::Confirmed(true)),
        vec![]
    );
}

#[test]
fn confirming_after_the_tab_is_gone_closes_nothing() {
    // "1:a|[2:b]|3:c"
    let mut state = bar(&PLAIN, 80, tabs(3, 1));
    let (request_id, _) = the_prompt(&after_mouse(&mut state, Mouse::MiddleClick(0, 0)));
    // Closed some other way while the confirmation was up.
    let mut remaining = tabs(3, 1);
    remaining.remove(0);
    state.update(Event::TabUpdate(remaining));
    assert_eq!(
        after_answer(&mut state, request_id, PromptResult::Confirmed(true)),
        vec![]
    );
}

#[test]
fn a_second_middle_click_replaces_the_close_confirmation_being_shown() {
    // "1:a|[2:b]|3:c"
    let mut state = bar(&PLAIN, 80, tabs(3, 1));
    let (first, _) = the_prompt(&after_mouse(&mut state, Mouse::MiddleClick(0, 0)));
    let (second, _) = the_prompt(&after_mouse(&mut state, Mouse::MiddleClick(0, 12)));
    assert_ne!(first, second);
    assert_eq!(
        after_answer(&mut state, first, PromptResult::Confirmed(true)),
        vec![]
    );
    assert_eq!(
        after_answer(&mut state, second, PromptResult::Confirmed(true)),
        vec![Call::CloseTabWithId(102)]
    );
}

#[test]
fn the_close_confirmation_names_and_closes_the_tab_drawn_under_the_click_after_the_tabs_move() {
    // "1:a|[2:b]|3:c" is drawn, then a `TabUpdate` puts a new tab first
    // before the bar draws again.
    let mut state = bar(&PLAIN, 80, tabs(3, 1));
    state.render(1, 80);
    let mut moved = vec![tab(0, "new")];
    moved[0].tab_id = 200;
    moved.extend(tabs(3, 1).into_iter().map(|mut info| {
        info.position += 1;
        info
    }));
    assert!(state.update(Event::TabUpdate(moved)));
    state.host.clear();
    assert!(!state.update(Event::Mouse(Mouse::MiddleClick(0, 0))));
    let (request_id, request) = the_prompt(&state.host.calls());
    let PromptSpec::Confirm { message, .. } = &request.spec else {
        panic!("expected a confirmation, got {:?}", request.spec);
    };
    // The tab clicked, "a", now stands second.
    assert_eq!(message.text, "Close tab 2 \"a\" and every pane in it?");
    assert_eq!(
        after_answer(&mut state, request_id, PromptResult::Confirmed(true)),
        vec![Call::CloseTabWithId(100)]
    );
}

#[test]
fn with_mouse_off_the_mouse_does_nothing() {
    let mut state = bar(&plain_with(&[("mouse", "false")]), 80, tabs(3, 1));
    for mouse in [
        Mouse::LeftClick(0, 0),
        Mouse::DoubleClick(0, 0),
        Mouse::RightClick(0, 0),
        Mouse::MiddleClick(0, 0),
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
        Mouse::MiddleClick(0, 0),
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
            Call::AddSettingsEntry("zjtabs".to_string()),
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
            own_url: Some("zjtabs".to_string()),
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
            own_url: Some("zjtabs".to_string()),
        }]
    );
    assert_eq!(SETTINGS_PANE_TITLE, "zjtabs settings");
    assert_eq!(OPEN_SETTINGS_MESSAGE, "zjtabs::open_settings");
}

/// A preview from the settings screen of `alias`, as `pipe` gets it, with
/// each change as `(key, value before, value now)`.
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
        PipeSource::Plugin(5),
        settings::SCHEMA.live_message,
        &None,
        &Some(live::encode(alias, &changes)),
        false,
    ))
}

#[test]
fn a_preview_through_the_bar_s_pipe_changes_what_it_draws() {
    let mut state = bar(&PLAIN, 80, tabs(3, 1));
    state.bar.own_url = Some("zjtabs".to_string());
    assert_eq!(shown(&state), "1:a|[2:b]|3:c");

    let changes = [
        ("tab_spacer", Some("|"), Some("/")),
        ("index_from_zero", None, Some("true")),
        ("active_tab_format", Some("[{index}:{name}{flags}]"), None),
    ];
    assert!(preview(&mut state, "zjtabs", &changes));
    // The active tab is back in Zellij's look, which draws no brackets.
    assert_eq!(shown(&state), "0:a/ 1 b /2:c");

    // Back again, as the screen sends it when an edit is undone.
    let undo = [
        ("tab_spacer", Some("/"), Some("|")),
        ("index_from_zero", Some("true"), None),
        ("active_tab_format", None, Some("[{index}:{name}{flags}]")),
    ];
    assert!(preview(&mut state, "zjtabs", &undo));
    assert_eq!(shown(&state), "1:a|[2:b]|3:c");
}

#[test]
fn a_preview_of_another_alias_changes_nothing() {
    let mut state = bar(&PLAIN, 80, tabs(3, 1));
    state.bar.own_url = Some("zjtabs".to_string());
    let changes = [("tab_spacer", Some("|"), Some("/"))];
    assert!(!preview(&mut state, "another", &changes));
    assert_eq!(shown(&state), "1:a|[2:b]|3:c");
}
