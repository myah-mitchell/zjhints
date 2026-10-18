//! Built-in themes.
//!
//! A theme is a set of default option values: the `color_<name>` aliases of a
//! palette, plus the format strings that use them. `layer` lays a theme under
//! the user's own configuration, so the rest of the plugin reads one map and
//! never needs to know a theme was involved.
//!
//! Three kinds of value select the look of the bar:
//!
//! - `zellij` (the default) and `custom` add nothing. Whatever the user left
//!   unstyled falls back to the active Zellij theme palette.
//! - A built-in name adds its aliases and formats under whatever the user set.
//! - Anything else is logged and treated as `zellij`.

use std::collections::BTreeMap;

const CONFIG_THEME: &str = "theme";
const CONFIG_MODE_FORMAT: &str = "mode_format";

/// Selects the Zellij theme palette, which is also what an unset `theme` means.
const THEME_ZELLIJ: &str = "zellij";
/// Selects no theme: only the user's own options apply.
const THEME_CUSTOM: &str = "custom";

/// The `color_<name>` aliases every built-in theme defines, in the order of
/// `Theme::palette`.
const ALIASES: [&str; 11] = [
    "bg", "fg", "surface", "muted", "accent", "red", "green", "yellow", "blue", "magenta", "cyan",
];

/// The Powerline right-pointing arrow, a Nerd Font glyph.
const ARROW: &str = "\u{e0b0}";

/// The alias each mode's prefix is colored with, keyed by the suffix of its
/// `mode_format_<mode>` option (see `mode_config_suffix` in `main.rs`).
const MODE_COLORS: [(&str, &str); 14] = [
    ("normal", "green"),
    ("locked", "red"),
    ("resize", "yellow"),
    ("pane", "blue"),
    ("move", "yellow"),
    ("tab", "magenta"),
    ("scroll", "cyan"),
    ("enter_search", "cyan"),
    ("search", "cyan"),
    ("rename_tab", "magenta"),
    ("rename_pane", "blue"),
    ("session", "magenta"),
    ("tmux", "green"),
    ("prompt", "accent"),
];

/// How a theme's format strings are drawn.
#[derive(Clone, Copy, PartialEq)]
enum Look {
    /// Flat blocks of color side by side.
    Flat,
    /// Blocks joined by Powerline arrows, which need a Nerd Font.
    Powerline,
}

struct Theme {
    name: &'static str,
    /// One hex color per entry of `ALIASES`.
    palette: [&'static str; 11],
    look: Look,
}

const CATPPUCCIN_MOCHA: [&str; 11] = [
    "#1e1e2e", "#cdd6f4", "#313244", "#6c7086", "#cba6f7", "#f38ba8", "#a6e3a1", "#f9e2af",
    "#89b4fa", "#f5c2e7", "#94e2d5",
];

const THEMES: [Theme; 7] = [
    Theme {
        name: "catppuccin-mocha",
        palette: CATPPUCCIN_MOCHA,
        look: Look::Flat,
    },
    Theme {
        name: "catppuccin-latte",
        palette: [
            "#eff1f5", "#4c4f69", "#ccd0da", "#8c8fa1", "#8839ef", "#d20f39", "#40a02b", "#df8e1d",
            "#1e66f5", "#ea76cb", "#179299",
        ],
        look: Look::Flat,
    },
    Theme {
        name: "dracula",
        // Dracula has no blue, so purple stands in for it.
        palette: [
            "#282a36", "#f8f8f2", "#44475a", "#6272a4", "#bd93f9", "#ff5555", "#50fa7b", "#f1fa8c",
            "#bd93f9", "#ff79c6", "#8be9fd",
        ],
        look: Look::Flat,
    },
    Theme {
        name: "nord",
        palette: [
            "#2e3440", "#d8dee9", "#3b4252", "#4c566a", "#88c0d0", "#bf616a", "#a3be8c", "#ebcb8b",
            "#81a1c1", "#b48ead", "#8fbcbb",
        ],
        look: Look::Flat,
    },
    Theme {
        name: "gruvbox-dark",
        palette: [
            "#282828", "#ebdbb2", "#3c3836", "#928374", "#fe8019", "#fb4934", "#b8bb26", "#fabd2f",
            "#83a598", "#d3869b", "#8ec07c",
        ],
        look: Look::Flat,
    },
    Theme {
        name: "tokyo-night",
        palette: [
            "#1a1b26", "#c0caf5", "#292e42", "#565f89", "#7aa2f7", "#f7768e", "#9ece6a", "#e0af68",
            "#7aa2f7", "#bb9af7", "#7dcfff",
        ],
        look: Look::Flat,
    },
    Theme {
        name: "powerline",
        palette: CATPPUCCIN_MOCHA,
        look: Look::Powerline,
    },
];

/// The theme `configuration` selects, if it names a built-in.
fn selected(configuration: &BTreeMap<String, String>) -> Option<&'static Theme> {
    let name = requested(configuration)?;
    THEMES.iter().find(|theme| theme.name == name)
}

/// The normalized value of `theme`, or `None` when it is unset or empty.
fn requested(configuration: &BTreeMap<String, String>) -> Option<String> {
    configuration
        .get(CONFIG_THEME)
        .map(|name| name.trim().to_lowercase())
        .filter(|name| !name.is_empty())
}

/// The value of `theme` when it names nothing this plugin knows, so the caller
/// can log it. `zellij`, `custom`, and unset are all known.
pub fn unknown_name(configuration: &BTreeMap<String, String>) -> Option<String> {
    let name = requested(configuration)?;
    let known = name == THEME_ZELLIJ
        || name == THEME_CUSTOM
        || THEMES.iter().any(|theme| theme.name == name);
    (!known).then_some(name)
}

/// `configuration` with the selected theme's options laid under it. A key the
/// user set, even to an empty string, is never replaced.
///
/// A theme colors each mode's prefix through `mode_format_<mode>`, which
/// outranks `mode_format`. When the user sets `mode_format` they mean it for
/// every mode, so those per-mode defaults are left out rather than overriding
/// it.
pub fn layer(configuration: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut layered = configuration.clone();
    let Some(theme) = selected(configuration) else {
        return layered;
    };
    let user_sets_mode_format = configuration.contains_key(CONFIG_MODE_FORMAT);
    for (key, value) in entries(theme) {
        if user_sets_mode_format && key.starts_with("mode_format_") {
            continue;
        }
        layered.entry(key).or_insert(value);
    }
    layered
}

/// Every option a theme sets.
fn entries(theme: &Theme) -> Vec<(String, String)> {
    let mut entries: Vec<(String, String)> = ALIASES
        .iter()
        .zip(theme.palette)
        .map(|(alias, hex)| (format!("color_{alias}"), hex.to_string()))
        .collect();

    let (key, desc) = match theme.look {
        Look::Flat => (
            "#[fg=$bg,bg=$accent,bold] {key} ".to_string(),
            "#[fg=$fg,bg=$surface] {desc} ".to_string(),
        ),
        Look::Powerline => (
            format!("#[fg=$bg,bg=$accent,bold] {{key}} #[fg=$accent,bg=$surface]{ARROW}"),
            format!("#[fg=$fg,bg=$surface] {{desc}} #[fg=$surface]{ARROW}"),
        ),
    };
    entries.push(("key_format".to_string(), key));
    entries.push(("desc_format".to_string(), desc));
    entries.push(("mode_format".to_string(), mode_format("accent", theme.look)));
    entries.push(("hint_spacer".to_string(), " ".to_string()));

    for (suffix, alias) in MODE_COLORS {
        entries.push((
            format!("mode_format_{suffix}"),
            mode_format(alias, theme.look),
        ));
    }
    entries
}

/// The mode prefix drawn on the color `alias`.
fn mode_format(alias: &str, look: Look) -> String {
    match look {
        Look::Flat => format!("#[fg=$bg,bg=${alias},bold] {{mode}} "),
        Look::Powerline => {
            format!("#[fg=$bg,bg=${alias},bold] {{mode}} #[fg=${alias}]{ARROW}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn themed(name: &str) -> BTreeMap<String, String> {
        layer(&cfg(&[("theme", name)]))
    }

    /// The `$alias` names a format string references.
    fn referenced_aliases(format: &str) -> Vec<String> {
        format
            .split('$')
            .skip(1)
            .map(|rest| {
                rest.chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect()
            })
            .collect()
    }

    fn is_hex_color(value: &str) -> bool {
        value.len() == 7
            && value.starts_with('#')
            && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
    }

    #[test]
    fn the_built_in_names_are_unique() {
        let mut names: Vec<&str> = THEMES.iter().map(|theme| theme.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), THEMES.len());
        assert!(!names.contains(&THEME_ZELLIJ));
        assert!(!names.contains(&THEME_CUSTOM));
    }

    #[test]
    fn every_built_in_defines_every_alias_as_a_hex_color() {
        for theme in &THEMES {
            let layered = themed(theme.name);
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
    fn every_built_in_format_resolves_and_leaves_nothing_raw() {
        for theme in &THEMES {
            let layered = themed(theme.name);
            let formats: Vec<(&String, &String)> = layered
                .iter()
                .filter(|(key, _)| {
                    matches!(key.as_str(), "key_format" | "desc_format" | "hint_spacer")
                        || key.starts_with("mode_format")
                })
                .collect();
            // key, desc, spacer, the global mode format and one per mode.
            assert_eq!(formats.len(), 4 + MODE_COLORS.len(), "{}", theme.name);

            for (key, format) in formats {
                for alias in referenced_aliases(format) {
                    assert!(
                        layered.contains_key(&format!("color_{alias}")),
                        "{}: {key} references undefined ${alias}",
                        theme.name
                    );
                }
                let parts = crate::format::render_template(
                    format,
                    &[("key", "Ctrl p"), ("desc", "pane"), ("mode", "NORMAL")],
                    &layered,
                    0.0,
                );
                let rendered = ansi_term::ANSIStrings(&parts).to_string();
                assert!(!rendered.contains("#["), "{}: {key}", theme.name);
                assert!(!rendered.contains('$'), "{}: {key}", theme.name);
                assert!(!rendered.contains('{'), "{}: {key}", theme.name);
            }
        }
    }

    #[test]
    fn every_aliased_color_in_a_format_paints_something() {
        // An unresolved `$alias` is dropped silently by the renderer, so check
        // the escape for each color the format asks for.
        for theme in &THEMES {
            let layered = themed(theme.name);
            let key_format = &layered["key_format"];
            let parts = crate::format::render_template(key_format, &[("key", "x")], &layered, 0.0);
            let rendered = ansi_term::ANSIStrings(&parts).to_string();
            let accent = escape_for(&layered["color_accent"]);
            let bg = escape_for(&layered["color_bg"]);
            assert!(rendered.contains(&format!("38;2;{bg}")), "{}", theme.name);
            assert!(
                rendered.contains(&format!("48;2;{accent}")),
                "{}",
                theme.name
            );
        }
    }

    /// `#rrggbb` as the `r;g;b` an RGB escape carries.
    fn escape_for(hex: &str) -> String {
        let channel = |range: std::ops::Range<usize>| u8::from_str_radix(&hex[range], 16).unwrap();
        format!("{};{};{}", channel(1..3), channel(3..5), channel(5..7))
    }

    #[test]
    fn the_palettes_are_not_copies_of_each_other_except_where_a_look_differs() {
        for (index, theme) in THEMES.iter().enumerate() {
            for other in &THEMES[index + 1..] {
                if theme.palette == other.palette {
                    assert!(
                        theme.look != other.look,
                        "{} and {} are the same theme",
                        theme.name,
                        other.name
                    );
                }
            }
        }
    }

    #[test]
    fn the_powerline_themes_join_their_blocks_with_arrows() {
        for theme in THEMES.iter().filter(|t| matches!(t.look, Look::Powerline)) {
            let layered = themed(theme.name);
            for key in [
                "key_format",
                "desc_format",
                "mode_format",
                "mode_format_locked",
            ] {
                assert!(layered[key].contains(ARROW), "{}: {key}", theme.name);
            }
        }
        for theme in THEMES.iter().filter(|t| matches!(t.look, Look::Flat)) {
            assert!(!themed(theme.name)["key_format"].contains(ARROW));
        }
    }

    #[test]
    fn user_colors_and_formats_win_over_the_theme() {
        let user = cfg(&[
            ("theme", "nord"),
            ("color_accent", "#010203"),
            ("key_format", "[{key}]"),
            ("hint_spacer", "| "),
        ]);
        let layered = layer(&user);
        assert_eq!(layered["color_accent"], "#010203");
        assert_eq!(layered["key_format"], "[{key}]");
        assert_eq!(layered["hint_spacer"], "| ");
        // What the user left alone still comes from the theme.
        assert_eq!(layered["color_bg"], "#2e3440");
        assert!(layered["desc_format"].contains("{desc}"));
    }

    #[test]
    fn an_empty_user_value_keeps_the_theme_out() {
        let layered = layer(&cfg(&[("theme", "nord"), ("key_format", "")]));
        assert_eq!(layered["key_format"], "");
    }

    #[test]
    fn a_user_mode_format_is_not_outranked_by_the_themes_per_mode_ones() {
        let layered = layer(&cfg(&[("theme", "dracula"), ("mode_format", "<{mode}>")]));
        assert_eq!(layered["mode_format"], "<{mode}>");
        assert!(layered.keys().all(|key| !key.starts_with("mode_format_")));
        // A per-mode option of the user's own is theirs to keep.
        let layered = layer(&cfg(&[
            ("theme", "dracula"),
            ("mode_format_locked", "LOCK"),
        ]));
        assert_eq!(layered["mode_format_locked"], "LOCK");
        assert!(layered["mode_format_normal"].contains("$green"));
    }

    #[test]
    fn zellij_custom_unset_and_unknown_add_nothing() {
        let base = [("key_format", "{key}"), ("color_blue", "#112233")];
        for name in ["zellij", "custom", "ZELLIJ", "", "no-such-theme"] {
            let mut pairs = base.to_vec();
            pairs.push(("theme", name));
            let user = cfg(&pairs);
            assert_eq!(layer(&user), user, "theme {name:?}");
        }
        let user = cfg(&base);
        assert_eq!(layer(&user), user);
        assert_eq!(layer(&cfg(&[])), cfg(&[]));
    }

    #[test]
    fn names_are_trimmed_and_case_insensitive() {
        assert_eq!(themed(" Nord ")["color_bg"], "#2e3440");
    }

    #[test]
    fn only_an_unrecognized_name_is_reported() {
        let unknown = |name: &str| unknown_name(&cfg(&[("theme", name)]));
        assert_eq!(unknown("solarized"), Some("solarized".to_string()));
        assert_eq!(unknown("Solarized "), Some("solarized".to_string()));
        for known in ["zellij", "custom", "nord", ""] {
            assert_eq!(unknown(known), None, "{known:?}");
        }
        assert_eq!(unknown_name(&cfg(&[])), None);
    }

    #[test]
    fn layering_twice_changes_nothing() {
        let once = themed("tokyo-night");
        assert_eq!(layer(&once), once);
    }
}
