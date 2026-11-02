//! Built-in themes, shared by every bar in the workspace.
//!
//! Three options each pick one kind of theme, and every plugin reads the same
//! three, so one setting gives every bar the same look:
//!
//! - `color_theme` picks the `color_<name>` aliases format strings draw with.
//!   `zellij` (the default) derives them from the running Zellij theme,
//!   `custom` adds none, and a built-in name adds its palette.
//! - `format_theme` picks a `Shape`. What the shape means for a plugin's own
//!   format strings is that plugin's business; this module only names them.
//! - `alias_theme` picks the symbols for modifiers, keys and modes.
//!
//! `zellij` is every option's default, and an unknown value is treated as
//! `zellij` (see `unknown_names` for reporting them). The plugin lays the
//! values a theme selects under the user's own configuration.

use std::collections::BTreeMap;
use zellij_tile::prelude::{PaletteColor, Styling};

/// The option that picks a color theme.
pub const CONFIG_COLOR_THEME: &str = "color_theme";
/// The option that picks a format theme.
pub const CONFIG_FORMAT_THEME: &str = "format_theme";
/// The option that picks an alias theme.
pub const CONFIG_ALIAS_THEME: &str = "alias_theme";
/// The options that choose a theme.
pub const OPTIONS: [&str; 3] = [CONFIG_COLOR_THEME, CONFIG_FORMAT_THEME, CONFIG_ALIAS_THEME];

/// The value every theme option starts at: follow Zellij.
pub const THEME_ZELLIJ: &str = "zellij";
/// The `color_theme` value that adds no aliases at all.
pub const THEME_CUSTOM: &str = "custom";

/// The `color_<name>` aliases every color theme defines, in the order of
/// `ColorTheme::palette`.
pub const ALIASES: [&str; 11] = [
    "bg", "fg", "surface", "muted", "accent", "red", "green", "yellow", "blue", "magenta", "cyan",
];

/// Every Zellij input mode, by the name a plugin's per-mode options are
/// suffixed with (`mode_format_locked`, `mode_key_locked`), with the color
/// alias the mode is drawn in.
pub const MODES: [(&str, &str); 15] = [
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
    ("rename_session", "magenta"),
    ("session", "magenta"),
    ("tmux", "green"),
    ("prompt", "accent"),
];

/// A built-in palette, one color per entry of `ALIASES`.
pub struct ColorTheme {
    pub name: &'static str,
    /// One hex color per entry of `ALIASES`.
    pub palette: [&'static str; 11],
}

pub const COLOR_THEMES: [ColorTheme; 6] = [
    ColorTheme {
        name: "catppuccin-mocha",
        palette: [
            "#1e1e2e", "#cdd6f4", "#313244", "#6c7086", "#cba6f7", "#f38ba8", "#a6e3a1", "#f9e2af",
            "#89b4fa", "#f5c2e7", "#94e2d5",
        ],
    },
    ColorTheme {
        name: "catppuccin-latte",
        palette: [
            "#eff1f5", "#4c4f69", "#ccd0da", "#8c8fa1", "#8839ef", "#d20f39", "#40a02b", "#df8e1d",
            "#1e66f5", "#ea76cb", "#179299",
        ],
    },
    ColorTheme {
        name: "dracula",
        // Dracula has no blue, so purple stands in for it.
        palette: [
            "#282a36", "#f8f8f2", "#44475a", "#6272a4", "#bd93f9", "#ff5555", "#50fa7b", "#f1fa8c",
            "#bd93f9", "#ff79c6", "#8be9fd",
        ],
    },
    ColorTheme {
        name: "nord",
        palette: [
            "#2e3440", "#d8dee9", "#3b4252", "#4c566a", "#88c0d0", "#bf616a", "#a3be8c", "#ebcb8b",
            "#81a1c1", "#b48ead", "#8fbcbb",
        ],
    },
    ColorTheme {
        name: "gruvbox-dark",
        palette: [
            "#282828", "#ebdbb2", "#3c3836", "#928374", "#fe8019", "#fb4934", "#b8bb26", "#fabd2f",
            "#83a598", "#d3869b", "#8ec07c",
        ],
    },
    ColorTheme {
        name: "tokyo-night",
        palette: [
            "#1a1b26", "#c0caf5", "#292e42", "#565f89", "#7aa2f7", "#f7768e", "#9ece6a", "#e0af68",
            "#7aa2f7", "#bb9af7", "#7dcfff",
        ],
    },
];

/// The Powerline right-pointing arrow, a Nerd Font glyph.
pub const ARROW: &str = "\u{e0b0}";
/// The left and right halves of a rounded pill, Nerd Font glyphs.
pub const PILL_LEFT: &str = "\u{e0b6}";
pub const PILL_RIGHT: &str = "\u{e0b4}";

/// The shapes a format theme draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// Blocks of color side by side.
    Flat,
    /// Colored text with no backgrounds.
    Plain,
    /// Blocks joined by Powerline arrows.
    Powerline,
    /// Rounded pills, one per item.
    Pills,
}

/// Every format theme, by name.
pub const FORMAT_THEMES: [(&str, Shape); 4] = [
    ("flat", Shape::Flat),
    ("plain", Shape::Plain),
    ("powerline", Shape::Powerline),
    ("pills", Shape::Pills),
];

/// A set of symbols for modifiers, keys and modes.
pub struct AliasTheme {
    pub name: &'static str,
    /// `(modifier, alias)` for `mod_alias_<modifier>`.
    pub modifiers: &'static [(&'static str, &'static str)],
    /// `(key, alias)` for `key_alias_<key>`.
    pub keys: &'static [(&'static str, &'static str)],
    /// `(mode, symbol)` for `mode_key_<mode>`, the mode prefix's `{key}`.
    pub modes: &'static [(&'static str, &'static str)],
}

pub const ALIAS_THEMES: [AliasTheme; 3] = [
    AliasTheme {
        name: "ascii",
        // Emacs notation: a symbol alias hugs the key, giving `C-p`.
        modifiers: &[
            ("ctrl", "C-"),
            ("alt", "M-"),
            ("shift", "S-"),
            ("super", "s-"),
        ],
        keys: &[
            ("enter", "RET"),
            ("esc", "ESC"),
            ("tab", "TAB"),
            ("space", "SPC"),
            ("backspace", "BS"),
            ("delete", "DEL"),
            ("insert", "INS"),
            ("left", "<"),
            ("down", "v"),
            ("up", "^"),
            ("right", ">"),
            ("home", "Home"),
            ("end", "End"),
            ("pageup", "PgUp"),
            ("pagedown", "PgDn"),
        ],
        // The mode's name says it already; there is no ASCII picture to add.
        modes: &[],
    },
    AliasTheme {
        name: "unicode",
        modifiers: &[
            ("ctrl", "^"),
            ("alt", "\u{2325}"),
            ("shift", "\u{21e7}"),
            ("super", "\u{2318}"),
        ],
        keys: &[
            ("enter", "\u{23ce}"),
            ("esc", "\u{238b}"),
            ("tab", "\u{21e5}"),
            ("space", "\u{2423}"),
            ("backspace", "\u{232b}"),
            ("delete", "\u{2326}"),
            ("insert", "\u{2380}"),
            ("left", "\u{2190}"),
            ("down", "\u{2193}"),
            ("up", "\u{2191}"),
            ("right", "\u{2192}"),
            ("home", "\u{21f1}"),
            ("end", "\u{21f2}"),
            ("pageup", "\u{21de}"),
            ("pagedown", "\u{21df}"),
        ],
        modes: &[
            ("normal", "\u{2328}"),
            ("locked", "\u{2298}"),
            ("resize", "\u{2921}"),
            ("pane", "\u{25a3}"),
            ("move", "\u{2725}"),
            ("tab", "\u{29c9}"),
            ("scroll", "\u{21d5}"),
            ("enter_search", "\u{2315}"),
            ("search", "\u{2315}"),
            ("rename_tab", "\u{270e}"),
            ("rename_pane", "\u{270e}"),
            ("rename_session", "\u{270e}"),
            ("session", "\u{25c9}"),
            ("tmux", "\u{2318}"),
            ("prompt", "\u{276f}"),
        ],
    },
    AliasTheme {
        name: "nerd-font",
        // Material Design glyphs from the Nerd Font set.
        modifiers: &[
            ("ctrl", "\u{f0634}"),
            ("alt", "\u{f0635}"),
            ("shift", "\u{f0636}"),
            ("super", "\u{f0633}"),
        ],
        keys: &[
            ("enter", "\u{f0311}"),
            ("esc", "\u{f12b7}"),
            ("tab", "\u{f0312}"),
            ("space", "\u{f1050}"),
            ("backspace", "\u{f006e}"),
            ("delete", "\u{f0e7e}"),
            ("left", "\u{f004d}"),
            ("down", "\u{f0045}"),
            ("up", "\u{f005d}"),
            ("right", "\u{f0054}"),
            ("home", "\u{f0600}"),
            ("end", "\u{f0601}"),
        ],
        modes: &[
            ("normal", "\u{f030c}"),
            ("locked", "\u{f033e}"),
            ("resize", "\u{f0a68}"),
            ("pane", "\u{f0bcc}"),
            ("move", "\u{f01be}"),
            ("tab", "\u{f04e9}"),
            ("scroll", "\u{f037d}"),
            ("enter_search", "\u{f06ed}"),
            ("search", "\u{f0349}"),
            ("rename_tab", "\u{f0455}"),
            ("rename_pane", "\u{f03eb}"),
            ("rename_session", "\u{f0455}"),
            ("session", "\u{f037a}"),
            ("tmux", "\u{f0633}"),
            ("prompt", "\u{f07b7}"),
        ],
    },
];

/// Every value `color_theme` takes, in the order the settings screen cycles
/// through them.
pub const COLOR_CHOICES: [&str; COLOR_THEMES.len() + 2] = {
    let mut names = [THEME_ZELLIJ; COLOR_THEMES.len() + 2];
    names[1] = THEME_CUSTOM;
    let mut index = 0;
    while index < COLOR_THEMES.len() {
        names[index + 2] = COLOR_THEMES[index].name;
        index += 1;
    }
    names
};

/// Every value `format_theme` takes.
pub const FORMAT_CHOICES: [&str; FORMAT_THEMES.len() + 1] = {
    let mut names = [THEME_ZELLIJ; FORMAT_THEMES.len() + 1];
    let mut index = 0;
    while index < FORMAT_THEMES.len() {
        names[index + 1] = FORMAT_THEMES[index].0;
        index += 1;
    }
    names
};

/// Every value `alias_theme` takes.
pub const ALIAS_CHOICES: [&str; ALIAS_THEMES.len() + 1] = {
    let mut names = [THEME_ZELLIJ; ALIAS_THEMES.len() + 1];
    let mut index = 0;
    while index < ALIAS_THEMES.len() {
        names[index + 1] = ALIAS_THEMES[index].name;
        index += 1;
    }
    names
};

/// The normalized value of the theme option `key`, or `None` when it is unset
/// or empty.
pub fn requested(configuration: &BTreeMap<String, String>, key: &str) -> Option<String> {
    configuration
        .get(key)
        .map(|name| name.trim().to_lowercase())
        .filter(|name| !name.is_empty())
}

/// Each theme option whose value names nothing this plugin knows, as
/// `(option, value)`, so the caller can log them.
pub fn unknown_names(configuration: &BTreeMap<String, String>) -> Vec<(&'static str, String)> {
    let options: [(&str, &[&str]); 3] = [
        (CONFIG_COLOR_THEME, &COLOR_CHOICES),
        (CONFIG_FORMAT_THEME, &FORMAT_CHOICES),
        (CONFIG_ALIAS_THEME, &ALIAS_CHOICES),
    ];
    options
        .into_iter()
        .filter_map(|(option, known)| {
            let name = requested(configuration, option)?;
            (!known.contains(&name.as_str())).then_some((option, name))
        })
        .collect()
}

/// Log each theme option in `configuration` that names no theme, prefixed
/// with `plugin`. Zellij writes a plugin's standard error to its log, which
/// is the one place a bad value gets reported; the bar itself draws with
/// `zellij` instead. Called where a configuration arrives, not each time the
/// palette changes and the themes are laid again.
pub fn log_unknown_themes(plugin: &str, configuration: &BTreeMap<String, String>) {
    for (option, name) in unknown_names(configuration) {
        eprintln!("{plugin}: unknown {option} \"{name}\", using \"{THEME_ZELLIJ}\"");
    }
}

/// The format theme `configuration` selects, or `None` for `zellij`, unset
/// or unknown.
pub fn shape(configuration: &BTreeMap<String, String>) -> Option<Shape> {
    let name = requested(configuration, CONFIG_FORMAT_THEME)?;
    FORMAT_THEMES
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, shape)| *shape)
}

/// The alias theme `configuration` selects, or `None` for `zellij`, unset or
/// unknown.
pub fn alias_theme(configuration: &BTreeMap<String, String>) -> Option<&'static AliasTheme> {
    let name = requested(configuration, CONFIG_ALIAS_THEME)?;
    ALIAS_THEMES.iter().find(|theme| theme.name == name)
}

/// The `(alias, color)` pairs `color_theme` selects.
pub fn color_entries(
    configuration: &BTreeMap<String, String>,
    zellij_palette: Option<&Styling>,
) -> Vec<(&'static str, String)> {
    let name = requested(configuration, CONFIG_COLOR_THEME);
    if name.as_deref() == Some(THEME_CUSTOM) {
        return Vec::new();
    }
    let built_in = name.and_then(|name| COLOR_THEMES.iter().find(|theme| theme.name == name));
    let colors: Vec<String> = match (built_in, zellij_palette) {
        (Some(theme), _) => theme.palette.iter().map(|hex| hex.to_string()).collect(),
        // `zellij`, unset, or unknown.
        (None, Some(styling)) => zellij_colors(styling).to_vec(),
        (None, None) => return Vec::new(),
    };
    ALIASES.into_iter().zip(colors).collect()
}

/// The running Zellij theme's colors, one per entry of `ALIASES`, picked from
/// the declarations Zellij fills from each palette slot.
pub fn zellij_colors(styling: &Styling) -> [String; 11] {
    [
        styling.text_unselected.background,
        styling.text_unselected.base,
        styling.text_selected.background,
        styling.table_title.background,
        styling.ribbon_selected.background,
        styling.exit_code_error.base,
        styling.exit_code_success.base,
        styling.exit_code_error.emphasis_0,
        styling.ribbon_unselected.emphasis_2,
        styling.text_unselected.emphasis_3,
        styling.text_unselected.emphasis_1,
    ]
    .map(color_value)
}

/// A palette color as a `color_<name>` value: `#rrggbb`, or the index of a
/// 256-color one.
pub fn color_value(color: PaletteColor) -> String {
    match color {
        PaletteColor::Rgb((r, g, b)) => format!("#{r:02x}{g:02x}{b:02x}"),
        PaletteColor::EightBit(index) => index.to_string(),
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

    fn is_hex_color(value: &str) -> bool {
        value.len() == 7
            && value.starts_with('#')
            && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
    }

    #[test]
    fn the_built_in_names_of_each_kind_are_unique() {
        for choices in [&COLOR_CHOICES[..], &FORMAT_CHOICES[..], &ALIAS_CHOICES[..]] {
            let mut names = choices.to_vec();
            names.sort_unstable();
            names.dedup();
            assert_eq!(names.len(), choices.len(), "{choices:?}");
        }
    }

    #[test]
    fn every_built_in_color_theme_gives_every_alias_a_hex_color() {
        for theme in &COLOR_THEMES {
            let entries = color_entries(&cfg(&[(CONFIG_COLOR_THEME, theme.name)]), None);
            let aliases: Vec<&str> = entries.iter().map(|(alias, _)| *alias).collect();
            assert_eq!(aliases, ALIASES, "{}", theme.name);
            assert!(
                entries.iter().all(|(_, color)| is_hex_color(color)),
                "{}",
                theme.name
            );
        }
    }

    #[test]
    fn the_zellij_color_theme_reads_the_running_palette() {
        let mut styling = Styling::default();
        styling.ribbon_selected.background = PaletteColor::Rgb((0xa6, 0xe3, 0xa1));
        styling.exit_code_error.base = PaletteColor::EightBit(196);
        for configuration in [cfg(&[]), cfg(&[(CONFIG_COLOR_THEME, "zellij")])] {
            let entries: BTreeMap<_, _> = color_entries(&configuration, Some(&styling))
                .into_iter()
                .collect();
            assert_eq!(entries["accent"], "#a6e3a1");
            // A 256-color palette entry is written as its index.
            assert_eq!(entries["red"], "196");
        }
    }

    #[test]
    fn custom_colors_or_no_palette_yet_give_no_aliases() {
        let styling = Styling::default();
        assert!(color_entries(&cfg(&[(CONFIG_COLOR_THEME, "custom")]), Some(&styling)).is_empty());
        assert!(color_entries(&cfg(&[]), None).is_empty());
    }

    #[test]
    fn a_theme_is_found_by_its_trimmed_lowercased_name() {
        assert_eq!(
            shape(&cfg(&[(CONFIG_FORMAT_THEME, " Pills ")])),
            Some(Shape::Pills)
        );
        assert_eq!(
            alias_theme(&cfg(&[(CONFIG_ALIAS_THEME, "ASCII")])).map(|theme| theme.name),
            Some("ascii")
        );
        for name in ["zellij", "", "no-such-theme"] {
            assert_eq!(
                shape(&cfg(&[(CONFIG_FORMAT_THEME, name)])),
                None,
                "{name:?}"
            );
            assert!(
                alias_theme(&cfg(&[(CONFIG_ALIAS_THEME, name)])).is_none(),
                "{name:?}"
            );
        }
    }
    #[test]
    fn only_unrecognized_names_are_reported_with_their_option() {
        let unknown = unknown_names(&cfg(&[
            (CONFIG_COLOR_THEME, "Solarized "),
            (CONFIG_FORMAT_THEME, "pills"),
            (CONFIG_ALIAS_THEME, "emoji"),
        ]));
        assert_eq!(
            unknown,
            vec![
                (CONFIG_COLOR_THEME, "solarized".to_string()),
                (CONFIG_ALIAS_THEME, "emoji".to_string()),
            ]
        );
        assert!(unknown_names(&cfg(&[])).is_empty());
        assert!(unknown_names(&cfg(&[(CONFIG_FORMAT_THEME, "custom")])).len() == 1);
    }
}
