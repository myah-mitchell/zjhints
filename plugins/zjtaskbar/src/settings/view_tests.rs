use super::options::{FAMILIES, OPTIONS};
use super::{draw_bar, preview_state, PREVIEW_COLUMNS, SCHEMA};
use crate::theme::tests::{every_combination, zellij_palette};
use std::collections::BTreeMap;
use zellij_tile::prelude::*;
use zjkit::ansi::{calculate_visible_length, AnsiParser, AnsiSegment};
use zjkit::settings::schema::Kind;
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

/// The preview of `pairs`, `width` columns wide, as plain text.
fn previewed(pairs: &[(&str, &str)], width: usize) -> String {
    plain(&draw_bar(config_of(pairs), &mode_info(), width))
}

/// A format string as the list draws it under `key`, as plain text.
fn sampled(key: &str, template: &str, pairs: &[(&str, &str)]) -> Option<String> {
    render_sample(&SCHEMA, key, template, &config_of(pairs)).map(|(plain, _)| plain)
}

/// The names of the placeholders the list fills under `key`.
fn sampled_names(key: &str) -> Vec<&'static str> {
    (SCHEMA.sample_values)(key, &BTreeMap::new())
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

/// The format syntax left in `line`, which the bar should have drawn
/// rather than shown as written.
fn left_raw(line: &str) -> Option<&'static str> {
    ["#[", "$", "{"].into_iter().find(|raw| line.contains(raw))
}

// The samples in the list.

#[test]
fn an_entry_format_is_sampled_with_a_title_an_index_a_tab_and_a_flag() {
    for key in [
        "entry_format",
        "focused_entry_format",
        "minimized_entry_format",
    ] {
        assert_eq!(
            sampled_names(key),
            ["title", "index", "tab", "flags"],
            "{key}"
        );
        assert_eq!(
            sampled(key, "{index}:{title}@{tab}{flags}", &[]).as_deref(),
            Some("2:editor@work (EXITED)"),
            "{key}"
        );
    }
}

#[test]
fn the_sample_flag_is_the_exited_flag_as_configured_without_its_styling() {
    let styled = [("exited_format", "#[fg=red] X")];
    assert_eq!(
        sampled("entry_format", "{title}{flags}", &styled).as_deref(),
        Some("editor X")
    );
    let values = (SCHEMA.sample_values)("entry_format", &config_of(&styled));
    assert_eq!(values[3], ("flags", " X".to_string()));
}

#[test]
fn the_other_tab_flag_is_sampled_with_another_tab_s_name() {
    assert_eq!(
        sampled("other_tab_format", " ({tab})", &[]).as_deref(),
        Some(" (docs)")
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
fn the_launcher_s_button_is_sampled_with_its_symbol() {
    assert_eq!(
        sampled("launcher_format", "[{symbol}]", &[]).as_deref(),
        Some("[Menu]")
    );
    assert_eq!(
        sampled("launcher_format", "[{symbol}]", &[("launcher_symbol", "+")]).as_deref(),
        Some("[+]")
    );
}

#[test]
fn the_clock_is_sampled_at_the_sample_time_in_its_own_format_and_offset() {
    assert_eq!(
        sampled("clock_item_format", "<{clock}>", &[]).as_deref(),
        Some("<09:41>")
    );
    let set = [("clock_format", "%I:%M %p"), ("clock_utc_offset", "+04:00")];
    assert_eq!(
        sampled("clock_item_format", "{clock}", &set).as_deref(),
        Some("01:41 PM")
    );
}

#[test]
fn the_session_and_the_mode_are_sampled_as_the_tray_draws_them() {
    assert_eq!(
        sampled("session_format", "[{session}]", &[]).as_deref(),
        Some("[work]")
    );
    // Unset, the description is the mode's name and the symbol draws
    // nothing, rather than either showing as written.
    assert_eq!(
        sampled("mode_format", "[{key}]{desc} {mode}", &[]).as_deref(),
        Some("[]NORMAL NORMAL")
    );
    let per_mode = [("mode_label_locked", "Lock"), ("mode_key_locked", "L")];
    assert_eq!(
        sampled("mode_format_locked", "{key} {desc} {mode}", &per_mode).as_deref(),
        Some("L Lock LOCKED")
    );
}

#[test]
fn other_formats_have_nothing_to_fill_in() {
    for key in [
        "entry_spacer",
        "tab_separator",
        "empty_format",
        "minimized_format",
        "exited_format",
        "plugin_format",
        "overflow_str",
        "tray_spacer",
    ] {
        assert_eq!(sampled_names(key), Vec::<&str>::new(), "{key}");
    }
}

#[test]
fn every_placeholder_a_format_names_is_sampled() {
    // A key of each family, as the list would have it.
    let family_key = |pattern: &str| pattern.replace("<mode>", "locked");
    let formats = OPTIONS
        .iter()
        .filter(|option| option.kind == Kind::Format)
        .map(|option| (option.name.to_string(), option.description))
        .chain(
            FAMILIES
                .iter()
                .filter(|family| family.kind == Kind::Format)
                .map(|family| (family_key(family.pattern), family.description)),
        );
    for (key, description) in formats {
        // Every placeholder the description names, and so the user may use.
        let mut named = Vec::new();
        let mut rest = description;
        while let Some((_, after)) = rest.split_once('{') {
            let (name, after) = after.split_once('}').unwrap();
            named.push(name);
            rest = after;
        }
        let template: String = named.iter().map(|name| format!("<{{{name}}}>")).collect();
        let sample = sampled(&key, &format!("x{template}"), &[]).unwrap();
        assert!(!sample.contains('{'), "{key}: {sample:?}");
        for name in named {
            assert!(sampled_names(&key).contains(&name), "{key}: {{{name}}}");
        }
    }
}

// The preview.

#[test]
fn the_preview_shows_the_launcher_the_focused_entry_and_a_marker() {
    let line = previewed(&[], 200);
    assert!(line.starts_with(" Menu "), "{line:?}");
    assert!(line.contains(" htop "), "{line:?}");
    assert!(line.contains(" editor "), "{line:?}");
    assert!(line.contains("+3"), "{line:?}");
    // The tray is empty by default, so the line ends with the marker.
    assert!(line.ends_with("+3 → "), "{line:?}");
    assert!(calculate_visible_length(&line) <= PREVIEW_COLUMNS);
}

#[test]
fn the_preview_lists_the_sample_panes_with_the_users_formats() {
    let formats = [
        ("show_launcher", "false"),
        ("entry_format", "{index}{flags}"),
        ("focused_entry_format", "[{index}{flags}]"),
        ("minimized_entry_format", "{index}{flags}"),
        ("entry_spacer", "|"),
        ("tab_separator", "/"),
        ("minimized_format", "_"),
        ("exited_format", "!"),
        ("plugin_format", "*"),
        ("other_tab_format", "@{tab}"),
    ];
    // Every flag is raised by one sample pane or another, and the panes of
    // the other tab are told apart by its name.
    assert_eq!(previewed(&formats, 200), "1|[2]|3_|4!/5@docs|6*@docs");
    // The panes go through the bar's own reading, so the scope applies.
    let in_tab = [&formats[..], &[("pane_scope", "tab")]].concat();
    assert_eq!(previewed(&in_tab, 200), "1|[2]|3_|4!");
}

#[test]
fn the_preview_is_no_wider_than_it_is_given_or_the_most_it_draws() {
    for (width, drawn) in [(200, PREVIEW_COLUMNS), (40, 40)] {
        let line = draw_bar(BTreeMap::new(), &mode_info(), width);
        assert!(calculate_visible_length(&line) <= drawn, "{width}");
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
    assert!(calculate_visible_length(&line) <= PREVIEW_COLUMNS);
}

#[test]
fn the_preview_draws_the_launcher_s_button_while_the_bar_would() {
    assert!(previewed(&[("launcher_symbol", "+")], 200).starts_with(" + "));
    for off in [("show_launcher", "false"), ("mouse", "false")] {
        let line = previewed(&[off], 200);
        assert!(!line.contains("Menu"), "{off:?}: {line:?}");
        assert!(line.contains(" editor "), "{off:?}: {line:?}");
    }
    // A bar with nothing for its menu to list draws no button either.
    let empty_menu = [
        ("launcher_new_terminal", "false"),
        ("launcher_zellij_actions", "false"),
    ];
    assert!(!previewed(&empty_menu, 200).contains("Menu"));
    let one_entry = [
        &empty_menu[..],
        &[("launch_1_name", "Logs"), ("launch_1_command", "tail")],
    ];
    assert!(previewed(&one_entry.concat(), 200).starts_with(" Menu "));
}

#[test]
fn the_preview_s_tray_shows_the_sample_time_the_session_and_the_mode() {
    let tray = [("tray", "mode session clock"), ("clock_format", "%H:%M:%S")];
    let line = previewed(&tray, 200);
    assert!(line.ends_with(" NORMAL   work   09:41:27 "), "{line:?}");
    // The screen's own session's name once Zellij has sent it, and its mode.
    let mut named = mode_info();
    named.session_name = Some("my-session".to_string());
    named.mode = InputMode::Locked;
    let line = plain(&draw_bar(config_of(&tray), &named, 200));
    assert!(
        line.ends_with(" LOCKED   my-session   09:41:27 "),
        "{line:?}"
    );
    // The tray's own options apply.
    let unshown = [("tray", "session clock"), ("show_session", "false")];
    assert!(!previewed(&unshown, 200).contains("work"));
    let offset = [("tray", "clock"), ("clock_utc_offset", "-05:30")];
    assert!(previewed(&offset, 200).ends_with(" 04:11 "));
}

#[test]
fn the_preview_s_clock_asks_nothing_of_the_system_and_draws_the_same_each_time() {
    for pairs in [&[][..], &[("tray", "mode session clock")]] {
        let state = preview_state(config_of(pairs), &mode_info(), 200);
        assert_eq!(state.host.calls(), Vec::new(), "{pairs:?}");
        assert_eq!(state.clock.text.as_deref(), Some("09:41"));
    }
    assert_eq!(
        draw_bar(BTreeMap::new(), &mode_info(), 200),
        draw_bar(BTreeMap::new(), &mode_info(), 200)
    );
}

#[test]
fn the_preview_draws_every_theme_combination_with_nothing_left_raw() {
    let mode_info = mode_info();
    for combination in every_combination() {
        for tray in ["clock", "mode session clock"] {
            let mut configuration = config_of(&combination);
            configuration.insert("tray".to_string(), tray.to_string());
            for width in [200, 40] {
                let line = draw_bar(configuration.clone(), &mode_info, width);
                let case = format!("{combination:?} {tray:?} {width}");
                assert!(!line.is_empty(), "{case}");
                if let Some(raw) = left_raw(&line) {
                    panic!("{case}: left {raw}");
                }
                let text = plain(&line);
                assert!(text.trim_end().ends_with("09:41"), "{case}: {text:?}");
                if tray == "clock" {
                    assert!(text.contains("editor"), "{case}: {text:?}");
                }
            }
        }
    }
}

#[test]
fn the_themes_change_the_preview_and_the_overrides_are_left_out_on_request() {
    let mode_info = mode_info();
    let themed = config_of(&[("format_theme", "plain"), ("entry_spacer", "|")]);
    let with = theme_preview(&SCHEMA, &themed, &mode_info, true, 80);
    let without = theme_preview(&SCHEMA, &themed, &mode_info, false, 80);
    assert!(plain(&with).contains('|'), "{with:?}");
    assert!(!plain(&without).contains('|'), "{without:?}");
    assert_ne!(
        with,
        theme_preview(&SCHEMA, &BTreeMap::new(), &mode_info, true, 80)
    );
}
