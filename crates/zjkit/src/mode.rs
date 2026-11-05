//! The current input mode, drawn as a prefix at the start of a bar.
//!
//! A bar with a pane of its own has no zjstatus `{mode}` widget beside it, so
//! it can draw the mode itself, turned on by the plugin's `show_mode` option.
//! Every bar reads the same options for it, under the same names zjstatus
//! uses for its own per-mode keys, so a mode set up once looks the same on
//! every bar:
//!
//! - `mode_format` is a format string for every mode, and `mode_format_<mode>`
//!   one for a single mode, which outranks it.
//! - `mode_label_<mode>` is the mode's name, drawn for `{desc}`.
//! - `mode_key_<mode>` is the mode's symbol, drawn for `{key}`.
//!
//! `<mode>` is the suffix `mode_config_suffix` gives, such as `enter_search`.
//! With no format at all, the prefix is a pill in the Zellij theme's
//! `ribbon_selected` colors.
//!
//! The themes supply all of these: see `layer_theme`.
//!
//! A bar that ends its line with the session's name, as zjtabs does, draws it
//! as the prefix's mirror image. `session_format` is the look each format
//! theme gives it.

use crate::format;
use crate::theme::{self, Shape, ARROW, ARROW_LEFT, MODES, PILL_LEFT, PILL_RIGHT};
use ansi_term::{
    ANSIString,
    Colour::{self, Fixed, RGB},
    Style,
};
use std::collections::BTreeMap;
use zellij_tile::prelude::{InputMode, PaletteColor, Styling};

/// The option that is the mode prefix's format string for every mode.
pub const CONFIG_MODE_FORMAT: &str = "mode_format";
/// The start of the option that is one mode's format string, such as
/// `mode_format_normal`, which outranks `mode_format`. Matches zjstatus's
/// own `mode_<suffix>` naming exactly (see `mode_config_suffix`), so an icon
/// set up for zjstatus's `{mode}` widget can be reused here.
pub const CONFIG_MODE_FORMAT_PREFIX: &str = "mode_format_";
/// The start of the option that is one mode's name, such as
/// `mode_label_locked`, drawn for `{desc}`.
pub const CONFIG_MODE_LABEL_PREFIX: &str = "mode_label_";
/// The start of the option that is one mode's symbol, such as
/// `mode_key_locked`, drawn for `{key}`.
pub const CONFIG_MODE_KEY_PREFIX: &str = "mode_key_";

/// Every Zellij input mode, in the order Zellij declares them.
pub const INPUT_MODES: [InputMode; 15] = [
    InputMode::Normal,
    InputMode::Locked,
    InputMode::Resize,
    InputMode::Pane,
    InputMode::Tab,
    InputMode::Scroll,
    InputMode::EnterSearch,
    InputMode::Search,
    InputMode::RenameTab,
    InputMode::RenamePane,
    InputMode::Session,
    InputMode::Move,
    InputMode::Prompt,
    InputMode::Tmux,
    InputMode::RenameSession,
];

/// Renders the current input mode as a styled prefix (e.g. " NORMAL "), shown
/// at the very start of a bar's line when its `show_mode` is enabled. Exists
/// so a bar can stand on its own in a pane, with its own mode indicator,
/// instead of relying on zjstatus's `{mode}` widget on the same line. For
/// zjhints that is the only way to get a mode indicator next to hints piped
/// through a headless setup, which doesn't receive nested-session state (see
/// docs/nested-sessions.md).
///
/// A per-mode override (`mode_format_normal`, `mode_format_locked`, ...)
/// takes precedence over the global `mode_format`, which takes precedence
/// over the built-in theme styling. Per-mode overrides use the same suffix
/// names as zjstatus's own `mode_<suffix>` config keys (see
/// `mode_config_suffix`), so an icon already set up for zjstatus's `{mode}`
/// widget can be copied straight over. `dim` fades either path the same
/// amount: the built-in styling through `colors` (already dimmed by the
/// caller), a custom format string through `format::render_template`, which
/// resolves its own literal colors and would otherwise ignore `colors`
/// (and `dim_when_unfocused`) entirely.
///
/// A format string draws the mode's name as `{desc}` and its symbol as
/// `{key}`, taken from `mode_label_<mode>` and `mode_key_<mode>` the way a
/// hint's come from its label and keys. `{mode}` is always the bare name. The
/// built-in styling draws the symbol before the name when there is one.
pub fn render_mode_prefix(
    mode: InputMode,
    colors: &Styling,
    mode_format: Option<&str>,
    config: &BTreeMap<String, String>,
    dim: f32,
) -> Vec<ANSIString<'static>> {
    let mode_name = format!("{:?}", mode).to_uppercase();
    let suffix = mode_config_suffix(mode);
    let per_mode = |prefix: &str| config.get(&format!("{prefix}{suffix}")).map(String::as_str);
    let label = per_mode(CONFIG_MODE_LABEL_PREFIX).unwrap_or(&mode_name);
    let symbol = per_mode(CONFIG_MODE_KEY_PREFIX).unwrap_or_default();
    let format_str = per_mode(CONFIG_MODE_FORMAT_PREFIX).or(mode_format);

    match format_str {
        Some(fmt) => format::render_template(
            fmt,
            &[("mode", &mode_name), ("desc", label), ("key", symbol)],
            config,
            dim,
        ),
        None if symbol.is_empty() => style_mode(label, colors),
        None => style_mode(&format!("{symbol} {label}"), colors),
    }
}

/// The zjstatus-style suffix for a mode's per-mode config keys, e.g.
/// `InputMode::EnterSearch` -> `"enter_search"`, matching zjstatus's own
/// `mode_enter_search` naming exactly (see `zjstatus`'s `ModeWidget::new`).
pub fn mode_config_suffix(mode: InputMode) -> &'static str {
    match mode {
        InputMode::Normal => "normal",
        InputMode::Locked => "locked",
        InputMode::Resize => "resize",
        InputMode::Pane => "pane",
        InputMode::Tab => "tab",
        InputMode::Scroll => "scroll",
        InputMode::EnterSearch => "enter_search",
        InputMode::Search => "search",
        InputMode::RenameTab => "rename_tab",
        InputMode::RenamePane => "rename_pane",
        InputMode::RenameSession => "rename_session",
        InputMode::Session => "session",
        InputMode::Move => "move",
        InputMode::Prompt => "prompt",
        InputMode::Tmux => "tmux",
    }
}

/// Default mode styling when neither a per-mode nor a global `mode_format`
/// is set: the theme's `ribbon_selected` pill, the same "currently active"
/// style Zellij's own ribbons use, bold so it reads as a badge rather than
/// plain text.
fn style_mode(mode_name: &str, palette: &Styling) -> Vec<ANSIString<'static>> {
    let bg = colour_of(palette.ribbon_selected.background);
    let fg = colour_of(palette.ribbon_selected.base);

    vec![Style::new()
        .fg(fg)
        .on(bg)
        .bold()
        .paint(format!(" {} ", mode_name))]
}

/// A Zellij palette color as the terminal color `ansi_term` paints with.
fn colour_of(color: PaletteColor) -> Colour {
    match color {
        PaletteColor::Rgb((r, g, b)) => RGB(r, g, b),
        PaletteColor::EightBit(index) => Fixed(index),
    }
}

/// Lay the mode prefix's options the selected themes set under `layered`,
/// which is `configuration` with the plugin's own theme options laid under it
/// so far. A key already in `layered`, the user's own included, is never
/// replaced.
///
/// - The alias theme gives every mode a symbol, `mode_key_<mode>`.
/// - The format theme gives `mode_format` and a `mode_format_<mode>` for
///   every mode, each in the color `theme::MODES` gives that mode, with a
///   block for the mode's symbol when it has one.
///
/// A per-mode format outranks `mode_format`. When the user sets
/// `mode_format` they mean it for every mode, so the theme's per-mode
/// formats are left out rather than overriding it.
pub fn layer_theme(
    configuration: &BTreeMap<String, String>,
    layered: &mut BTreeMap<String, String>,
) {
    if let Some(theme) = theme::alias_theme(configuration) {
        for (mode, symbol) in theme.modes {
            layered
                .entry(format!("{CONFIG_MODE_KEY_PREFIX}{mode}"))
                .or_insert_with(|| symbol.to_string());
        }
    }

    let Some(shape) = theme::shape(configuration) else {
        return;
    };
    // Read after the alias theme went in, so a mode whose symbol came from
    // either the theme or the user gets a block for it.
    let keyed: Vec<bool> = MODES
        .iter()
        .map(|(mode, _)| {
            layered
                .get(&format!("{CONFIG_MODE_KEY_PREFIX}{mode}"))
                .is_some_and(|symbol| !symbol.is_empty())
        })
        .collect();
    let user_sets_mode_format = configuration.contains_key(CONFIG_MODE_FORMAT);
    layered
        .entry(CONFIG_MODE_FORMAT.to_string())
        .or_insert_with(|| mode_format("accent", shape, false));
    if !user_sets_mode_format {
        for ((mode, alias), keyed) in MODES.iter().zip(keyed) {
            layered
                .entry(format!("{CONFIG_MODE_FORMAT_PREFIX}{mode}"))
                .or_insert_with(|| mode_format(alias, shape, keyed));
        }
    }
}

/// The mode prefix drawn on the color `alias` in `shape`, with a block for
/// the mode's `{key}` symbol when `keyed`.
///
/// The prefix opens the line, against the edge of the pane, so a shape with
/// ends draws only the end that faces the rest of the line.
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

/// The color alias every format theme draws the session name on.
///
/// The mode prefix takes the color of its mode, so it changes as the mode
/// does. The session name stays the same from one mode to the next, so it
/// keeps one color: the theme's accent, which the active tab and the theme's
/// own `mode_format` are drawn in too.
const SESSION_COLOR: &str = "accent";

/// The session name drawn in `shape`, for a bar that ends its line with it:
/// the mirror image of the mode prefix, which opens the line. Its one
/// placeholder, `{session}`, is the session's name.
///
/// The block closes the line, against the right edge of the pane, so a shape
/// with ends draws only the end that faces the rest of the line, on its left:
/// the left half of a pill, or an arrow pointing back along the line. A flat
/// or plain one is the mode prefix's look with the name in place of the
/// mode's.
pub fn session_format(shape: Shape) -> String {
    let color = SESSION_COLOR;
    match shape {
        Shape::Flat => format!("#[fg=$bg,bg=${color},bold] {{session}} "),
        Shape::Plain => format!("#[fg=${color},bold]{{session}}"),
        Shape::Powerline => {
            format!("#[fg=${color}]{ARROW_LEFT}#[fg=$bg,bg=${color},bold] {{session}} ")
        }
        Shape::Pills => {
            format!("#[fg=$surface]{PILL_LEFT}#[fg=${color},bg=$surface,bold] {{session}} ")
        }
    }
}

/// The placeholders a bar fills in the mode prefix's format string under
/// `key`, with the sample value each gets in the settings screen's list, or
/// `None` when `key` is not one of the mode prefix's formats.
pub fn sample_values(key: &str) -> Option<Vec<(&'static str, String)>> {
    if key == CONFIG_MODE_FORMAT {
        Some(vec![("mode", "NORMAL".to_string())])
    } else {
        // The bar names a mode by its variant, which is the suffix without
        // underscores: enter_search is ENTERSEARCH.
        key.strip_prefix(CONFIG_MODE_FORMAT_PREFIX)
            .map(|mode| vec![("mode", mode.replace('_', "").to_uppercase())])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::schema::MODE_NAMES;
    use crate::theme::{CONFIG_ALIAS_THEME, CONFIG_FORMAT_THEME, FORMAT_THEMES};

    fn cfg(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn layered(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        let configuration = cfg(pairs);
        let mut layered = configuration.clone();
        layer_theme(&configuration, &mut layered);
        layered
    }

    #[test]
    fn every_mode_has_a_suffix_the_schema_and_the_themes_know() {
        let suffixes: Vec<&str> = INPUT_MODES.into_iter().map(mode_config_suffix).collect();
        let mut sorted = suffixes.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), INPUT_MODES.len(), "a suffix is used twice");
        for suffix in suffixes {
            assert!(MODE_NAMES.contains(&suffix), "{suffix}");
            assert!(MODES.iter().any(|(mode, _)| *mode == suffix), "{suffix}");
        }
        assert_eq!(MODE_NAMES.len(), INPUT_MODES.len());
        assert_eq!(MODES.len(), INPUT_MODES.len());
    }

    #[test]
    fn no_theme_adds_nothing() {
        assert_eq!(
            layered(&[("mode_key_locked", "L")]),
            cfg(&[("mode_key_locked", "L")])
        );
    }

    #[test]
    fn a_format_theme_sets_a_format_for_every_mode_that_names_it_through_desc() {
        for (name, _) in FORMAT_THEMES {
            let layered = layered(&[(CONFIG_FORMAT_THEME, name)]);
            assert!(layered[CONFIG_MODE_FORMAT].contains("{desc}"), "{name}");
            for (mode, alias) in MODES {
                let format = &layered[&format!("{CONFIG_MODE_FORMAT_PREFIX}{mode}")];
                assert!(format.contains("{desc}"), "{name} {mode}");
                assert!(format.contains(&format!("${alias}")), "{name} {mode}");
                assert!(!format.contains("{key}"), "{name} {mode}");
            }
        }
    }

    #[test]
    fn an_alias_theme_gives_every_mode_a_symbol_and_a_block_for_it() {
        let layered = layered(&[
            (CONFIG_FORMAT_THEME, "flat"),
            (CONFIG_ALIAS_THEME, "unicode"),
        ]);
        for (mode, _) in MODES {
            assert!(!layered[&format!("{CONFIG_MODE_KEY_PREFIX}{mode}")].is_empty());
            assert!(layered[&format!("{CONFIG_MODE_FORMAT_PREFIX}{mode}")].contains("{key}"));
        }
    }

    #[test]
    fn the_session_format_mirrors_the_mode_prefix_in_every_shape() {
        for (name, shape) in FORMAT_THEMES {
            let session = session_format(shape);
            let mode = mode_format("accent", shape, false);
            assert!(session.contains("{session}"), "{name}");
            assert!(session.contains("$accent"), "{name}");
            // The prefix's end faces right, after its block; the session's
            // faces left, before its block.
            let (session_end, mode_end) = match shape {
                Shape::Powerline => (Some(ARROW_LEFT), Some(ARROW)),
                Shape::Pills => (Some(PILL_LEFT), Some(PILL_RIGHT)),
                Shape::Flat | Shape::Plain => (None, None),
            };
            match (session_end, mode_end) {
                (Some(session_end), Some(mode_end)) => {
                    assert!(mode.ends_with(mode_end), "{name}");
                    let end = session.find(session_end).expect(name);
                    assert!(end < session.find("{session}").unwrap(), "{name}");
                    assert!(!session.contains(mode_end), "{name}");
                }
                // The same look as the prefix, with the name for the mode's.
                _ => assert_eq!(session.replace("{session}", "{desc}"), mode, "{name}"),
            }
        }
    }

    #[test]
    fn the_user_s_mode_format_keeps_the_per_mode_formats_out() {
        let layered = layered(&[
            (CONFIG_FORMAT_THEME, "pills"),
            (CONFIG_MODE_FORMAT, "<{mode}>"),
        ]);
        assert_eq!(layered[CONFIG_MODE_FORMAT], "<{mode}>");
        assert!(layered
            .keys()
            .all(|key| !key.starts_with(CONFIG_MODE_FORMAT_PREFIX)));
    }
}
