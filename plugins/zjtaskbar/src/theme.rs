//! The entry options each theme sets.
//!
//! The themes themselves, and the three options that pick them, are
//! `zjkit::theme`, shared with the other bars. This module turns a selection
//! into zjtaskbar's own options: `layer` lays them under the user's
//! configuration, so the rest of the plugin reads one map and never needs to
//! know a theme was involved.
//!
//! - `color_theme` supplies the `color_<name>` aliases.
//! - `format_theme` supplies the entry, spacer, tab separator, overflow
//!   marker, `overflow_str`, launcher button and tray format strings, which
//!   draw with those aliases. `zellij` (the default) adds none, leaving the
//!   bar in Zellij's own ribbon colors.
//! - `alias_theme` supplies the symbol each flag of `{flags}` shows as, the
//!   launcher's `{symbol}`, and each mode's `{key}` in the tray. `zellij`
//!   (the default) adds none, leaving the bar's own words.
//!
//! The three touch separate options, so the order they are laid in does not
//! matter. No theme sets `empty_format`: what an empty bar says is the
//! user's to choose, and by default it says nothing.

use crate::flags::FLAGS;
use crate::{
    CONFIG_CLOCK_ITEM_FORMAT, CONFIG_ENTRY_FORMAT, CONFIG_ENTRY_SPACER,
    CONFIG_FOCUSED_ENTRY_FORMAT, CONFIG_LAUNCHER_FORMAT, CONFIG_LAUNCHER_SYMBOL,
    CONFIG_MINIMIZED_ENTRY_FORMAT, CONFIG_OTHER_TAB_FORMAT, CONFIG_OVERFLOW_LEFT_FORMAT,
    CONFIG_OVERFLOW_RIGHT_FORMAT, CONFIG_OVERFLOW_STR, CONFIG_SESSION_FORMAT, CONFIG_TAB_SEPARATOR,
    CONFIG_TRAY_SPACER,
};
use std::collections::BTreeMap;
use zellij_tile::prelude::Styling;
use zjkit::mode::Edge;
use zjkit::theme::{color_entries, Shape, ARROW, ARROW_LEFT, PILL_LEFT, PILL_RIGHT};

/// The symbol each flag shows as under each alias theme, in the order of
/// `flags::FLAGS`: minimized, exited, plugin, and in another tab.
const FLAG_SYMBOLS: [(&str, [&str; FLAGS.len()]); 3] = [
    // An underscore for a window put away at the bottom, a cross for a
    // finished command, the first letter of "plugin", and an at sign for
    // where else the pane is.
    ("ascii", ["_", "X", "P", "@"]),
    // An arrow down, a cross, a gear and an arrow out to another place.
    ("unicode", ["\u{2193}", "\u{2715}", "\u{2699}", "\u{2197}"]),
    // Material Design glyphs from the Nerd Font set: window-minimize,
    // exit-to-app, puzzle and tab.
    (
        "nerd-font",
        ["\u{f05b0}", "\u{f0206}", "\u{f0431}", "\u{f04e9}"],
    ),
];

/// The launcher's `{symbol}` under each alias theme.
const LAUNCHER_SYMBOLS: [(&str, &str); 3] = [
    // A plus, for something new to open.
    ("ascii", "+"),
    // The trigram for heaven, which reads as a menu's three lines.
    ("unicode", "\u{2630}"),
    // The Material Design apps glyph from the Nerd Font set, a grid of
    // squares, as a desktop's app launcher shows.
    ("nerd-font", "\u{f003b}"),
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
            // Spaced from the title, or the flag before it, the way the
            // default words are. The other-tab flag names the tab after its
            // symbol, as its default does in parentheses.
            let format = if flag.option == CONFIG_OTHER_TAB_FORMAT {
                format!(" {symbol}{{tab}}")
            } else {
                format!(" {symbol}")
            };
            add(flag.option.to_string(), format);
        }
    }
    if let Some(symbol) = launcher_symbol(configuration) {
        add(CONFIG_LAUNCHER_SYMBOL.to_string(), symbol.to_string());
    }

    if let Some(shape) = zjkit::theme::shape(configuration) {
        for (key, value) in format_entries(shape) {
            add(key.to_string(), value);
        }
    }
    // The tray's mode item, in the shape the mode prefix of zjhints and
    // zjtabs takes, turned to face the entries from the line's right end.
    zjkit::mode::layer_theme_at(configuration, &mut layered, Edge::Right);
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

/// The launcher's symbol under the alias theme `configuration` selects, or
/// `None` for `zellij`, unset or unknown.
fn launcher_symbol(configuration: &BTreeMap<String, String>) -> Option<&'static str> {
    let theme = zjkit::theme::alias_theme(configuration)?;
    LAUNCHER_SYMBOLS
        .iter()
        .find(|(name, _)| *name == theme.name)
        .map(|(_, symbol)| *symbol)
}

/// The formats a format theme sets.
///
/// The focused entry is drawn the way zjtabs draws the active tab, and every
/// other entry the way it draws the rest, so the two bars match under one
/// theme. A minimized entry has the shape of a normal one with its text in
/// the muted color, as it is not on screen. An entry's `{flags}` take the
/// style of the text around them, so a flag's symbol shows on the entry's
/// own background. The markers and the overflow string are drawn like a
/// normal entry, so they read as part of the row, and the tab separator is a
/// muted line between the groups of entries.
///
/// The launcher's button has the shape of an entry, with its symbol in the
/// accent color on the background of a normal one and in bold, so it stands
/// apart from both the focused entry and the rest.
///
/// The tray's items stand at the line's right end, so each is the mirror
/// image of an entry: a shape with ends draws only its left one, facing
/// back along the line toward the entries, as zjtabs draws its session
/// name. The clock is drawn like a normal entry, and the session's name as
/// zjtabs draws it, in the accent color; see `zjkit::mode::session_format`.
/// The tray's spacer is the entries' own.
fn format_entries(shape: Shape) -> [(&'static str, String); 12] {
    let separator = "#[fg=$muted] \u{2502} ".to_string();
    let launcher = match shape {
        Shape::Flat => "#[fg=$accent,bg=$surface,bold] {symbol} ".to_string(),
        Shape::Plain => "#[fg=$accent]{symbol}".to_string(),
        Shape::Powerline => {
            format!("#[fg=$accent,bg=$surface,bold] {{symbol}} #[fg=$surface]{ARROW}")
        }
        Shape::Pills => format!(
            "#[fg=$surface]{PILL_LEFT}#[fg=$accent,bg=$surface,bold]{{symbol}}#[fg=$surface]{PILL_RIGHT}"
        ),
    };
    let clock = match shape {
        Shape::Flat => "#[fg=$fg,bg=$surface] {clock} ".to_string(),
        Shape::Plain => "#[fg=$fg]{clock}".to_string(),
        Shape::Powerline => {
            format!("#[fg=$surface]{ARROW_LEFT}#[fg=$fg,bg=$surface] {{clock}} ")
        }
        Shape::Pills => format!("#[fg=$surface]{PILL_LEFT}#[fg=$fg,bg=$surface] {{clock}} "),
    };
    let (entry, focused, minimized, spacer, left, right, overflow) = match shape {
        Shape::Flat => (
            "#[fg=$fg,bg=$surface] {title}{flags} ".to_string(),
            "#[fg=$bg,bg=$accent,bold] {title}{flags} ".to_string(),
            "#[fg=$muted,bg=$surface] {title}{flags} ".to_string(),
            " ",
            "#[fg=$fg,bg=$surface] \u{2190} +{count} ".to_string(),
            "#[fg=$fg,bg=$surface] +{count} \u{2192} ".to_string(),
            "#[fg=$fg,bg=$surface]\u{2026} ".to_string(),
        ),
        Shape::Plain => (
            "#[fg=$fg]{title}{flags}".to_string(),
            "#[fg=$accent,bold]{title}{flags}".to_string(),
            "#[fg=$muted]{title}{flags}".to_string(),
            "  ",
            "#[fg=$muted]\u{2190} +{count}".to_string(),
            "#[fg=$muted]+{count} \u{2192}".to_string(),
            "#[fg=$muted]\u{2026}".to_string(),
        ),
        Shape::Powerline => (
            format!("#[fg=$fg,bg=$surface] {{title}}{{flags}} #[fg=$surface]{ARROW}"),
            format!("#[fg=$bg,bg=$accent,bold] {{title}}{{flags}} #[fg=$accent]{ARROW}"),
            format!("#[fg=$muted,bg=$surface] {{title}}{{flags}} #[fg=$surface]{ARROW}"),
            " ",
            format!("#[fg=$fg,bg=$surface] \u{2190} +{{count}} #[fg=$surface]{ARROW}"),
            format!("#[fg=$fg,bg=$surface] +{{count}} \u{2192} #[fg=$surface]{ARROW}"),
            format!("#[fg=$fg,bg=$surface]\u{2026} #[fg=$surface]{ARROW}"),
        ),
        Shape::Pills => (
            format!(
                "#[fg=$surface]{PILL_LEFT}#[fg=$fg,bg=$surface]{{title}}{{flags}}#[fg=$surface]{PILL_RIGHT}"
            ),
            format!(
                "#[fg=$accent]{PILL_LEFT}#[fg=$bg,bg=$accent,bold]{{title}}{{flags}}#[fg=$accent]{PILL_RIGHT}"
            ),
            format!(
                "#[fg=$surface]{PILL_LEFT}#[fg=$muted,bg=$surface]{{title}}{{flags}}#[fg=$surface]{PILL_RIGHT}"
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
        (CONFIG_ENTRY_FORMAT, entry),
        (CONFIG_FOCUSED_ENTRY_FORMAT, focused),
        (CONFIG_MINIMIZED_ENTRY_FORMAT, minimized),
        (CONFIG_ENTRY_SPACER, spacer.to_string()),
        (CONFIG_TAB_SEPARATOR, separator),
        (CONFIG_OVERFLOW_LEFT_FORMAT, left),
        (CONFIG_OVERFLOW_RIGHT_FORMAT, right),
        (CONFIG_OVERFLOW_STR, overflow),
        (CONFIG_LAUNCHER_FORMAT, launcher),
        (CONFIG_TRAY_SPACER, spacer.to_string()),
        (CONFIG_CLOCK_ITEM_FORMAT, clock),
        (CONFIG_SESSION_FORMAT, zjkit::mode::session_format(shape)),
    ]
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::CONFIG_EMPTY_FORMAT;
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
        key.ends_with("_format")
            || key.starts_with(zjkit::mode::CONFIG_MODE_FORMAT_PREFIX)
            || key == CONFIG_ENTRY_SPACER
            || key == CONFIG_TAB_SEPARATOR
            || key == CONFIG_TRAY_SPACER
            || key == CONFIG_OVERFLOW_STR
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
                        ("title", "editor"),
                        ("index", "1"),
                        ("tab", "work"),
                        ("flags", " _"),
                        ("count", "3"),
                        ("symbol", "+"),
                        ("clock", "09:41"),
                        ("session", "work"),
                        ("desc", "NORMAL"),
                        ("key", "N"),
                        ("mode", "NORMAL"),
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
    fn a_format_theme_sets_every_entry_format() {
        for (name, _) in FORMAT_THEMES {
            let layered = themed(&[(CONFIG_FORMAT_THEME, name)]);
            let entry_formats = [
                CONFIG_ENTRY_FORMAT,
                CONFIG_FOCUSED_ENTRY_FORMAT,
                CONFIG_MINIMIZED_ENTRY_FORMAT,
            ];
            for key in entry_formats.into_iter().chain([
                CONFIG_ENTRY_SPACER,
                CONFIG_TAB_SEPARATOR,
                CONFIG_OVERFLOW_LEFT_FORMAT,
                CONFIG_OVERFLOW_RIGHT_FORMAT,
                CONFIG_OVERFLOW_STR,
            ]) {
                assert!(layered.contains_key(key), "{name} lacks {key}");
            }
            for key in entry_formats {
                for placeholder in ["{title}", "{flags}"] {
                    assert!(layered[key].contains(placeholder), "{name} {key}");
                }
            }
            for key in [CONFIG_OVERFLOW_LEFT_FORMAT, CONFIG_OVERFLOW_RIGHT_FORMAT] {
                assert!(layered[key].contains("{count}"), "{name} {key}");
            }
            assert!(
                layered[CONFIG_LAUNCHER_FORMAT].contains("{symbol}"),
                "{name}"
            );
            // Each kind of entry stands out from the others.
            assert_ne!(
                layered[CONFIG_ENTRY_FORMAT], layered[CONFIG_FOCUSED_ENTRY_FORMAT],
                "{name}"
            );
            assert_ne!(
                layered[CONFIG_ENTRY_FORMAT], layered[CONFIG_MINIMIZED_ENTRY_FORMAT],
                "{name}"
            );
            assert!(!layered.contains_key(CONFIG_EMPTY_FORMAT), "{name}");
        }
    }

    #[test]
    fn every_alias_theme_has_a_symbol_for_every_flag() {
        for theme in &ALIAS_THEMES {
            let layered = themed(&[(CONFIG_ALIAS_THEME, theme.name)]);
            for flag in &FLAGS {
                // The other-tab flag names the tab after its symbol.
                let symbol = layered
                    .get(flag.option)
                    .map(|format| format.trim_end_matches("{tab}").trim());
                assert!(
                    symbol.is_some_and(|symbol| symbol.chars().count() == 1),
                    "{} {}",
                    theme.name,
                    flag.option
                );
            }
            assert!(layered[CONFIG_OTHER_TAB_FORMAT].ends_with("{tab}"));
            assert_eq!(
                layered
                    .get(CONFIG_LAUNCHER_SYMBOL)
                    .map(|symbol| symbol.chars().count()),
                Some(1),
                "{} launcher_symbol",
                theme.name
            );
        }
    }

    #[test]
    fn the_ascii_alias_theme_is_ascii() {
        let layered = themed(&[(CONFIG_ALIAS_THEME, "ascii")]);
        assert_eq!(layered["minimized_format"], " _");
        assert_eq!(layered["other_tab_format"], " @{tab}");
        assert_eq!(layered["launcher_symbol"], "+");
        assert!(layered.values().all(|value| value.is_ascii()));
    }

    #[test]
    fn the_nerd_font_shapes_use_their_glyphs_and_the_others_do_not() {
        let glyphs = [ARROW, PILL_LEFT, PILL_RIGHT];
        for (name, shape) in FORMAT_THEMES {
            let layered = themed(&[(CONFIG_FORMAT_THEME, name)]);
            let needs_font = matches!(shape, Shape::Powerline | Shape::Pills);
            for key in [CONFIG_ENTRY_FORMAT, CONFIG_LAUNCHER_FORMAT] {
                let uses_glyph = glyphs.iter().any(|glyph| layered[key].contains(glyph));
                assert_eq!(uses_glyph, needs_font, "{name} {key}");
            }
        }
    }

    #[test]
    fn user_options_win_over_every_theme() {
        let layered = themed(&[
            (CONFIG_COLOR_THEME, "nord"),
            (CONFIG_FORMAT_THEME, "flat"),
            (CONFIG_ALIAS_THEME, "unicode"),
            ("color_accent", "#010203"),
            ("entry_format", "[{title}]"),
            ("entry_spacer", "| "),
            ("tab_separator", " / "),
            ("overflow_left_format", "<{count}"),
            ("exited_format", "+X"),
        ]);
        assert_eq!(layered["color_accent"], "#010203");
        assert_eq!(layered["entry_format"], "[{title}]");
        assert_eq!(layered["entry_spacer"], "| ");
        assert_eq!(layered["tab_separator"], " / ");
        assert_eq!(layered["overflow_left_format"], "<{count}");
        assert_eq!(layered["exited_format"], "+X");
        // What the user left alone still comes from the themes.
        assert_eq!(layered["color_bg"], "#2e3440");
        assert!(layered["focused_entry_format"].contains("{title}"));
        assert_eq!(layered["minimized_format"], " \u{2193}");
    }

    #[test]
    fn an_empty_user_value_keeps_the_theme_out() {
        let layered = themed(&[
            (CONFIG_FORMAT_THEME, "flat"),
            (CONFIG_ALIAS_THEME, "ascii"),
            ("entry_format", ""),
            ("plugin_format", ""),
        ]);
        assert_eq!(layered["entry_format"], "");
        assert_eq!(layered["plugin_format"], "");
    }

    #[test]
    fn zellij_and_unknown_values_add_nothing_beyond_the_palette() {
        let base = [("entry_format", "{title}"), ("color_blue", "#112233")];
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
    fn a_format_theme_shapes_the_entries_as_zjtabs_shapes_its_tabs() {
        for (name, shape) in FORMAT_THEMES {
            let layered = themed(&[(CONFIG_FORMAT_THEME, name)]);
            let ending = match shape {
                Shape::Powerline => Some(ARROW),
                Shape::Pills => Some(PILL_RIGHT),
                Shape::Flat | Shape::Plain => None,
            };
            for key in [
                CONFIG_ENTRY_FORMAT,
                CONFIG_FOCUSED_ENTRY_FORMAT,
                CONFIG_MINIMIZED_ENTRY_FORMAT,
            ] {
                if let Some(ending) = ending {
                    assert!(layered[key].ends_with(ending), "{name} {key}");
                }
            }
            assert!(
                layered[CONFIG_FOCUSED_ENTRY_FORMAT].contains("$accent"),
                "{name}"
            );
            assert!(
                layered[CONFIG_MINIMIZED_ENTRY_FORMAT].contains("$muted"),
                "{name}"
            );
            // The button takes the entries' shape, and stands apart from the
            // focused one and the rest.
            if let Some(ending) = ending {
                assert!(layered[CONFIG_LAUNCHER_FORMAT].ends_with(ending), "{name}");
            }
            for key in [CONFIG_ENTRY_FORMAT, CONFIG_FOCUSED_ENTRY_FORMAT] {
                let entry_look = layered[key].replace("{title}{flags}", "{symbol}");
                assert_ne!(layered[CONFIG_LAUNCHER_FORMAT], entry_look, "{name} {key}");
            }
        }
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
