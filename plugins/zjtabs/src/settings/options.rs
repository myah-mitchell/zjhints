//! Every option the bar reads, as the settings screen presents it.
//!
//! This is the one list of options in the code. Tests hold it to the bar's own
//! `CONFIG_*` and `DEFAULT_*` constants, so an option added to the bar and not
//! here, or a default changed in one place only, fails the build rather than
//! going missing from the screen or showing the wrong default. They also hold
//! it to the configuration reference (docs/zjtabs/configuration.md), so an
//! option added to one and not the other fails the build too.

use zjkit::settings::schema::{family, option, FamilySpec, Kind, OptionSpec};
use zjkit::theme::{ALIAS_CHOICES, COLOR_CHOICES, FORMAT_CHOICES};

/// A heading the options are grouped under, in the order they are shown.
///
/// A group may hold fixed options, families, or both. Adding one takes its
/// name here and its options or families naming it as their group.
pub const GROUPS: &[&str] = &[
    "Tabs",
    "Flags",
    "Mode prefix",
    "Session name",
    "Overflow",
    "Theme",
    "Colors",
    "Behavior",
    "Mouse",
];

pub const OPTIONS: &[OptionSpec] = &[
    option(
        "tab_format",
        "Tabs",
        Kind::Format,
        None,
        "Format string for each tab but the active one, Zellij's ribbon colors when unset",
    ),
    option(
        "active_tab_format",
        "Tabs",
        Kind::Format,
        None,
        "Format string for the active tab, Zellij's ribbon colors when unset",
    ),
    option(
        "tab_spacer",
        "Tabs",
        Kind::Format,
        Some(" "),
        "Format string drawn between tabs and markers, empty for no gap",
    ),
    option(
        "index_from_zero",
        "Tabs",
        Kind::Bool,
        Some("false"),
        "Number tabs from 0 in {index}, rather than from 1",
    ),
    option(
        "rename_placeholder",
        "Tabs",
        Kind::Text,
        Some("Enter name..."),
        "Name drawn for the tab being renamed while its new name is still empty",
    ),
    option(
        "fullscreen_format",
        "Flags",
        Kind::Format,
        Some(" (FULLSCREEN)"),
        "Format string for the flag of a tab with a fullscreen pane, empty to hide it",
    ),
    option(
        "sync_format",
        "Flags",
        Kind::Format,
        Some(" (SYNC)"),
        "Format string for the flag of a tab whose panes all take the same input, empty to hide it",
    ),
    option(
        "floating_format",
        "Flags",
        Kind::Format,
        Some(" (FLOATING)"),
        "Format string for the flag of a tab showing its floating panes, empty to hide it",
    ),
    option(
        "bell_format",
        "Flags",
        Kind::Format,
        Some(" \u{25cf}"),
        "Format string for the flag of a tab that rang the bell or needs you, empty to hide it",
    ),
    option(
        "show_mode",
        "Mode prefix",
        Kind::Bool,
        Some("true"),
        "Draw the current input mode before the tabs",
    ),
    option(
        "mode_format",
        "Mode prefix",
        Kind::Format,
        None,
        "Format string for the mode prefix, with {desc}, {key} and {mode} placeholders",
    ),
    option(
        "show_session",
        "Session name",
        Kind::Bool,
        Some("true"),
        "Draw the session's name at the end of the bar, after the tabs; while unset, Zellij's hide_session_name hides it",
    ),
    option(
        "session_format",
        "Session name",
        Kind::Format,
        None,
        "Format string for the session name, with a {session} placeholder",
    ),
    option(
        "auto_width",
        "Overflow",
        Kind::Bool,
        Some("true"),
        "Fit the tabs to the bar's own pane, keeping the active tab on screen",
    ),
    option(
        "max_length",
        "Overflow",
        Kind::Integer,
        Some("0"),
        "Hard cap on the line's width in columns, 0 for unlimited",
    ),
    option(
        "overflow_left_format",
        "Overflow",
        Kind::Format,
        None,
        "Format string for the marker of tabs hidden before the shown ones, with {count}",
    ),
    option(
        "overflow_right_format",
        "Overflow",
        Kind::Format,
        None,
        "Format string for the marker of tabs hidden after the shown ones, with {count}",
    ),
    option(
        "overflow_str",
        "Overflow",
        Kind::Format,
        Some("..."),
        "Format string appended when a lone tab is too wide to fit and must be cut mid-tab",
    ),
    option(
        "color_theme",
        "Theme",
        Kind::Choice(&COLOR_CHOICES),
        Some("zellij"),
        "The colors named $bg, $accent and so on: Zellij's, none, or a built-in palette",
    ),
    option(
        "format_theme",
        "Theme",
        Kind::Choice(&FORMAT_CHOICES),
        Some("zellij"),
        "The shape of tabs, markers, the mode prefix and the session name: Zellij's ribbons or a built-in format",
    ),
    option(
        "alias_theme",
        "Theme",
        Kind::Choice(&ALIAS_CHOICES),
        Some("zellij"),
        "The symbols for the flags and modes: Zellij's words or a built-in set",
    ),
    option(
        "hidden",
        "Behavior",
        Kind::Bool,
        Some("false"),
        "Start with the bar hidden; only a zjtabs::show or zjtabs::toggle keybinding or zellij pipe brings it back",
    ),
    option(
        "hide_when_nested",
        "Behavior",
        Kind::Bool,
        Some("true"),
        "Render nothing while this session is nested inside another",
    ),
    option(
        "collapse_when_empty",
        "Behavior",
        Kind::Bool,
        Some("true"),
        "Hand the layout row back while the bar has nothing to draw",
    ),
    option(
        "dim_when_unfocused",
        "Behavior",
        Kind::Bool,
        Some("true"),
        "Dim the tabs while this session is not the one receiving input",
    ),
    option(
        "dim_strength",
        "Behavior",
        Kind::Fraction,
        Some("0.5"),
        "How strongly to dim, from 0.0 to 1.0",
    ),
    option(
        "mouse",
        "Mouse",
        Kind::Bool,
        Some("true"),
        "Click a tab to switch to it, scroll to step through tabs, double-click to rename, middle-click to close, right-click for the menu",
    ),
    option(
        "mouse_wheel_reversed",
        "Mouse",
        Kind::Bool,
        Some("false"),
        "Scroll up for the previous tab and down for the next, instead of Zellij's own way round",
    ),
    option(
        "mouse_close_confirm",
        "Mouse",
        Kind::Bool,
        Some("true"),
        "Ask before a middle click closes a tab",
    ),
];

pub const FAMILIES: &[FamilySpec] = &[
    family(
        "mode_format_<mode>",
        "Mode prefix",
        Kind::Format,
        "The mode prefix's format string for one mode",
    ),
    family(
        "mode_label_<mode>",
        "Mode prefix",
        Kind::Text,
        "A mode's name, drawn for {desc} in the mode prefix",
    ),
    family(
        "mode_key_<mode>",
        "Mode prefix",
        Kind::Text,
        "A mode's symbol, drawn for {key} in the mode prefix",
    ),
    family(
        "color_<name>",
        "Colors",
        Kind::Text,
        "A color alias, used as $name in format strings",
    ),
];

/// Options of the settings screen itself, read only when it is opened. They
/// configure the screen rather than the bar, so they are not listed on it.
pub const SCREEN_OPTIONS: &[&str] = &["settings", "alias"];

/// Options the bar accepts but ignores, so a config setting one still loads.
/// The settings screen says each has no effect, and the configuration
/// reference lists them under "Deprecated".
///
/// `ambiguous_width` asked for East Asian Ambiguous characters to be measured
/// two columns wide. Zellij lays a plugin's pane out with them one column
/// wide whatever the terminal draws, so the bar always measures them as one.
pub const DEPRECATED_OPTIONS: &[&str] = &["ambiguous_width"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flags::FLAGS;
    use crate::settings::SCHEMA;
    use crate::{
        CONFIG_ACTIVE_TAB_FORMAT, CONFIG_AUTO_WIDTH, CONFIG_COLLAPSE_WHEN_EMPTY,
        CONFIG_DIM_STRENGTH, CONFIG_DIM_WHEN_UNFOCUSED, CONFIG_HIDE_WHEN_NESTED,
        CONFIG_INDEX_FROM_ZERO, CONFIG_MAX_LENGTH, CONFIG_MOUSE, CONFIG_MOUSE_CLOSE_CONFIRM,
        CONFIG_MOUSE_WHEEL_REVERSED, CONFIG_OVERFLOW_LEFT_FORMAT, CONFIG_OVERFLOW_RIGHT_FORMAT,
        CONFIG_OVERFLOW_STR, CONFIG_RENAME_PLACEHOLDER, CONFIG_SESSION_FORMAT, CONFIG_SHOW_MODE,
        CONFIG_SHOW_SESSION, CONFIG_TAB_FORMAT, CONFIG_TAB_SPACER, DEFAULT_AUTO_WIDTH,
        DEFAULT_COLLAPSE_WHEN_EMPTY, DEFAULT_DIM_STRENGTH, DEFAULT_DIM_WHEN_UNFOCUSED,
        DEFAULT_HIDE_WHEN_NESTED, DEFAULT_INDEX_FROM_ZERO, DEFAULT_MAX_LENGTH, DEFAULT_MOUSE,
        DEFAULT_MOUSE_CLOSE_CONFIRM, DEFAULT_MOUSE_WHEEL_REVERSED, DEFAULT_OVERFLOW_STR,
        DEFAULT_RENAME_PLACEHOLDER, DEFAULT_SHOW_MODE, DEFAULT_SHOW_SESSION, DEFAULT_TAB_SPACER,
    };
    use zjkit::settings::reference;

    const REFERENCE: &str = include_str!("../../../../docs/zjtabs/configuration.md");

    /// What an option's default should read, from the constant the bar falls
    /// back on, or `None` for an option that is unset by default. Every
    /// option the bar reads is named here, so adding one to the bar and
    /// forgetting the screen, or the other way around, fails a test.
    fn bar_defaults() -> Vec<(&'static str, Option<String>)> {
        let mut defaults = vec![
            (CONFIG_TAB_FORMAT, None),
            (CONFIG_ACTIVE_TAB_FORMAT, None),
            (CONFIG_TAB_SPACER, Some(DEFAULT_TAB_SPACER.to_string())),
            (
                CONFIG_INDEX_FROM_ZERO,
                Some(DEFAULT_INDEX_FROM_ZERO.to_string()),
            ),
            (
                CONFIG_RENAME_PLACEHOLDER,
                Some(DEFAULT_RENAME_PLACEHOLDER.to_string()),
            ),
            (CONFIG_OVERFLOW_LEFT_FORMAT, None),
            (CONFIG_OVERFLOW_RIGHT_FORMAT, None),
            (CONFIG_OVERFLOW_STR, Some(DEFAULT_OVERFLOW_STR.to_string())),
            (CONFIG_AUTO_WIDTH, Some(DEFAULT_AUTO_WIDTH.to_string())),
            (CONFIG_MAX_LENGTH, Some(DEFAULT_MAX_LENGTH.to_string())),
            (
                CONFIG_HIDE_WHEN_NESTED,
                Some(DEFAULT_HIDE_WHEN_NESTED.to_string()),
            ),
            (
                CONFIG_COLLAPSE_WHEN_EMPTY,
                Some(DEFAULT_COLLAPSE_WHEN_EMPTY.to_string()),
            ),
            (
                CONFIG_DIM_WHEN_UNFOCUSED,
                Some(DEFAULT_DIM_WHEN_UNFOCUSED.to_string()),
            ),
            (CONFIG_DIM_STRENGTH, Some(DEFAULT_DIM_STRENGTH.to_string())),
            (CONFIG_SHOW_MODE, Some(DEFAULT_SHOW_MODE.to_string())),
            (zjkit::mode::CONFIG_MODE_FORMAT, None),
            (CONFIG_SHOW_SESSION, Some(DEFAULT_SHOW_SESSION.to_string())),
            (CONFIG_SESSION_FORMAT, None),
            (CONFIG_MOUSE, Some(DEFAULT_MOUSE.to_string())),
            (
                CONFIG_MOUSE_WHEEL_REVERSED,
                Some(DEFAULT_MOUSE_WHEEL_REVERSED.to_string()),
            ),
            (
                CONFIG_MOUSE_CLOSE_CONFIRM,
                Some(DEFAULT_MOUSE_CLOSE_CONFIRM.to_string()),
            ),
            // The options zjkit reads for every bar: whether the bar starts
            // hidden, and the themes.
            (zjkit::visibility::OPTION, Some("false".to_string())),
        ];
        for option in zjkit::theme::OPTIONS {
            defaults.push((option, Some(zjkit::theme::THEME_ZELLIJ.to_string())));
        }
        defaults.extend(
            FLAGS
                .iter()
                .map(|flag| (flag.option, Some(flag.default.to_string()))),
        );
        defaults
    }

    #[test]
    fn every_option_the_bar_reads_is_listed_with_the_default_it_falls_back_on() {
        let defaults = bar_defaults();
        for (name, default) in &defaults {
            let listed = SCHEMA
                .find(name)
                .unwrap_or_else(|| panic!("{name} is not on the settings screen"));
            assert_eq!(listed.default.map(str::to_string), *default, "{name}");
        }
        for option in OPTIONS {
            assert!(
                defaults.iter().any(|(name, _)| *name == option.name),
                "{} is on the settings screen but the bar does not read it",
                option.name
            );
        }
        assert_eq!(OPTIONS.len(), defaults.len());
    }

    #[test]
    fn every_documented_option_is_in_the_table_and_no_other() {
        reference::assert_names_match(REFERENCE, &SCHEMA);
    }

    #[test]
    fn defaults_and_groups_match_the_reference() {
        reference::assert_defaults_and_groups_match(REFERENCE, &SCHEMA);
    }

    #[test]
    fn groups_are_in_reference_order() {
        reference::assert_group_order_matches(REFERENCE, &SCHEMA);
    }

    #[test]
    fn every_option_has_a_unique_name_a_known_group_and_a_description() {
        for (index, option) in OPTIONS.iter().enumerate() {
            assert!(GROUPS.contains(&option.group), "{}", option.name);
            assert!(!option.description.is_empty(), "{}", option.name);
            assert!(
                OPTIONS[..index]
                    .iter()
                    .all(|other| other.name != option.name),
                "{} is listed twice",
                option.name
            );
        }
        for family in FAMILIES {
            assert!(GROUPS.contains(&family.group), "{}", family.pattern);
        }
        for group in GROUPS {
            let has_options = OPTIONS.iter().any(|option| option.group == *group);
            let has_families = FAMILIES.iter().any(|family| family.group == *group);
            assert!(has_options || has_families, "{group} is empty");
        }
    }

    #[test]
    fn choice_defaults_are_among_their_choices() {
        for option in OPTIONS {
            if let (Kind::Choice(choices), Some(default)) = (option.kind, option.default) {
                assert!(choices.contains(&default), "{}", option.name);
            }
        }
    }

    #[test]
    fn defaults_are_valid_for_their_kind() {
        for option in OPTIONS {
            if let Some(default) = option.default {
                assert_eq!(
                    zjkit::settings::schema::validate(option.kind, default),
                    Ok(()),
                    "{}",
                    option.name
                );
            }
        }
    }

    #[test]
    fn color_aliases_are_a_family_of_the_colors_group() {
        assert_eq!(SCHEMA.group_of("color_blue"), Some("Colors"));
        assert_eq!(SCHEMA.kind_of("color_blue"), Kind::Text);
        assert_eq!(SCHEMA.group_of("something_else"), None);
    }

    #[test]
    fn per_mode_keys_are_a_family_of_the_mode_prefix_group_for_known_modes_only() {
        for (key, kind) in [
            ("mode_format_locked", Kind::Format),
            ("mode_label_enter_search", Kind::Text),
            ("mode_key_rename_tab", Kind::Text),
        ] {
            assert_eq!(SCHEMA.group_of(key), Some("Mode prefix"), "{key}");
            assert_eq!(SCHEMA.kind_of(key), kind, "{key}");
        }
        // A word that is no mode of Zellij's fits none, but is listed with
        // the family it starts like, which the screen warns about.
        assert!(SCHEMA.family_of("mode_format_bogus").is_none());
        assert_eq!(SCHEMA.group_of("mode_format_bogus"), Some("Mode prefix"));
        assert_eq!(
            SCHEMA
                .misfit_family("mode_label_bogus")
                .map(|family| family.pattern),
            Some("mode_label_<mode>")
        );
    }

    #[test]
    fn formats_are_told_apart_from_plain_text() {
        for name in [
            "tab_format",
            "active_tab_format",
            "tab_spacer",
            "fullscreen_format",
            "sync_format",
            "floating_format",
            "bell_format",
            "overflow_left_format",
            "overflow_right_format",
            "overflow_str",
            "mode_format",
            "mode_format_normal",
            "session_format",
        ] {
            assert_eq!(SCHEMA.kind_of(name), Kind::Format, "{name}");
        }
        assert_eq!(SCHEMA.kind_of("rename_placeholder"), Kind::Text);
        assert_eq!(SCHEMA.kind_of("max_length"), Kind::Integer);
        assert_eq!(SCHEMA.kind_of("dim_strength"), Kind::Fraction);
    }
}
