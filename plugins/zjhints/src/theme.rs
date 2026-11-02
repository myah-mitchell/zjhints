//! The hint options each theme sets.
//!
//! The themes themselves, and the three options that pick them, are
//! `zjkit::theme`, shared with the other bars. This module turns a selection
//! into zjhints' own options: `layer` lays them under the user's
//! configuration, so the rest of the plugin reads one map and never needs to
//! know a theme was involved.
//!
//! - `color_theme` supplies the `color_<name>` aliases.
//! - `format_theme` supplies the hint and mode format strings, which draw with
//!   those aliases. `zellij` (the default) adds none, leaving Zellij's ribbons.
//! - `alias_theme` supplies the key and modifier aliases and each mode's
//!   `{key}` symbol. `zellij` (the default) adds none, leaving Zellij's names.
//!
//! The three touch separate options, so the order they are laid in does not
//! matter.

use crate::{
    CONFIG_DESC_FORMAT, CONFIG_DROP_INDICATOR, CONFIG_HINT_SPACER, CONFIG_KEY_ALIAS_PREFIX,
    CONFIG_KEY_FORMAT, CONFIG_MODE_FORMAT, CONFIG_MODE_FORMAT_PREFIX, CONFIG_MODE_KEY_PREFIX,
    CONFIG_MOD_ALIAS_PREFIX, CONFIG_OVERFLOW_STR,
};
use std::collections::BTreeMap;
use zellij_tile::prelude::{BareKey, Styling};
use zjkit::theme::{color_entries, Shape, ARROW, MODES, PILL_LEFT, PILL_RIGHT};
pub use zjkit::theme::{ALIAS_CHOICES, COLOR_CHOICES, FORMAT_CHOICES};

/// `configuration` with the selected themes' options laid under it. A key the
/// user set, even to an empty string, is never replaced.
///
/// `zellij_palette` is the running Zellij theme, which `color_theme zellij`
/// takes its aliases from. `None` (no palette has arrived yet) adds none.
///
/// A format theme colors each mode's prefix through `mode_format_<mode>`,
/// which outranks `mode_format`. When the user sets `mode_format` they mean it
/// for every mode, so those per-mode defaults are left out rather than
/// overriding it.
pub fn layer(
    configuration: &BTreeMap<String, String>,
    zellij_palette: Option<&Styling>,
) -> BTreeMap<String, String> {
    let mut layered = configuration.clone();
    let mut add = |key: String, value: String| {
        layered.entry(key).or_insert(value);
    };

    for (alias, color) in color_entries(configuration, zellij_palette) {
        add(format!("color_{alias}"), color);
    }

    if let Some(theme) = zjkit::theme::alias_theme(configuration) {
        for (modifier, alias) in theme.modifiers {
            add(
                format!("{CONFIG_MOD_ALIAS_PREFIX}{modifier}"),
                alias.to_string(),
            );
        }
        for (key, alias) in theme.keys {
            // The user may alias the key by a synonym, which `key_<name>`
            // would outrank.
            let user_aliased = key
                .parse::<BareKey>()
                .is_ok_and(|bare_key| crate::key_alias(&bare_key, configuration).is_some());
            if !user_aliased {
                add(format!("{CONFIG_KEY_ALIAS_PREFIX}{key}"), alias.to_string());
            }
        }
        for (mode, symbol) in theme.modes {
            add(
                format!("{CONFIG_MODE_KEY_PREFIX}{mode}"),
                symbol.to_string(),
            );
        }
    }

    if let Some(shape) = zjkit::theme::shape(configuration) {
        // Read after the alias theme went in, so a mode whose symbol came
        // from either one gets a block for it.
        let has_key = |mode: &str| {
            layered
                .get(&format!("{CONFIG_MODE_KEY_PREFIX}{mode}"))
                .is_some_and(|symbol| !symbol.is_empty())
        };
        let keyed: Vec<bool> = MODES.iter().map(|(mode, _)| has_key(mode)).collect();
        let user_sets_mode_format = configuration.contains_key(CONFIG_MODE_FORMAT);
        for (key, value) in format_entries(shape) {
            layered.entry(key).or_insert(value);
        }
        if !user_sets_mode_format {
            for ((mode, alias), keyed) in MODES.iter().zip(keyed) {
                layered
                    .entry(format!("{CONFIG_MODE_FORMAT_PREFIX}{mode}"))
                    .or_insert_with(|| mode_format(alias, shape, keyed));
            }
        }
    }
    layered
}

/// The hint formats a format theme sets, and its `mode_format` for display
/// and for modes it has no own entry for.
fn format_entries(shape: Shape) -> Vec<(String, String)> {
    // The drop indicator is drawn like a description, so the gap where
    // hints were dropped reads as part of the bar. The overflow marker ends a
    // hint the cut went through, so it closes that hint's shape.
    let (key, desc, spacer, drop, overflow) = match shape {
        Shape::Flat => (
            "#[fg=$bg,bg=$accent,bold] {key} ".to_string(),
            "#[fg=$fg,bg=$surface] {desc} ".to_string(),
            " ",
            "#[fg=$fg,bg=$surface] … ".to_string(),
            "#[fg=$fg,bg=$surface]… ".to_string(),
        ),
        Shape::Plain => (
            "#[fg=$accent,bold]{key}".to_string(),
            "#[fg=$fg] {desc}".to_string(),
            "  ",
            "#[fg=$muted]…".to_string(),
            "#[fg=$muted]…".to_string(),
        ),
        Shape::Powerline => (
            format!("#[fg=$bg,bg=$accent,bold] {{key}} #[fg=$accent,bg=$surface]{ARROW}"),
            format!("#[fg=$fg,bg=$surface] {{desc}} #[fg=$surface]{ARROW}"),
            " ",
            format!("#[fg=$fg,bg=$surface] … #[fg=$surface]{ARROW}"),
            format!("#[fg=$fg,bg=$surface]… #[fg=$surface]{ARROW}"),
        ),
        Shape::Pills => (
            format!("#[fg=$accent]{PILL_LEFT}#[fg=$bg,bg=$accent,bold]{{key}} "),
            format!("#[fg=$accent,bg=$surface,bold] {{desc}}#[fg=$surface]{PILL_RIGHT}"),
            " ",
            format!("#[fg=$surface]{PILL_LEFT}#[fg=$fg,bg=$surface]…#[fg=$surface]{PILL_RIGHT}"),
            format!("#[fg=$fg,bg=$surface]…#[fg=$surface]{PILL_RIGHT}"),
        ),
    };
    vec![
        (CONFIG_KEY_FORMAT.to_string(), key),
        (CONFIG_DESC_FORMAT.to_string(), desc),
        (CONFIG_HINT_SPACER.to_string(), spacer.to_string()),
        (CONFIG_DROP_INDICATOR.to_string(), drop),
        (CONFIG_OVERFLOW_STR.to_string(), overflow),
        (
            CONFIG_MODE_FORMAT.to_string(),
            mode_format("accent", shape, false),
        ),
    ]
}

/// The mode prefix drawn on the color `alias`, with a block for the mode's
/// `{key}` symbol when `keyed`.
fn mode_format(alias: &str, shape: Shape, keyed: bool) -> String {
    match (shape, keyed) {
        (Shape::Flat, false) => format!("#[fg=$bg,bg=${alias},bold] {{desc}} "),
        (Shape::Flat, true) => format!("#[fg=$bg,bg=${alias},bold] {{key}} {{desc}} "),
        (Shape::Plain, false) => format!("#[fg=${alias},bold]{{desc}}"),
        (Shape::Plain, true) => format!("#[fg=${alias},bold]{{key}} {{desc}}"),
        (Shape::Powerline, false) => {
            format!("#[fg=$bg,bg=${alias},bold] {{desc}} #[fg=${alias}]{ARROW}")
        }
        (Shape::Powerline, true) => {
            format!("#[fg=$bg,bg=${alias},bold] {{key}} {{desc}} #[fg=${alias}]{ARROW}")
        }
        (Shape::Pills, false) => {
            format!("#[fg=${alias},bg=$surface,bold] {{desc}} #[fg=$surface]{PILL_RIGHT}")
        }
        (Shape::Pills, true) => format!(
            "#[fg=${alias},bg=$surface,bold] {{desc}} #[fg=$bg,bg=${alias},bold] {{key}} #[fg=${alias}]{PILL_RIGHT}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SCHEMA;
    use zellij_tile::prelude::PaletteColor;
    use zjkit::settings::schema::Kind;
    use zjkit::theme::unknown_names;
    use zjkit::theme::*;

    fn cfg(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn themed(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        layer(&cfg(pairs), None)
    }

    /// A Zellij palette whose every color is `#0a0b0c`, apart from the few
    /// set to tell the aliases apart.
    fn zellij_palette() -> Styling {
        let mut styling = Styling::default();
        let gray = PaletteColor::Rgb((10, 11, 12));
        for declaration in [
            &mut styling.text_unselected,
            &mut styling.text_selected,
            &mut styling.ribbon_unselected,
            &mut styling.ribbon_selected,
            &mut styling.table_title,
            &mut styling.exit_code_error,
            &mut styling.exit_code_success,
        ] {
            declaration.base = gray;
            declaration.background = gray;
            declaration.emphasis_0 = gray;
            declaration.emphasis_1 = gray;
            declaration.emphasis_2 = gray;
            declaration.emphasis_3 = gray;
        }
        styling.ribbon_selected.background = PaletteColor::Rgb((0xa6, 0xe3, 0xa1));
        styling.exit_code_error.base = PaletteColor::EightBit(196);
        styling
    }

    #[test]
    fn the_settings_screen_offers_every_value_of_each_theme_option() {
        for (option, choices) in [
            (CONFIG_COLOR_THEME, &COLOR_CHOICES[..]),
            (CONFIG_FORMAT_THEME, &FORMAT_CHOICES[..]),
            (CONFIG_ALIAS_THEME, &ALIAS_CHOICES[..]),
        ] {
            let spec = SCHEMA.find(option).unwrap();
            let Kind::Choice(offered) = spec.kind else {
                panic!("{option} should be a choice");
            };
            assert_eq!(offered, choices, "{option}");
            assert_eq!(offered[0], THEME_ZELLIJ, "{option}");
            assert_eq!(spec.default, Some(THEME_ZELLIJ), "{option}");
            for name in offered {
                assert!(
                    unknown_names(&cfg(&[(option, name)])).is_empty(),
                    "{option} {name}"
                );
            }
        }
    }

    fn is_hex_color(value: &str) -> bool {
        value.len() == 7
            && value.starts_with('#')
            && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
    }

    #[test]
    fn every_built_in_color_theme_defines_every_alias_as_a_hex_color() {
        for theme in &COLOR_THEMES {
            let layered = themed(&[(CONFIG_COLOR_THEME, theme.name)]);
            for alias in ALIASES {
                let value = layered.get(&format!("color_{alias}"));
                assert!(
                    value.is_some_and(|value| is_hex_color(value)),
                    "{} lacks a hex color_{alias}",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn the_zellij_color_theme_takes_its_aliases_from_the_running_palette() {
        let palette = zellij_palette();
        for user in [cfg(&[]), cfg(&[(CONFIG_COLOR_THEME, "zellij")])] {
            let layered = layer(&user, Some(&palette));
            assert_eq!(layered["color_accent"], "#a6e3a1");
            // A 256-color palette entry is written as its index.
            assert_eq!(layered["color_red"], "196");
            assert_eq!(layered["color_bg"], "#0a0b0c");
            assert!(ALIASES
                .iter()
                .all(|alias| layered.contains_key(&format!("color_{alias}"))));
        }
        // An index resolves the way a format string reads it.
        let layered = layer(&cfg(&[]), Some(&palette));
        let parts = zjkit::format::render_template("#[fg=$red]x", &[], &layered, 0.0);
        assert!(ansi_term::ANSIStrings(&parts)
            .to_string()
            .contains("38;5;196"));
    }

    #[test]
    fn no_palette_yet_or_custom_colors_add_no_aliases() {
        assert_eq!(themed(&[]), cfg(&[]));
        let custom = cfg(&[(CONFIG_COLOR_THEME, "custom")]);
        assert_eq!(layer(&custom, Some(&zellij_palette())), custom);
    }

    /// Every combination of the three theme options.
    fn every_combination() -> Vec<Vec<(&'static str, &'static str)>> {
        let mut combinations = Vec::new();
        for color in COLOR_CHOICES {
            for format in FORMAT_CHOICES {
                for alias in ALIAS_CHOICES {
                    combinations.push(vec![
                        (CONFIG_COLOR_THEME, color),
                        (CONFIG_FORMAT_THEME, format),
                        (CONFIG_ALIAS_THEME, alias),
                    ]);
                }
            }
        }
        combinations
    }

    #[test]
    fn every_combination_renders_with_nothing_left_raw() {
        let palette = zellij_palette();
        for combination in every_combination() {
            let layered = layer(&cfg(&combination), Some(&palette));
            for (key, format) in layered.iter().filter(|(key, _)| {
                matches!(
                    key.as_str(),
                    "key_format"
                        | "desc_format"
                        | "hint_spacer"
                        | "drop_indicator"
                        | "overflow_str"
                ) || key.starts_with("mode_format")
            }) {
                let parts = zjkit::format::render_template(
                    format,
                    &[("key", "Ctrl p"), ("desc", "pane"), ("mode", "NORMAL")],
                    &layered,
                    0.0,
                );
                let rendered = ansi_term::ANSIStrings(&parts).to_string();
                for raw in ["#[", "$", "{"] {
                    assert!(!rendered.contains(raw), "{combination:?}: {key} left {raw}");
                }
            }
        }
    }

    #[test]
    fn a_format_theme_sets_the_hint_formats_and_a_format_for_every_mode() {
        for (name, _) in FORMAT_THEMES {
            let layered = themed(&[(CONFIG_FORMAT_THEME, name)]);
            for key in [
                "key_format",
                "desc_format",
                "hint_spacer",
                "drop_indicator",
                "overflow_str",
                "mode_format",
            ] {
                assert!(layered.contains_key(key), "{name} lacks {key}");
            }
            for (mode, _) in MODES {
                assert!(
                    layered.contains_key(&format!("mode_format_{mode}")),
                    "{name} {mode}"
                );
            }
        }
    }

    #[test]
    fn a_mode_gets_a_key_block_only_when_it_has_a_symbol() {
        let plain = themed(&[(CONFIG_FORMAT_THEME, "pills")]);
        assert!(MODES
            .iter()
            .all(|(mode, _)| !plain[&format!("mode_format_{mode}")].contains("{key}")));

        let with_glyphs = themed(&[
            (CONFIG_FORMAT_THEME, "pills"),
            (CONFIG_ALIAS_THEME, "nerd-font"),
        ]);
        assert!(MODES
            .iter()
            .all(|(mode, _)| with_glyphs[&format!("mode_format_{mode}")].contains("{key}")));

        // A symbol of the user's own counts the same as a theme's.
        let one = themed(&[(CONFIG_FORMAT_THEME, "flat"), ("mode_key_locked", "L")]);
        assert!(one["mode_format_locked"].contains("{key}"));
        assert!(!one["mode_format_normal"].contains("{key}"));
        // So does emptying one a theme set.
        let emptied = themed(&[
            (CONFIG_FORMAT_THEME, "flat"),
            (CONFIG_ALIAS_THEME, "unicode"),
            ("mode_key_locked", ""),
        ]);
        assert!(!emptied["mode_format_locked"].contains("{key}"));
        assert!(emptied["mode_format_normal"].contains("{key}"));
    }

    #[test]
    fn every_mode_prefix_names_the_mode_through_desc() {
        for combination in every_combination() {
            let layered = themed(&combination);
            for (key, format) in layered
                .iter()
                .filter(|(key, _)| key.starts_with("mode_format"))
            {
                assert!(format.contains("{desc}"), "{combination:?}: {key}");
            }
        }
    }

    #[test]
    fn the_nerd_font_shapes_use_their_glyphs_and_the_others_do_not() {
        let glyphs = [ARROW, PILL_LEFT, PILL_RIGHT];
        for (name, shape) in FORMAT_THEMES {
            let layered = themed(&[(CONFIG_FORMAT_THEME, name)]);
            let uses_glyph = glyphs
                .iter()
                .any(|glyph| layered["desc_format"].contains(glyph));
            let needs_font = matches!(shape, Shape::Powerline | Shape::Pills);
            assert_eq!(uses_glyph, needs_font, "{name}");
        }
    }

    #[test]
    fn an_alias_theme_names_only_modifiers_keys_and_modes_the_bar_reads() {
        for theme in &ALIAS_THEMES {
            for (modifier, _) in theme.modifiers {
                assert!(
                    crate::parse_modifier_name(modifier).is_some(),
                    "{} {modifier}",
                    theme.name
                );
            }
            for (mode, _) in theme.modes {
                assert!(
                    MODES.iter().any(|(known, _)| known == mode),
                    "{} {mode}",
                    theme.name
                );
            }
            for (key, alias) in theme.keys {
                // The name a key is looked up by first, so `layer` can find
                // the key it names.
                let first_name = key
                    .parse::<BareKey>()
                    .ok()
                    .map(|bare_key| crate::key_alias_names(&bare_key).remove(0));
                assert_eq!(first_name.as_deref(), Some(*key), "{}", theme.name);
                assert!(!alias.is_empty());
            }
        }
    }

    #[test]
    fn the_ascii_alias_theme_is_ascii() {
        let layered = themed(&[(CONFIG_ALIAS_THEME, "ascii")]);
        assert_eq!(layered["mod_alias_ctrl"], "C-");
        assert!(layered.values().all(|value| value.is_ascii()));
    }

    #[test]
    fn user_options_win_over_every_theme() {
        let layered = themed(&[
            (CONFIG_COLOR_THEME, "nord"),
            (CONFIG_FORMAT_THEME, "flat"),
            (CONFIG_ALIAS_THEME, "unicode"),
            ("color_accent", "#010203"),
            ("key_format", "[{key}]"),
            ("hint_spacer", "| "),
            ("mod_alias_ctrl", "Ctl-"),
            ("mode_key_normal", "N"),
        ]);
        assert_eq!(layered["color_accent"], "#010203");
        assert_eq!(layered["key_format"], "[{key}]");
        assert_eq!(layered["hint_spacer"], "| ");
        assert_eq!(layered["mod_alias_ctrl"], "Ctl-");
        assert_eq!(layered["mode_key_normal"], "N");
        // What the user left alone still comes from the themes.
        assert_eq!(layered["color_bg"], "#2e3440");
        assert!(layered["desc_format"].contains("{desc}"));
        assert_eq!(layered["mod_alias_alt"], "\u{2325}");
    }

    #[test]
    fn a_format_theme_s_overflow_str_closes_the_cut_hint_in_its_shape() {
        let pills = themed(&[(CONFIG_FORMAT_THEME, "pills")]);
        assert!(pills["overflow_str"].ends_with(PILL_RIGHT));
        let powerline = themed(&[(CONFIG_FORMAT_THEME, "powerline")]);
        assert!(powerline["overflow_str"].ends_with(ARROW));
        // The user's own marker outranks the theme's.
        let own = themed(&[(CONFIG_FORMAT_THEME, "pills"), ("overflow_str", "~")]);
        assert_eq!(own["overflow_str"], "~");
    }

    #[test]
    fn an_empty_user_value_keeps_the_theme_out() {
        let layered = themed(&[(CONFIG_FORMAT_THEME, "flat"), ("key_format", "")]);
        assert_eq!(layered["key_format"], "");
    }

    #[test]
    fn a_key_the_user_aliased_by_a_synonym_keeps_their_alias() {
        let layered = themed(&[(CONFIG_ALIAS_THEME, "unicode"), ("key_alias_return", "RET")]);
        assert!(!layered.contains_key("key_alias_enter"));
        assert_eq!(
            crate::key_alias(&BareKey::Enter, &layered).as_deref(),
            Some("RET")
        );
        // Keys the user left alone still come from the theme.
        assert_eq!(layered["key_alias_esc"], "\u{238b}");
    }

    #[test]
    fn a_user_mode_format_is_not_outranked_by_the_themes_per_mode_ones() {
        let layered = themed(&[(CONFIG_FORMAT_THEME, "flat"), ("mode_format", "<{mode}>")]);
        assert_eq!(layered["mode_format"], "<{mode}>");
        assert!(layered.keys().all(|key| !key.starts_with("mode_format_")));
        // A per-mode option of the user's own is theirs to keep.
        let layered = themed(&[
            (CONFIG_FORMAT_THEME, "flat"),
            ("mode_format_locked", "LOCK"),
        ]);
        assert_eq!(layered["mode_format_locked"], "LOCK");
        assert!(layered["mode_format_normal"].contains("$green"));
    }

    #[test]
    fn zellij_and_unknown_values_add_nothing_beyond_the_palette() {
        let base = [("key_format", "{key}"), ("color_blue", "#112233")];
        for option in [CONFIG_COLOR_THEME, CONFIG_FORMAT_THEME, CONFIG_ALIAS_THEME] {
            for name in ["zellij", "ZELLIJ", "", "no-such-theme"] {
                let mut pairs = base.to_vec();
                pairs.push((option, name));
                let user = cfg(&pairs);
                assert_eq!(layer(&user, None), user, "{option} {name:?}");
            }
        }
    }

    #[test]
    fn names_are_trimmed_and_case_insensitive() {
        assert_eq!(
            themed(&[(CONFIG_COLOR_THEME, " Nord ")])["color_bg"],
            "#2e3440"
        );
        assert!(themed(&[(CONFIG_ALIAS_THEME, "ASCII")]).contains_key("mod_alias_ctrl"));
    }

    #[test]
    fn layering_twice_changes_nothing() {
        let palette = zellij_palette();
        for combination in every_combination() {
            let once = layer(&cfg(&combination), Some(&palette));
            assert_eq!(layer(&once, Some(&palette)), once, "{combination:?}");
        }
    }
}
