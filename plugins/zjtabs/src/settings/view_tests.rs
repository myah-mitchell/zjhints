use super::{draw_bar, PREVIEW_COLUMNS, SCHEMA};
use crate::theme::tests::{every_combination, zellij_palette};
use std::collections::BTreeMap;
use zellij_tile::prelude::*;
use zjkit::ansi::{calculate_visible_length, AnsiParser, AnsiSegment};
use zjkit::settings::view::{render_sample, theme_preview};

fn config_of(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// The mode and Zellij theme the settings screen is running in.
fn mode_info() -> ModeInfo {
    let mut mode_info = ModeInfo {
        mode: InputMode::Normal,
        ..Default::default()
    };
    mode_info.style.colors = zellij_palette();
    mode_info
}

/// `text` with the styling stripped out.
fn plain(text: &str) -> String {
    let mut parser = AnsiParser::new(text);
    let mut visible = String::new();
    while let Some(segment) = parser.next_segment() {
        if let AnsiSegment::VisibleChar(ch) = segment {
            visible.push(ch);
        }
    }
    visible
}

/// A format string as the list draws it under `key`, as plain text.
fn sampled(key: &str, template: &str, pairs: &[(&str, &str)]) -> Option<String> {
    render_sample(&SCHEMA, key, template, &config_of(pairs)).map(|(plain, _)| plain)
}

#[test]
fn a_tab_format_is_sampled_with_an_index_a_name_and_a_flag() {
    for key in ["tab_format", "active_tab_format"] {
        assert_eq!(
            sampled(key, "{index}:{name}{flags}", &[]).as_deref(),
            Some("2:editor \u{25cf}"),
            "{key}"
        );
    }
}

#[test]
fn the_sample_flag_is_the_bell_flag_as_configured_without_its_styling() {
    assert_eq!(
        sampled("tab_format", "{flags}", &[("bell_format", " #[fg=red]!")]).as_deref(),
        Some(" !")
    );
}

#[test]
fn an_overflow_marker_is_sampled_with_a_count() {
    for key in ["overflow_left_format", "overflow_right_format"] {
        assert_eq!(
            sampled(key, "+{count}", &[]).as_deref(),
            Some("+3"),
            "{key}"
        );
    }
}

#[test]
fn other_formats_have_nothing_to_fill_in() {
    for key in [
        "tab_spacer",
        "bell_format",
        "overflow_str",
        "something_else",
    ] {
        assert!(
            (SCHEMA.sample_values)(key, &BTreeMap::new()).is_empty(),
            "{key}"
        );
    }
}

#[test]
fn the_preview_shows_the_active_tab_a_flagged_one_and_markers_for_the_rest() {
    let line = plain(&draw_bar(BTreeMap::new(), &mode_info(), 200));
    assert!(line.contains("3 editor"), "{line:?}");
    assert!(line.contains("server \u{25cf}"), "{line:?}");
    assert!(line.contains("+"), "{line:?}");
    assert!(!line.contains("build"), "{line:?}");
}

#[test]
fn the_preview_is_no_wider_than_it_is_given_or_the_most_it_draws() {
    for (width, drawn) in [(200, PREVIEW_COLUMNS), (40, 40)] {
        let line = draw_bar(BTreeMap::new(), &mode_info(), width);
        assert!(calculate_visible_length(&line, false) <= drawn, "{width}");
    }
}

#[test]
fn the_preview_draws_whatever_the_bar_would_hide_it_for() {
    let hiding = config_of(&[("hidden", "true"), ("hide_when_nested", "true")]);
    assert_eq!(
        draw_bar(hiding, &mode_info(), 80),
        draw_bar(BTreeMap::new(), &mode_info(), 80)
    );
}

#[test]
fn the_preview_is_neither_hidden_nor_dimmed_by_a_nested_session() {
    let undisturbed = draw_bar(BTreeMap::new(), &mode_info(), 80);
    // A nested session whose host's bars are on screen, and that is not
    // where the keyboard goes.
    let mut nested = mode_info();
    nested.session_ancestry = vec!["host".to_string()];
    nested.session_ascended = Some(true);
    // A session whose keyboard has gone to one it hosts.
    let mut descended = mode_info();
    descended.session_dimmed = Some(true);
    let asked_to = config_of(&[("hide_when_nested", "true"), ("dim_when_unfocused", "true")]);
    for mode_info in [nested, descended] {
        for configuration in [BTreeMap::new(), asked_to.clone()] {
            assert_eq!(draw_bar(configuration, &mode_info, 80), undisturbed);
        }
    }
}

#[test]
fn the_preview_fits_the_pane_whatever_auto_width_says() {
    let unfitted = config_of(&[("auto_width", "false")]);
    let line = draw_bar(unfitted, &mode_info(), 80);
    assert!(calculate_visible_length(&line, false) <= PREVIEW_COLUMNS);
}

#[test]
fn the_preview_draws_the_users_formats_with_the_sample_tabs() {
    let configuration = config_of(&[
        ("show_mode", "false"),
        ("show_session", "false"),
        ("tab_format", "{index}:{name}{flags}"),
        ("active_tab_format", "[{index}:{name}{flags}]"),
        ("tab_spacer", "|"),
        ("bell_format", "!"),
        ("overflow_right_format", "{count}>"),
    ]);
    let line = plain(&draw_bar(configuration, &mode_info(), 80));
    assert!(line.starts_with("1:shell|2:docs"), "{line:?}");
    assert!(line.contains("[3:editor]|4:server!|"), "{line:?}");
    assert!(line.ends_with(">"), "{line:?}");
}

#[test]
fn the_preview_draws_every_theme_combination_with_nothing_left_raw() {
    let mode_info = mode_info();
    for combination in every_combination() {
        let configuration = config_of(&combination);
        for width in [200, 40] {
            let line = draw_bar(configuration.clone(), &mode_info, width);
            assert!(!line.is_empty(), "{combination:?} {width}");
            let text = plain(&line);
            assert!(text.contains("editor"), "{combination:?} {width}: {text:?}");
            for raw in ["#[", "$", "{"] {
                assert!(!line.contains(raw), "{combination:?} {width}: left {raw}");
            }
        }
    }
}

#[test]
fn a_mode_format_is_sampled_with_the_mode_it_is_for() {
    assert_eq!(
        sampled("mode_format", "<{mode}>", &[]).as_deref(),
        Some("<NORMAL>")
    );
    assert_eq!(
        sampled("mode_format_enter_search", "<{mode}>", &[]).as_deref(),
        Some("<ENTERSEARCH>")
    );
}

#[test]
fn the_preview_shows_the_mode_prefix_unless_show_mode_is_off() {
    let line = plain(&draw_bar(BTreeMap::new(), &mode_info(), 200));
    assert!(line.starts_with(" NORMAL "), "{line:?}");
    assert!(line.contains("3 editor"), "{line:?}");
    let unshown = config_of(&[("show_mode", "false")]);
    let line = plain(&draw_bar(unshown, &mode_info(), 200));
    assert!(!line.contains("NORMAL"), "{line:?}");
    assert!(line.contains("3 editor"), "{line:?}");
}

#[test]
fn the_preview_draws_the_mode_prefix_in_every_theme_combination_with_nothing_left_raw() {
    let mode_info = mode_info();
    for combination in every_combination() {
        let mut configuration = config_of(&combination);
        configuration.insert("show_mode".to_string(), "true".to_string());
        for width in [200, 40] {
            let line = draw_bar(configuration.clone(), &mode_info, width);
            let text = plain(&line);
            assert!(text.contains("NORMAL"), "{combination:?} {width}: {text:?}");
            assert!(text.contains("editor"), "{combination:?} {width}: {text:?}");
            for raw in ["#[", "$", "{"] {
                assert!(!line.contains(raw), "{combination:?} {width}: left {raw}");
            }
        }
    }
}

#[test]
fn a_session_format_is_sampled_with_a_session_name() {
    assert_eq!(
        sampled("session_format", "[{session}]", &[]).as_deref(),
        Some("[work]")
    );
}

#[test]
fn the_preview_ends_with_the_session_name_unless_show_session_is_off() {
    // The sample name before Zellij has named the screen's own session, and
    // that name once it has.
    let mut named = mode_info();
    named.session_name = Some("my-session".to_string());
    for (mode_info, session) in [(mode_info(), " work "), (named, " my-session ")] {
        let line = plain(&draw_bar(BTreeMap::new(), &mode_info, 200));
        assert!(line.ends_with(session), "{line:?}");
        assert_eq!(calculate_visible_length(&line, false), PREVIEW_COLUMNS);
        assert!(line.contains("3 editor"), "{line:?}");
    }
    let unshown = config_of(&[("show_session", "false")]);
    let line = plain(&draw_bar(unshown, &mode_info(), 200));
    assert!(!line.contains("work"), "{line:?}");
}

#[test]
fn the_preview_draws_the_session_name_in_every_theme_combination_with_nothing_left_raw() {
    let mode_info = mode_info();
    for combination in every_combination() {
        let configuration = config_of(&combination);
        // Wide enough for every tab, and narrow enough for markers with room
        // for the session name beside them.
        for width in [200, 60] {
            let line = draw_bar(configuration.clone(), &mode_info, width);
            let text = plain(&line);
            assert!(
                text.trim_end().ends_with("work"),
                "{combination:?} {width}: {text:?}"
            );
            assert!(text.contains("editor"), "{combination:?} {width}: {text:?}");
            for raw in ["#[", "$", "{"] {
                assert!(!line.contains(raw), "{combination:?} {width}: left {raw}");
            }
        }
    }
}

#[test]
fn the_themes_change_the_preview_and_the_overrides_are_left_out_on_request() {
    let mode_info = mode_info();
    let themed = config_of(&[("format_theme", "plain"), ("tab_spacer", "|")]);
    let with = theme_preview(&SCHEMA, &themed, &mode_info, true, 80);
    let without = theme_preview(&SCHEMA, &themed, &mode_info, false, 80);
    assert!(plain(&with).contains('|'), "{with:?}");
    assert!(!plain(&without).contains('|'), "{without:?}");
    assert_ne!(
        with,
        theme_preview(&SCHEMA, &BTreeMap::new(), &mode_info, true, 80)
    );
}
