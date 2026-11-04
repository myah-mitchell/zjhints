//! Every option the bar reads, as the settings screen presents it.
//!
//! This is the one list of options in the code. Tests hold it to the bar's
//! own `CONFIG_*` and `DEFAULT_*` constants, so an option added to the bar
//! and not here, or a default changed in one place only, fails the build
//! rather than going missing from the screen or showing the wrong default.
//! They also hold it to the configuration reference
//! (docs/zjtaskbar/configuration.md), so an option added to one and not the
//! other fails the build too.

use crate::entries::PANE_SCOPE_CHOICES;
use zjkit::settings::schema::{family, option, FamilySpec, Kind, OptionSpec};
use zjkit::theme::{ALIAS_CHOICES, COLOR_CHOICES, FORMAT_CHOICES};

/// A heading the options are grouped under, in the order they are shown.
///
/// A group may hold fixed options, families, or both. Adding one takes its
/// name here and its options or families naming it as their group.
pub const GROUPS: &[&str] = &[
    "Entries", "Flags", "Overflow", "Theme", "Colors", "Behavior", "Mouse", "Launcher", "Tray",
    "Clock", "Session", "Mode",
];

pub const OPTIONS: &[OptionSpec] = &[
    option(
        "pane_scope",
        "Entries",
        Kind::Choice(&PANE_SCOPE_CHOICES),
        Some("all"),
        "List the floating panes of every tab, or of the active tab only",
    ),
    option(
        "entry_format",
        "Entries",
        Kind::Format,
        None,
        "Format string for each entry neither focused nor minimized, Zellij's ribbon colors when unset",
    ),
    option(
        "focused_entry_format",
        "Entries",
        Kind::Format,
        None,
        "Format string for the focused entry, Zellij's ribbon colors when unset",
    ),
    option(
        "minimized_entry_format",
        "Entries",
        Kind::Format,
        None,
        "Format string for a minimized entry, Zellij's text colors when unset",
    ),
    option(
        "entry_spacer",
        "Entries",
        Kind::Format,
        Some(" "),
        "Format string drawn between entries of one tab and beside markers, empty for no gap",
    ),
    option(
        "tab_separator",
        "Entries",
        Kind::Format,
        Some(" \u{2502} "),
        "Format string drawn between the entries of two tabs, in place of the spacer",
    ),
    option(
        "empty_format",
        "Entries",
        Kind::Format,
        None,
        "Format string drawn while there are no floating panes, nothing when unset",
    ),
    option(
        "minimized_format",
        "Flags",
        Kind::Format,
        Some(" (MINIMIZED)"),
        "Format string for the flag of a minimized pane, empty to hide it",
    ),
    option(
        "exited_format",
        "Flags",
        Kind::Format,
        Some(" (EXITED)"),
        "Format string for the flag of a pane whose command has finished, empty to hide it",
    ),
    option(
        "plugin_format",
        "Flags",
        Kind::Format,
        Some(" (PLUGIN)"),
        "Format string for the flag of a pane that runs a plugin, empty to hide it",
    ),
    option(
        "other_tab_format",
        "Flags",
        Kind::Format,
        Some(" ({tab})"),
        "Format string for the flag of a pane in another tab, with {tab}, empty to hide it",
    ),
    option(
        "auto_width",
        "Overflow",
        Kind::Bool,
        Some("true"),
        "Fit the entries to the bar's own pane, keeping the focused entry on screen",
    ),
    option(
        "max_length",
        "Overflow",
        Kind::Integer,
        Some("0"),
        "Hard cap on the line's width in columns, 0 for unlimited",
    ),
    option(
        "ambiguous_width",
        "Overflow",
        Kind::Choice(&["1", "2"]),
        Some("1"),
        "Columns an East Asian Ambiguous character occupies",
    ),
    option(
        "overflow_left_format",
        "Overflow",
        Kind::Format,
        None,
        "Format string for the marker of entries hidden before the shown ones, with {count}",
    ),
    option(
        "overflow_right_format",
        "Overflow",
        Kind::Format,
        None,
        "Format string for the marker of entries hidden after the shown ones, with {count}",
    ),
    option(
        "overflow_str",
        "Overflow",
        Kind::Format,
        Some("..."),
        "Format string appended when a lone entry is too wide to fit and must be cut",
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
        "The shape of entries, markers, separators and the launcher's button: Zellij's ribbons or a built-in format",
    ),
    option(
        "alias_theme",
        "Theme",
        Kind::Choice(&ALIAS_CHOICES),
        Some("zellij"),
        "The symbols for the flags and the launcher's button: the bar's own words or a built-in set",
    ),
    option(
        "hidden",
        "Behavior",
        Kind::Bool,
        Some("false"),
        "Start with the bar hidden; only a zjtaskbar::show or zjtaskbar::toggle keybinding or zellij pipe brings it back",
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
        "Dim the entries while this session is not the one receiving input",
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
        "Click an entry to focus, minimize or restore its pane, scroll to step through them, double-click to rename, middle-click to close, right-click for a menu",
    ),
    option(
        "mouse_wheel_reversed",
        "Mouse",
        Kind::Bool,
        Some("false"),
        "Scroll up for the previous pane and down for the next, the reverse of Zellij's own tab bar",
    ),
    option(
        "mouse_close_confirm",
        "Mouse",
        Kind::Bool,
        Some("true"),
        "Ask before a middle click or the pane menu closes a pane",
    ),
    option(
        "show_launcher",
        "Launcher",
        Kind::Bool,
        Some("true"),
        "Draw a button at the bar's left end whose menu opens new floating panes, while mouse is on",
    ),
    option(
        "launcher_format",
        "Launcher",
        Kind::Format,
        None,
        "Format string for the launcher's button, with {symbol}, Zellij's ribbon colors when unset",
    ),
    option(
        "launcher_symbol",
        "Launcher",
        Kind::Text,
        Some("Menu"),
        "What {symbol} draws in the launcher's button",
    ),
    option(
        "launcher_new_terminal",
        "Launcher",
        Kind::Bool,
        Some("true"),
        "List a new terminal first in the launcher's menu",
    ),
    option(
        "launcher_zellij_actions",
        "Launcher",
        Kind::Bool,
        Some("true"),
        "List Zellij's session manager, settings and plugin manager last in the launcher's menu",
    ),
    option(
        "tray",
        "Tray",
        Kind::Text,
        None,
        "The items at the bar's right end, in order: any of mode, session and clock; none when unset",
    ),
    option(
        "tray_spacer",
        "Tray",
        Kind::Format,
        Some(" "),
        "Format string drawn between the tray's items and before the tray, empty for no gap",
    ),
    option(
        "clock_format",
        "Clock",
        Kind::Text,
        Some("%H:%M"),
        "The time the clock shows, with strftime specifiers such as %H, %M and %a",
    ),
    option(
        "clock_item_format",
        "Clock",
        Kind::Format,
        None,
        "Format string for the clock, with {clock}, Zellij's ribbon colors when unset",
    ),
    option(
        "clock_utc_offset",
        "Clock",
        Kind::Text,
        None,
        "The local time's offset from UTC, as +02:00 or -0530; asked of the system when unset",
    ),
    option(
        "show_session",
        "Session",
        Kind::Bool,
        Some("true"),
        "Draw the session item while session is in the tray; while unset, Zellij's hide_session_name hides it",
    ),
    option(
        "session_format",
        "Session",
        Kind::Format,
        None,
        "Format string for the session item, with {session}, Zellij's selected ribbon colors when unset",
    ),
    option(
        "mode_format",
        "Mode",
        Kind::Format,
        None,
        "Format string for the mode item, with {desc}, {key} and {mode}, a block in the selected ribbon's colors when unset",
    ),
];

pub const FAMILIES: &[FamilySpec] = &[
    family(
        "mode_format_<mode>",
        "Mode",
        Kind::Format,
        "The mode item's format string for one mode",
    ),
    family(
        "mode_label_<mode>",
        "Mode",
        Kind::Text,
        "A mode's name, drawn for {desc} in the mode item",
    ),
    family(
        "mode_key_<mode>",
        "Mode",
        Kind::Text,
        "A mode's symbol, drawn for {key} in the mode item",
    ),
    family(
        "color_<name>",
        "Colors",
        Kind::Text,
        "A color alias, used as $name in format strings",
    ),
    family(
        "launch_<n>_name",
        "Launcher",
        Kind::Text,
        "What the launcher's menu shows for entry n, listed in the order of n",
    ),
    family(
        "launch_<n>_command",
        "Launcher",
        Kind::Text,
        "The program entry n runs in a new floating pane",
    ),
    family(
        "launch_<n>_args",
        "Launcher",
        Kind::Text,
        "The arguments of entry n's command, split as a shell splits them",
    ),
    family(
        "launch_<n>_cwd",
        "Launcher",
        Kind::Text,
        "The directory entry n's command runs in, the focused pane's when unset",
    ),
    family(
        "launch_<n>_plugin",
        "Launcher",
        Kind::Text,
        "The plugin URL or alias entry n opens in a new floating pane",
    ),
    family(
        "launch_<n>_config_<key>",
        "Launcher",
        Kind::Text,
        "One key of the configuration entry n's plugin opens with",
    ),
];

/// Options of the settings screen itself, read only when it is opened. They
/// configure the screen rather than the bar, so they are not listed on it.
pub const SCREEN_OPTIONS: &[&str] = &["settings", "alias"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flags::FLAGS;
    use crate::settings::SCHEMA;
    use crate::{
        CONFIG_AUTO_WIDTH, CONFIG_COLLAPSE_WHEN_EMPTY, CONFIG_DIM_STRENGTH,
        CONFIG_DIM_WHEN_UNFOCUSED, CONFIG_EMPTY_FORMAT, CONFIG_ENTRY_FORMAT, CONFIG_ENTRY_SPACER,
        CONFIG_FOCUSED_ENTRY_FORMAT, CONFIG_HIDE_WHEN_NESTED, CONFIG_LAUNCHER_FORMAT,
        CONFIG_LAUNCHER_NEW_TERMINAL, CONFIG_LAUNCHER_SYMBOL, CONFIG_LAUNCHER_ZELLIJ_ACTIONS,
        CONFIG_MAX_LENGTH, CONFIG_MINIMIZED_ENTRY_FORMAT, CONFIG_MOUSE, CONFIG_MOUSE_CLOSE_CONFIRM,
        CONFIG_MOUSE_WHEEL_REVERSED, CONFIG_OVERFLOW_LEFT_FORMAT, CONFIG_OVERFLOW_RIGHT_FORMAT,
        CONFIG_OVERFLOW_STR, CONFIG_PANE_SCOPE, CONFIG_SHOW_LAUNCHER, CONFIG_TAB_SEPARATOR,
        DEFAULT_AUTO_WIDTH, DEFAULT_COLLAPSE_WHEN_EMPTY, DEFAULT_DIM_STRENGTH,
        DEFAULT_DIM_WHEN_UNFOCUSED, DEFAULT_ENTRY_SPACER, DEFAULT_HIDE_WHEN_NESTED,
        DEFAULT_LAUNCHER_NEW_TERMINAL, DEFAULT_LAUNCHER_SYMBOL, DEFAULT_LAUNCHER_ZELLIJ_ACTIONS,
        DEFAULT_MAX_LENGTH, DEFAULT_MOUSE, DEFAULT_MOUSE_CLOSE_CONFIRM,
        DEFAULT_MOUSE_WHEEL_REVERSED, DEFAULT_OVERFLOW_STR, DEFAULT_PANE_SCOPE,
        DEFAULT_SHOW_LAUNCHER, DEFAULT_TAB_SEPARATOR,
    };
    use crate::{
        CONFIG_CLOCK_FORMAT, CONFIG_CLOCK_ITEM_FORMAT, CONFIG_CLOCK_UTC_OFFSET,
        CONFIG_SESSION_FORMAT, CONFIG_SHOW_SESSION, CONFIG_TRAY, CONFIG_TRAY_SPACER,
        DEFAULT_CLOCK_FORMAT, DEFAULT_SHOW_SESSION, DEFAULT_TRAY, DEFAULT_TRAY_SPACER,
    };
    use zjkit::settings::reference;

    const REFERENCE: &str = include_str!("../../../../docs/zjtaskbar/configuration.md");

    /// What an option's default should read, from the constant the bar falls
    /// back on, or `None` for an option that is unset by default. Every
    /// option the bar reads is named here, so adding one to the bar and
    /// forgetting the screen, or the other way around, fails a test.
    fn bar_defaults() -> Vec<(&'static str, Option<String>)> {
        let mut defaults = vec![
            (CONFIG_PANE_SCOPE, Some(DEFAULT_PANE_SCOPE.to_string())),
            (CONFIG_ENTRY_FORMAT, None),
            (CONFIG_FOCUSED_ENTRY_FORMAT, None),
            (CONFIG_MINIMIZED_ENTRY_FORMAT, None),
            (CONFIG_ENTRY_SPACER, Some(DEFAULT_ENTRY_SPACER.to_string())),
            (
                CONFIG_TAB_SEPARATOR,
                Some(DEFAULT_TAB_SEPARATOR.to_string()),
            ),
            (CONFIG_EMPTY_FORMAT, None),
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
            (CONFIG_MOUSE, Some(DEFAULT_MOUSE.to_string())),
            (
                CONFIG_MOUSE_WHEEL_REVERSED,
                Some(DEFAULT_MOUSE_WHEEL_REVERSED.to_string()),
            ),
            (
                CONFIG_MOUSE_CLOSE_CONFIRM,
                Some(DEFAULT_MOUSE_CLOSE_CONFIRM.to_string()),
            ),
            (
                CONFIG_SHOW_LAUNCHER,
                Some(DEFAULT_SHOW_LAUNCHER.to_string()),
            ),
            (CONFIG_LAUNCHER_FORMAT, None),
            (
                CONFIG_LAUNCHER_SYMBOL,
                Some(DEFAULT_LAUNCHER_SYMBOL.to_string()),
            ),
            (
                CONFIG_LAUNCHER_NEW_TERMINAL,
                Some(DEFAULT_LAUNCHER_NEW_TERMINAL.to_string()),
            ),
            (
                CONFIG_LAUNCHER_ZELLIJ_ACTIONS,
                Some(DEFAULT_LAUNCHER_ZELLIJ_ACTIONS.to_string()),
            ),
            // An empty tray is written as unset, as every option that draws
            // nothing by default is.
            (
                CONFIG_TRAY,
                Some(DEFAULT_TRAY.to_string()).filter(|tray| !tray.is_empty()),
            ),
            (CONFIG_TRAY_SPACER, Some(DEFAULT_TRAY_SPACER.to_string())),
            (CONFIG_CLOCK_FORMAT, Some(DEFAULT_CLOCK_FORMAT.to_string())),
            (CONFIG_CLOCK_ITEM_FORMAT, None),
            (CONFIG_CLOCK_UTC_OFFSET, None),
            (CONFIG_SHOW_SESSION, Some(DEFAULT_SHOW_SESSION.to_string())),
            (CONFIG_SESSION_FORMAT, None),
            (zjkit::mode::CONFIG_MODE_FORMAT, None),
            // The options zjkit reads for every bar: how wide an ambiguous
            // character is, whether the bar starts hidden, and the themes.
            (zjkit::config::CONFIG_AMBIGUOUS_WIDTH, Some("1".to_string())),
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
    fn each_field_of_a_launcher_entry_is_a_family_of_its_own() {
        let pattern_of = |key: &str| SCHEMA.family_of(key).map(|family| family.pattern);
        for (key, pattern) in [
            ("launch_1_name", "launch_<n>_name"),
            ("launch_10_command", "launch_<n>_command"),
            ("launch_2_args", "launch_<n>_args"),
            ("launch_2_cwd", "launch_<n>_cwd"),
            ("launch_3_plugin", "launch_<n>_plugin"),
            ("launch_3_config_theme", "launch_<n>_config_<key>"),
            // A plugin's own key named as an entry's field is still its key.
            ("launch_3_config_name", "launch_<n>_config_<key>"),
            ("launch_3_config_command", "launch_<n>_config_<key>"),
            ("launch_0_name", "launch_<n>_name"),
        ] {
            assert_eq!(pattern_of(key), Some(pattern), "{key}");
            assert_eq!(SCHEMA.group_of(key), Some("Launcher"), "{key}");
        }
        // Keys the launcher reads no entry from. Each starts like a field's
        // key, so the screen lists it under Launcher, warning that it fits
        // no field.
        for key in [
            "launch_1_bogus",
            "launch_name",
            "launch_01_name",
            "launch_one_name",
            "launch_1_2_command",
            "launch_18446744073709551616_name",
            "launch_1",
        ] {
            assert_eq!(pattern_of(key), None, "{key}");
            assert!(SCHEMA.misfit_family(key).is_some(), "{key}");
            assert_eq!(SCHEMA.group_of(key), Some("Launcher"), "{key}");
        }
        assert_eq!(SCHEMA.patterns_with_prefix("launch_"), "launch_<n>_*");
        // A key that does not start with `launch_` is no field's at all.
        assert_eq!(pattern_of("launcher_bogus"), None);
        assert!(SCHEMA.misfit_family("launcher_bogus").is_none());
        assert_eq!(SCHEMA.group_of("launcher_bogus"), None);
    }

    #[test]
    fn per_mode_keys_are_a_family_of_the_mode_group_for_known_modes_only() {
        for (key, kind) in [
            ("mode_format_locked", Kind::Format),
            ("mode_label_enter_search", Kind::Text),
            ("mode_key_rename_tab", Kind::Text),
        ] {
            assert_eq!(SCHEMA.group_of(key), Some("Mode"), "{key}");
            assert_eq!(SCHEMA.kind_of(key), kind, "{key}");
        }
        // A word that is no mode of Zellij's fits none, but is listed with
        // the family it starts like, which the screen warns about.
        assert!(SCHEMA.family_of("mode_format_bogus").is_none());
        assert_eq!(SCHEMA.group_of("mode_format_bogus"), Some("Mode"));
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
            "entry_format",
            "focused_entry_format",
            "minimized_entry_format",
            "entry_spacer",
            "tab_separator",
            "empty_format",
            "minimized_format",
            "exited_format",
            "plugin_format",
            "other_tab_format",
            "overflow_left_format",
            "overflow_right_format",
            "overflow_str",
            "launcher_format",
            "tray_spacer",
            "clock_item_format",
            "session_format",
            "mode_format",
            "mode_format_normal",
        ] {
            assert_eq!(SCHEMA.kind_of(name), Kind::Format, "{name}");
        }
        // strftime specifiers, not a format string's placeholders.
        assert_eq!(SCHEMA.kind_of("clock_format"), Kind::Text);
        assert_eq!(SCHEMA.kind_of("clock_utc_offset"), Kind::Text);
        assert_eq!(SCHEMA.kind_of("tray"), Kind::Text);
        assert_eq!(SCHEMA.kind_of("max_length"), Kind::Integer);
        assert_eq!(SCHEMA.kind_of("dim_strength"), Kind::Fraction);
        assert!(matches!(SCHEMA.kind_of("pane_scope"), Kind::Choice(_)));
        assert_eq!(SCHEMA.kind_of("launcher_symbol"), Kind::Text);
    }
}
