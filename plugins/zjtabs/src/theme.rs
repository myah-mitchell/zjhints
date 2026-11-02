//! The tab options each theme sets.
//!
//! The themes themselves, and the three options that pick them, are
//! `zjkit::theme`, shared with the other bars. This module turns a selection
//! into zjtabs' own options: `layer` lays them under the user's
//! configuration, so the rest of the plugin reads one map and never needs to
//! know a theme was involved.
//!
//! - `color_theme` supplies the `color_<name>` aliases.
//! - `format_theme` supplies the tab, spacer, overflow marker and
//!   `overflow_str` format strings, which draw with those aliases. `zellij`
//!   (the default) adds none, leaving the tabs in Zellij's own ribbon colors.
//! - `alias_theme` supplies the symbol each flag of `{flags}` shows as.
//!   `zellij` (the default) adds none, leaving Zellij's own words.
//!
//! The three touch separate options, so the order they are laid in does not
//! matter.

use crate::flags::FLAGS;
use crate::{
    CONFIG_ACTIVE_TAB_FORMAT, CONFIG_OVERFLOW_LEFT_FORMAT, CONFIG_OVERFLOW_RIGHT_FORMAT,
    CONFIG_OVERFLOW_STR, CONFIG_TAB_FORMAT, CONFIG_TAB_SPACER,
};
use std::collections::BTreeMap;
use zellij_tile::prelude::Styling;
use zjkit::theme::{color_entries, Shape, ARROW, PILL_LEFT, PILL_RIGHT};

/// The symbol each flag shows as under each alias theme, in the order of
/// `flags::FLAGS`: fullscreen, sync, floating panes shown, and the bell.
const FLAG_SYMBOLS: [(&str, [&str; FLAGS.len()]); 3] = [
    // The letters tmux marks a zoomed window and a bell with, and the first
    // letter of the other two.
    ("ascii", ["Z", "S", "F", "!"]),
    // Arrows out to the corners, arrows both ways, a raised square and the
    // dot Zellij's own tab bar marks a tab that needs the user with.
    ("unicode", ["\u{2922}", "\u{21c4}", "\u{2750}", "\u{25cf}"]),
    // Material Design glyphs from the Nerd Font set: fullscreen, sync,
    // layers and a bell.
    (
        "nerd-font",
        ["\u{f0293}", "\u{f04e6}", "\u{f0328}", "\u{f009a}"],
    ),
];

/// `configuration` with the selected themes' options laid under it. A key the
/// user set, even to an empty string, is never replaced.
///
/// `zellij_palette` is the running Zellij theme, which `color_theme zellij`
/// takes its aliases from. `None` (no palette has arrived yet) adds none.
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

    if let Some(symbols) = flag_symbols(configuration) {
        for (flag, symbol) in FLAGS.iter().zip(symbols) {
            // Spaced from the name, or the flag before it, the way the
            // default words are.
            add(flag.option.to_string(), format!(" {symbol}"));
        }
    }

    if let Some(shape) = zjkit::theme::shape(configuration) {
        for (key, value) in format_entries(shape) {
            add(key.to_string(), value);
        }
    }
    layered
}

/// The flag symbols of the alias theme `configuration` selects, or `None`
/// for `zellij`, unset or unknown.
fn flag_symbols(configuration: &BTreeMap<String, String>) -> Option<[&'static str; FLAGS.len()]> {
    let theme = zjkit::theme::alias_theme(configuration)?;
    FLAG_SYMBOLS
        .iter()
        .find(|(name, _)| *name == theme.name)
        .map(|(_, symbols)| *symbols)
}

/// The formats a format theme sets.
///
/// A tab's `{flags}` take the style of the text around them, so a flag's
/// symbol shows on the tab's own background. Every marker and the overflow
/// string are drawn like an inactive tab, so they read as part of the row of
/// tabs.
fn format_entries(shape: Shape) -> [(&'static str, String); 6] {
    let (tab, active_tab, spacer, left, right, overflow) = match shape {
        Shape::Flat => (
            "#[fg=$fg,bg=$surface] {index} {name}{flags} ".to_string(),
            "#[fg=$bg,bg=$accent,bold] {index} {name}{flags} ".to_string(),
            " ",
            "#[fg=$fg,bg=$surface] \u{2190} +{count} ".to_string(),
            "#[fg=$fg,bg=$surface] +{count} \u{2192} ".to_string(),
            "#[fg=$fg,bg=$surface]\u{2026} ".to_string(),
        ),
        Shape::Plain => (
            "#[fg=$muted]{index} {name}{flags}".to_string(),
            "#[fg=$accent,bold]{index} {name}{flags}".to_string(),
            "  ",
            "#[fg=$muted]\u{2190} +{count}".to_string(),
            "#[fg=$muted]+{count} \u{2192}".to_string(),
            "#[fg=$muted]\u{2026}".to_string(),
        ),
        Shape::Powerline => (
            format!("#[fg=$fg,bg=$surface] {{index}} {{name}}{{flags}} #[fg=$surface]{ARROW}"),
            format!(
                "#[fg=$bg,bg=$accent,bold] {{index}} {{name}}{{flags}} #[fg=$accent]{ARROW}"
            ),
            " ",
            format!("#[fg=$fg,bg=$surface] \u{2190} +{{count}} #[fg=$surface]{ARROW}"),
            format!("#[fg=$fg,bg=$surface] +{{count}} \u{2192} #[fg=$surface]{ARROW}"),
            format!("#[fg=$fg,bg=$surface]\u{2026} #[fg=$surface]{ARROW}"),
        ),
        Shape::Pills => (
            format!(
                "#[fg=$surface]{PILL_LEFT}#[fg=$fg,bg=$surface]{{index}} {{name}}{{flags}}#[fg=$surface]{PILL_RIGHT}"
            ),
            format!(
                "#[fg=$accent]{PILL_LEFT}#[fg=$bg,bg=$accent,bold]{{index}} {{name}}{{flags}}#[fg=$accent]{PILL_RIGHT}"
            ),
            " ",
            format!(
                "#[fg=$surface]{PILL_LEFT}#[fg=$fg,bg=$surface]\u{2190} +{{count}}#[fg=$surface]{PILL_RIGHT}"
            ),
            format!(
                "#[fg=$surface]{PILL_LEFT}#[fg=$fg,bg=$surface]+{{count}} \u{2192}#[fg=$surface]{PILL_RIGHT}"
            ),
            format!("#[fg=$fg,bg=$surface]\u{2026}#[fg=$surface]{PILL_RIGHT}"),
        ),
    };
    [
        (CONFIG_TAB_FORMAT, tab),
        (CONFIG_ACTIVE_TAB_FORMAT, active_tab),
        (CONFIG_TAB_SPACER, spacer.to_string()),
        (CONFIG_OVERFLOW_LEFT_FORMAT, left),
        (CONFIG_OVERFLOW_RIGHT_FORMAT, right),
        (CONFIG_OVERFLOW_STR, overflow),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flags::FLAGS;
    use zellij_tile::prelude::PaletteColor;
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
    pub(crate) fn zellij_palette() -> Styling {
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

    /// Every combination of the three theme options.
    pub(crate) fn every_combination() -> Vec<Vec<(&'static str, &'static str)>> {
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

    /// The options a theme may set that are format strings.
    fn is_format(key: &str) -> bool {
        key.ends_with("_format") || key == CONFIG_TAB_SPACER || key == CONFIG_OVERFLOW_STR
    }

    #[test]
    fn every_combination_renders_with_nothing_left_raw() {
        let palette = zellij_palette();
        for combination in every_combination() {
            let layered = layer(&cfg(&combination), Some(&palette));
            for (key, format) in layered.iter().filter(|(key, _)| is_format(key)) {
                let parts = zjkit::format::render_template(
                    format,
                    &[
                        ("index", "1"),
                        ("name", "editor"),
                        ("flags", " Z"),
                        ("count", "3"),
                    ],
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
    fn a_format_theme_sets_every_tab_format() {
        for (name, _) in FORMAT_THEMES {
            let layered = themed(&[(CONFIG_FORMAT_THEME, name)]);
            for key in [
                CONFIG_TAB_FORMAT,
                CONFIG_ACTIVE_TAB_FORMAT,
                CONFIG_TAB_SPACER,
                CONFIG_OVERFLOW_LEFT_FORMAT,
                CONFIG_OVERFLOW_RIGHT_FORMAT,
                CONFIG_OVERFLOW_STR,
            ] {
                assert!(layered.contains_key(key), "{name} lacks {key}");
            }
            for key in [CONFIG_TAB_FORMAT, CONFIG_ACTIVE_TAB_FORMAT] {
                for placeholder in ["{index}", "{name}", "{flags}"] {
                    assert!(layered[key].contains(placeholder), "{name} {key}");
                }
            }
            for key in [CONFIG_OVERFLOW_LEFT_FORMAT, CONFIG_OVERFLOW_RIGHT_FORMAT] {
                assert!(layered[key].contains("{count}"), "{name} {key}");
            }
            // The active tab stands out from the others.
            assert_ne!(
                layered[CONFIG_TAB_FORMAT], layered[CONFIG_ACTIVE_TAB_FORMAT],
                "{name}"
            );
        }
    }

    #[test]
    fn every_alias_theme_has_a_symbol_for_every_flag() {
        for theme in &ALIAS_THEMES {
            let layered = themed(&[(CONFIG_ALIAS_THEME, theme.name)]);
            for flag in &FLAGS {
                let symbol = layered.get(flag.option);
                assert!(
                    symbol.is_some_and(|symbol| symbol.trim().chars().count() == 1),
                    "{} {}",
                    theme.name,
                    flag.option
                );
            }
        }
    }

    #[test]
    fn the_ascii_alias_theme_is_ascii() {
        let layered = themed(&[(CONFIG_ALIAS_THEME, "ascii")]);
        assert_eq!(layered["fullscreen_format"], " Z");
        assert!(layered.values().all(|value| value.is_ascii()));
    }

    #[test]
    fn the_nerd_font_shapes_use_their_glyphs_and_the_others_do_not() {
        let glyphs = [ARROW, PILL_LEFT, PILL_RIGHT];
        for (name, shape) in FORMAT_THEMES {
            let layered = themed(&[(CONFIG_FORMAT_THEME, name)]);
            let uses_glyph = glyphs
                .iter()
                .any(|glyph| layered[CONFIG_TAB_FORMAT].contains(glyph));
            let needs_font = matches!(shape, Shape::Powerline | Shape::Pills);
            assert_eq!(uses_glyph, needs_font, "{name}");
        }
    }

    #[test]
    fn user_options_win_over_every_theme() {
        let layered = themed(&[
            (CONFIG_COLOR_THEME, "nord"),
            (CONFIG_FORMAT_THEME, "flat"),
            (CONFIG_ALIAS_THEME, "unicode"),
            ("color_accent", "#010203"),
            ("tab_format", "[{name}]"),
            ("tab_spacer", "| "),
            ("overflow_left_format", "<{count}"),
            ("sync_format", "+S"),
        ]);
        assert_eq!(layered["color_accent"], "#010203");
        assert_eq!(layered["tab_format"], "[{name}]");
        assert_eq!(layered["tab_spacer"], "| ");
        assert_eq!(layered["overflow_left_format"], "<{count}");
        assert_eq!(layered["sync_format"], "+S");
        // What the user left alone still comes from the themes.
        assert_eq!(layered["color_bg"], "#2e3440");
        assert!(layered["active_tab_format"].contains("{name}"));
        assert_eq!(layered["fullscreen_format"], " \u{2922}");
    }

    #[test]
    fn an_empty_user_value_keeps_the_theme_out() {
        let layered = themed(&[
            (CONFIG_FORMAT_THEME, "flat"),
            (CONFIG_ALIAS_THEME, "ascii"),
            ("tab_format", ""),
            ("bell_format", ""),
        ]);
        assert_eq!(layered["tab_format"], "");
        assert_eq!(layered["bell_format"], "");
    }

    #[test]
    fn zellij_and_unknown_values_add_nothing_beyond_the_palette() {
        let base = [("tab_format", "{name}"), ("color_blue", "#112233")];
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
    fn the_zellij_color_theme_takes_its_aliases_from_the_running_palette() {
        let layered = layer(&cfg(&[]), Some(&zellij_palette()));
        assert_eq!(layered["color_accent"], "#a6e3a1");
        assert_eq!(layered["color_red"], "196");
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
