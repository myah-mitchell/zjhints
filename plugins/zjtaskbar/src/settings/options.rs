//! Every option the bar reads, as the settings screen presents it.
//!
//! This is the one list of options in the code. Tests hold it to the bar's
//! own `CONFIG_*` and `DEFAULT_*` constants, so an option added to the bar
//! and not here, or a default changed in one place only, fails the build
//! rather than going missing from the screen or showing the wrong default.

use crate::entries::PANE_SCOPE_CHOICES;
use zjkit::settings::schema::{family, option, FamilySpec, Kind, OptionSpec};
use zjkit::theme::{ALIAS_CHOICES, COLOR_CHOICES, FORMAT_CHOICES};

/// A heading the options are grouped under, in the order they are shown:
/// zjtaskbar's own first, in the order the bar draws what they set up. Then
/// Theme, Colors, Behavior and Mouse, where zjtabs shows them, so the groups
/// the bars share sit in the same place in each.
///
/// A group may hold fixed options, families, or both. Adding one takes its
/// name here and its options or families naming it as their group.
pub const GROUPS: &[&str] = &[
    "Entries", "Flags", "Overflow", "Theme", "Colors", "Behavior", "Mouse",
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
        None,
        "Format string for the flag of a pane in another tab, with {tab}; no flag when unset",
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
        "The shape of entries, markers and separators: Zellij's ribbons or a built-in format",
    ),
    option(
        "alias_theme",
        "Theme",
        Kind::Choice(&ALIAS_CHOICES),
        Some("zellij"),
        "The symbols for the flags: the bar's own words or a built-in set",
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
];

pub const FAMILIES: &[FamilySpec] = &[family(
    "color_<name>",
    "Colors",
    Kind::Text,
    "A color alias, used as $name in format strings",
)];

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
        CONFIG_AUTO_WIDTH, CONFIG_COLLAPSE_WHEN_EMPTY, CONFIG_DIM_STRENGTH,
        CONFIG_DIM_WHEN_UNFOCUSED, CONFIG_EMPTY_FORMAT, CONFIG_ENTRY_FORMAT, CONFIG_ENTRY_SPACER,
        CONFIG_FOCUSED_ENTRY_FORMAT, CONFIG_HIDE_WHEN_NESTED, CONFIG_MAX_LENGTH,
        CONFIG_MINIMIZED_ENTRY_FORMAT, CONFIG_MOUSE, CONFIG_MOUSE_CLOSE_CONFIRM,
        CONFIG_MOUSE_WHEEL_REVERSED, CONFIG_OVERFLOW_LEFT_FORMAT, CONFIG_OVERFLOW_RIGHT_FORMAT,
        CONFIG_OVERFLOW_STR, CONFIG_PANE_SCOPE, CONFIG_TAB_SEPARATOR, DEFAULT_AUTO_WIDTH,
        DEFAULT_COLLAPSE_WHEN_EMPTY, DEFAULT_DIM_STRENGTH, DEFAULT_DIM_WHEN_UNFOCUSED,
        DEFAULT_ENTRY_SPACER, DEFAULT_HIDE_WHEN_NESTED, DEFAULT_MAX_LENGTH, DEFAULT_MOUSE,
        DEFAULT_MOUSE_CLOSE_CONFIRM, DEFAULT_MOUSE_WHEEL_REVERSED, DEFAULT_OVERFLOW_STR,
        DEFAULT_PANE_SCOPE, DEFAULT_TAB_SEPARATOR,
    };

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
            // The options zjkit reads for every bar: whether the bar starts
            // hidden, and the themes.
            (zjkit::visibility::OPTION, Some("false".to_string())),
        ];
        for option in zjkit::theme::OPTIONS {
            defaults.push((option, Some(zjkit::theme::THEME_ZELLIJ.to_string())));
        }
        // A flag with no default words, the other tab's, is unset.
        defaults.extend(FLAGS.iter().map(|flag| {
            let default = (!flag.default.is_empty()).then(|| flag.default.to_string());
            (flag.option, default)
        }));
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
        ] {
            assert_eq!(SCHEMA.kind_of(name), Kind::Format, "{name}");
        }
        assert_eq!(SCHEMA.kind_of("max_length"), Kind::Integer);
        assert_eq!(SCHEMA.kind_of("dim_strength"), Kind::Fraction);
        assert!(matches!(SCHEMA.kind_of("pane_scope"), Kind::Choice(_)));
    }
}
