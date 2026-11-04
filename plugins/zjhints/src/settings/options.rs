//! Every option the bar reads, as the settings screen presents it.
//!
//! This is the one list of options in the code. A test holds it to the
//! configuration reference (docs/configuration.md), so an option added to one
//! and not the other fails the build rather than going missing from the screen.

use zjkit::settings::schema::{family, option, FamilySpec, Kind, OptionSpec};

/// A heading the options are grouped under, in the order they are shown. The
/// same headings, in the same order, as the configuration reference.
///
/// A group may hold fixed options, families, or both. Adding one takes its
/// name here, a heading in the reference, and its options or families naming
/// it as their group.
pub const GROUPS: &[&str] = &[
    "Fitting",
    "Hints",
    "Keys and ordering",
    "Theme",
    "Styling",
    "Colors",
    "Key aliases",
    "Modifier aliases",
    "Labels",
    "Mode prefix",
    "Nested sessions",
    "Mouse",
    "Integration",
];

pub const OPTIONS: &[OptionSpec] = &[
    option(
        "auto_width",
        "Fitting",
        Kind::Bool,
        Some("true"),
        "Fit the hints to the bar's own pane, or to the terminal when feeding zjstatus",
    ),
    option(
        "max_length",
        "Fitting",
        Kind::Integer,
        Some("0"),
        "Hard cap on the line's width in columns, 0 for unlimited",
    ),
    option(
        "reserve_columns",
        "Fitting",
        Kind::Integer,
        Some("0"),
        "Columns to leave free beside the hints in zjstatus; unused in the bar's own pane",
    ),
    option(
        "ambiguous_width",
        "Fitting",
        Kind::Choice(&["1", "2"]),
        Some("1"),
        "Columns an East Asian Ambiguous character occupies",
    ),
    option(
        "overflow_str",
        "Fitting",
        Kind::Format,
        Some("..."),
        "Format string appended when a lone hint is too wide to fit and must be cut mid-hint",
    ),
    option(
        "hint_precedence",
        "Fitting",
        Kind::Choice(&["tl", "lt"]),
        Some("tl"),
        "Which pinned group is kept longest",
    ),
    option(
        "discover_hints",
        "Hints",
        Kind::Bool,
        Some("true"),
        "Also show every enabled keybinding beyond the curated list",
    ),
    option(
        "hide_shared_hints",
        "Hints",
        Kind::Bool,
        Some("true"),
        "Hide bindings inherited from the base mode, except in locked mode",
    ),
    option(
        "hide_in_base_mode",
        "Hints",
        Kind::Bool,
        Some("false"),
        "Hide hints in the base mode, whichever mode default_mode names",
    ),
    option(
        "hidden",
        "Hints",
        Kind::Bool,
        Some("false"),
        "Start with the bar hidden; only a zjhints::show or zjhints::toggle keybinding or zellij pipe brings it back",
    ),
    option(
        "direction_keys",
        "Keys and ordering",
        Kind::Choice(&["both", "arrows", "letters"]),
        Some("both"),
        "Which keys to show when a hint is bound to both hjkl and the arrows",
    ),
    option(
        "key_order",
        "Keys and ordering",
        Kind::Choice(&["qwerty", "dvorak", "colemak", "abcdef", "none"]),
        Some("qwerty"),
        "How keys within a hint are ordered",
    ),
    option(
        "hint_order",
        "Keys and ordering",
        Kind::Text,
        None,
        "Hint ids pinned to the start or end of each mode, around a * for the rest",
    ),
    option(
        "color_theme",
        "Theme",
        Kind::Choice(&crate::theme::COLOR_CHOICES),
        Some("zellij"),
        "The colors named $bg, $accent and so on: Zellij's, none, or a built-in palette",
    ),
    option(
        "format_theme",
        "Theme",
        Kind::Choice(&crate::theme::FORMAT_CHOICES),
        Some("zellij"),
        "The shape of hints and the mode prefix: Zellij's ribbons or a built-in format",
    ),
    option(
        "alias_theme",
        "Theme",
        Kind::Choice(&crate::theme::ALIAS_CHOICES),
        Some("zellij"),
        "The symbols for keys, modifiers and modes: Zellij's names or a built-in set",
    ),
    option(
        "key_format",
        "Styling",
        Kind::Format,
        None,
        "Format string for each hint's keys, theme palette when unset",
    ),
    option(
        "desc_format",
        "Styling",
        Kind::Format,
        None,
        "Format string for each hint's description, theme palette when unset",
    ),
    option(
        "hint_spacer",
        "Styling",
        Kind::Format,
        None,
        "Format string drawn between hints and after the mode prefix",
    ),
    option(
        "drop_indicator",
        "Styling",
        Kind::Format,
        None,
        "Format string marking where hints were dropped (… when mouse is on)",
    ),
    option(
        "hide_when_nested",
        "Nested sessions",
        Kind::Bool,
        Some("true"),
        "Render nothing while this session is nested inside another",
    ),
    option(
        "collapse_when_empty",
        "Nested sessions",
        Kind::Bool,
        Some("true"),
        "Hand the layout row back while the bar has nothing to draw",
    ),
    option(
        "dim_when_unfocused",
        "Nested sessions",
        Kind::Bool,
        Some("true"),
        "Dim hints while this session is not the one receiving input",
    ),
    option(
        "dim_strength",
        "Nested sessions",
        Kind::Fraction,
        Some("0.5"),
        "How strongly to dim, from 0.0 to 1.0",
    ),
    option(
        "show_mode",
        "Mode prefix",
        Kind::Bool,
        Some("false"),
        "Prefix the hints line with the current input mode",
    ),
    option(
        "mode_format",
        "Mode prefix",
        Kind::Format,
        None,
        "Format string for the mode prefix, with {desc}, {key} and {mode} placeholders",
    ),
    option(
        "mouse",
        "Mouse",
        Kind::Bool,
        Some("false"),
        "Right-click menu, clickable hints, and menus of hints that did not fit",
    ),
    option(
        "mouse_nested",
        "Mouse",
        Kind::Bool,
        Some("false"),
        "Also click a descended-into nested session's hints",
    ),
    option(
        "mouse_hover",
        "Mouse",
        Kind::Bool,
        Some("true"),
        "Preview a hint's menu while the pointer rests on it",
    ),
    option(
        "mouse_menus",
        "Mouse",
        Kind::Bool,
        Some("true"),
        "Open a hint's menu when it is clicked",
    ),
    option(
        "pipe_name",
        "Integration",
        Kind::Text,
        Some("zjhints"),
        "Name of the pipe zjstatus reads the hints from",
    ),
];

pub const FAMILIES: &[FamilySpec] = &[
    family(
        "key_format_<id>",
        "Styling",
        Kind::Format,
        "Format string for one hint's keys",
    ),
    family(
        "desc_format_<id>",
        "Styling",
        Kind::Format,
        "Format string for one hint's description",
    ),
    family(
        "keys_<id>",
        "Styling",
        Kind::Text,
        "A fixed string in place of a hint's keys",
    ),
    family(
        "color_<name>",
        "Colors",
        Kind::Text,
        "A color alias, used as $name in format strings",
    ),
    family(
        "key_alias_<name>",
        "Key aliases",
        Kind::Text,
        "A symbol in place of a key's name",
    ),
    family(
        "mod_alias_<name>",
        "Modifier aliases",
        Kind::Text,
        "A symbol in place of a modifier's name",
    ),
    family(
        "chord_mods_<name>",
        "Modifier aliases",
        Kind::Text,
        "The modifiers of a chord alias",
    ),
    family(
        "chord_alias_<name>",
        "Modifier aliases",
        Kind::Text,
        "The symbol shown for a chord alias",
    ),
    family(
        "label_<id>",
        "Labels",
        Kind::Text,
        "A hint's label; empty hides the hint",
    ),
    family(
        "label_<mode>_<id>",
        "Labels",
        Kind::Text,
        "A hint's label in one mode; empty hides it",
    ),
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
];

/// Options of the settings screen itself, read only when it is opened. They
/// configure the screen rather than the bar, so they are not listed on it.
pub const SCREEN_OPTIONS: &[&str] = &["settings", "alias"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SCHEMA;
    use zjkit::settings::reference;

    const REFERENCE: &str = include_str!("../../../../docs/configuration.md");

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
    fn choice_defaults_are_among_their_choices() {
        for option in OPTIONS {
            if let (Kind::Choice(choices), Some(default)) = (option.kind, option.default) {
                assert!(choices.contains(&default), "{}", option.name);
            }
        }
    }

    #[test]
    fn keys_are_sorted_into_the_closest_family() {
        let pattern = |key| SCHEMA.family_of(key).map(|family| family.pattern);
        assert_eq!(pattern("label_split_down"), Some("label_<id>"));
        assert_eq!(pattern("key_format_quit"), Some("key_format_<id>"));
        assert_eq!(pattern("color_blue"), Some("color_<name>"));
        assert_eq!(pattern("mode_format_locked"), Some("mode_format_<mode>"));
        assert_eq!(
            pattern("label_locked_mode_normal"),
            Some("label_<mode>_<id>")
        );
        assert_eq!(pattern("mode_format_bogus"), None);
        assert_eq!(pattern("label_"), None);
        assert_eq!(pattern("something_else"), None);
    }

    #[test]
    fn keys_outside_the_options_are_sorted_into_their_family_groups() {
        assert_eq!(SCHEMA.group_of("color_blue"), Some("Colors"));
        assert_eq!(SCHEMA.group_of("key_alias_enter"), Some("Key aliases"));
        assert_eq!(SCHEMA.group_of("mod_alias_ctrl"), Some("Modifier aliases"));
        assert_eq!(
            SCHEMA.group_of("chord_mods_leader"),
            Some("Modifier aliases")
        );
        assert_eq!(
            SCHEMA.group_of("chord_alias_leader"),
            Some("Modifier aliases")
        );
        assert_eq!(SCHEMA.group_of("label_split_down"), Some("Labels"));
        assert_eq!(SCHEMA.group_of("label_locked_mode_normal"), Some("Labels"));
        assert_eq!(SCHEMA.group_of("mode_format_locked"), Some("Mode prefix"));
        assert_eq!(SCHEMA.group_of("key_format_quit"), Some("Styling"));
        assert_eq!(SCHEMA.group_of("desc_format_quit"), Some("Styling"));
        assert_eq!(SCHEMA.group_of("keys_go_to_tab"), Some("Styling"));
        assert_eq!(SCHEMA.group_of("mode_format"), Some("Mode prefix"));
        assert_eq!(SCHEMA.group_of("something_else"), None);
        // A key that starts like a family's but fits none is listed with
        // the family it was likely meant for, which the screen warns about.
        for (key, group, pattern) in [
            ("mode_format_bogus", "Mode prefix", "mode_format_<mode>"),
            ("label_", "Labels", "label_<id>"),
            ("color_", "Colors", "color_<name>"),
        ] {
            assert_eq!(SCHEMA.group_of(key), Some(group), "{key}");
            let misfit = SCHEMA.misfit_family(key).map(|family| family.pattern);
            assert_eq!(misfit, Some(pattern), "{key}");
        }
        assert_eq!(SCHEMA.patterns_with_prefix("label_"), "label_*");
        assert!(SCHEMA.misfit_family("something_else").is_none());
        assert!(SCHEMA.misfit_family("label_quit").is_none());
    }

    #[test]
    fn format_strings_are_told_apart_from_plain_text() {
        assert_eq!(SCHEMA.kind_of("key_format"), Kind::Format);
        assert_eq!(SCHEMA.kind_of("key_format_quit"), Kind::Format);
        assert_eq!(SCHEMA.kind_of("mode_format_locked"), Kind::Format);
        assert_eq!(SCHEMA.kind_of("label_quit"), Kind::Text);
        assert_eq!(SCHEMA.kind_of("overflow_str"), Kind::Format);
        assert_eq!(SCHEMA.kind_of("max_length"), Kind::Integer);
        assert_eq!(SCHEMA.kind_of("something_else"), Kind::Text);
        assert_eq!(
            FAMILIES
                .iter()
                .map(FamilySpec::prefix)
                .find(|prefix| prefix.starts_with("color")),
            Some("color_")
        );
    }
}
