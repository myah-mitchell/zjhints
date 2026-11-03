//! Typed reads of a plugin's configuration.
//!
//! Zellij hands a plugin its options as a `BTreeMap<String, String>`, every
//! value a string whatever the option means. These getters turn one entry into
//! the type the plugin wants, and share one rule for a bad entry: it is
//! treated as if the option were unset, so a typo in a config file falls back
//! to the default instead of stopping the bar from drawing.
//!
//! Values are not trimmed. The settings screen (`settings::schema::validate`)
//! rejects a value with stray whitespace, so one that gets here through a hand
//! edited config reads as unset, the same as on the screen. `wide_ambiguous`
//! is the one exception, as its own comment says.

use std::collections::BTreeMap;

/// The boolean option `key`, or `default` when it is unset or not a boolean.
///
/// `true` and `false` are accepted in any letter case, since a hand written
/// config is as likely to say `True`. Anything else, including `yes`, `1` and
/// the empty string, is not a boolean and gives `default`.
pub fn bool_option(configuration: &BTreeMap<String, String>, key: &str, default: bool) -> bool {
    configuration
        .get(key)
        .and_then(|value| value.to_lowercase().parse::<bool>().ok())
        .unwrap_or(default)
}

/// The whole-number option `key`, or `default` when it is unset or not a
/// number of zero or more.
///
/// Negative numbers, fractions and the empty string all give `default`. A
/// value too large for `usize` does too, rather than wrapping.
pub fn usize_option(configuration: &BTreeMap<String, String>, key: &str, default: usize) -> usize {
    configuration
        .get(key)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

/// The option `key` as a number from `0.0` to `1.0`, or `default` when it is
/// unset, not a number, or not finite.
///
/// A number outside that range is pulled to the nearest end, and so is
/// `default`. Options of this kind are strengths and ratios, where a blend
/// overshoots past its target above `1.0` and runs backward below `0.0`, so
/// the range is held here and no caller has to remember to.
///
/// Rust reads `nan`, `inf` and `infinity` as numbers, in any letter case and
/// with a sign. None of them is a strength anyone means, and NaN would pass
/// through the clamp and turn a blend black, so they give `default` too.
pub fn fraction_option(configuration: &BTreeMap<String, String>, key: &str, default: f32) -> f32 {
    configuration
        .get(key)
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite())
        .unwrap_or(default)
        .clamp(0.0, 1.0)
}

/// The text option `key`, or `None` when it is unset or empty.
///
/// An empty string means "not set" rather than "set to nothing", so that a
/// setting cleared on the settings screen, which writes an empty value, gets
/// the same treatment as one that was never there. Whitespace is kept as
/// written, so a value of one space is text.
pub fn non_empty_string_option(
    configuration: &BTreeMap<String, String>,
    key: &str,
) -> Option<String> {
    configuration
        .get(key)
        .filter(|value| !value.is_empty())
        .cloned()
}

/// The option that says how many columns an East Asian Ambiguous character
/// takes, `1` or `2`. Nerd Font glyphs are Ambiguous, and a terminal set up
/// to show them usually draws two, so a bar measuring them as one would think
/// a line fits when it overflows.
pub const CONFIG_AMBIGUOUS_WIDTH: &str = "ambiguous_width";

/// Whether `ambiguous_width` is `2`, which is what `ansi`'s `wide_ambiguous`
/// arguments take. Anything else, unset included, means one column.
///
/// Unlike the getters above, this one trims the value first, so `" 2"` counts
/// as `2`.
pub fn wide_ambiguous(configuration: &BTreeMap<String, String>) -> bool {
    configuration
        .get(CONFIG_AMBIGUOUS_WIDTH)
        .is_some_and(|value| value.trim() == "2")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configuration(key: &str, value: &str) -> BTreeMap<String, String> {
        BTreeMap::from([(key.to_string(), value.to_string())])
    }

    #[test]
    fn a_missing_key_gives_the_default() {
        let empty = BTreeMap::new();
        assert!(bool_option(&empty, "flag", true));
        assert!(!bool_option(&empty, "flag", false));
        assert_eq!(usize_option(&empty, "count", 7), 7);
        assert_eq!(fraction_option(&empty, "strength", 0.25), 0.25);
        assert_eq!(non_empty_string_option(&empty, "text"), None);
    }

    #[test]
    fn a_bool_is_read_in_any_letter_case() {
        for (value, expected) in [("true", true), ("TRUE", true), ("False", false)] {
            let configuration = configuration("flag", value);
            assert_eq!(bool_option(&configuration, "flag", !expected), expected);
        }
    }

    #[test]
    fn a_value_that_is_not_a_bool_gives_the_default() {
        for value in ["yes", "1", "", " true", "true ", "tru"] {
            let configuration = configuration("flag", value);
            assert!(bool_option(&configuration, "flag", true), "{value:?}");
            assert!(!bool_option(&configuration, "flag", false), "{value:?}");
        }
    }

    #[test]
    fn a_usize_is_read_as_written() {
        assert_eq!(usize_option(&configuration("count", "0"), "count", 9), 0);
        assert_eq!(usize_option(&configuration("count", "42"), "count", 9), 42);
    }

    #[test]
    fn a_value_that_is_not_a_usize_gives_the_default() {
        for value in [
            "-1",
            "1.5",
            "",
            " 3",
            "3 ",
            "many",
            "99999999999999999999999",
        ] {
            let configuration = configuration("count", value);
            assert_eq!(usize_option(&configuration, "count", 9), 9, "{value:?}");
        }
    }

    #[test]
    fn a_fraction_inside_the_range_is_kept() {
        for (value, expected) in [("0", 0.0), ("0.25", 0.25), ("1", 1.0), ("1.0", 1.0)] {
            let configuration = configuration("strength", value);
            assert_eq!(fraction_option(&configuration, "strength", 0.5), expected);
        }
    }

    #[test]
    fn a_fraction_outside_the_range_is_pulled_to_its_nearest_end() {
        for (value, expected) in [("1.5", 1.0), ("100", 1.0), ("-0.1", 0.0), ("-5", 0.0)] {
            let configuration = configuration("strength", value);
            assert_eq!(fraction_option(&configuration, "strength", 0.5), expected);
        }
    }

    #[test]
    fn a_value_that_is_not_a_fraction_gives_the_default() {
        for value in ["", "high", " 0.5", "0.5 "] {
            let configuration = configuration("strength", value);
            assert_eq!(fraction_option(&configuration, "strength", 0.5), 0.5);
        }
    }

    #[test]
    fn a_fraction_that_is_not_finite_gives_the_default() {
        for value in ["nan", "NaN", "-nan", "inf", "-inf", "infinity", "+Infinity"] {
            let configuration = configuration("strength", value);
            assert_eq!(
                fraction_option(&configuration, "strength", 0.5),
                0.5,
                "{value:?}"
            );
        }
    }

    #[test]
    fn a_default_outside_the_range_is_pulled_in_too() {
        let empty = BTreeMap::new();
        assert_eq!(fraction_option(&empty, "strength", 2.0), 1.0);
        assert_eq!(fraction_option(&empty, "strength", -2.0), 0.0);
    }

    #[test]
    fn a_non_empty_string_is_kept_as_written() {
        let padded = configuration("text", "  #[fg=red] | ");
        assert_eq!(
            non_empty_string_option(&padded, "text").as_deref(),
            Some("  #[fg=red] | ")
        );
        let space = configuration("text", " ");
        assert_eq!(
            non_empty_string_option(&space, "text").as_deref(),
            Some(" ")
        );
    }

    #[test]
    fn an_empty_string_is_unset() {
        let cleared = configuration("text", "");
        assert_eq!(non_empty_string_option(&cleared, "text"), None);
    }

    #[test]
    fn only_an_ambiguous_width_of_two_is_wide() {
        assert!(!wide_ambiguous(&BTreeMap::new()));
        for (value, expected) in [
            ("2", true),
            (" 2 ", true),
            ("1", false),
            ("02", false),
            ("", false),
        ] {
            let configuration = configuration(CONFIG_AMBIGUOUS_WIDTH, value);
            assert_eq!(wide_ambiguous(&configuration), expected, "{value:?}");
        }
    }
}
