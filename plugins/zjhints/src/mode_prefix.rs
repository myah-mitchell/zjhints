//! The current mode drawn ahead of the hints, with `show_mode`.

use crate::{CONFIG_MODE_FORMAT_PREFIX, CONFIG_MODE_KEY_PREFIX, CONFIG_MODE_LABEL_PREFIX};
use ansi_term::{
    ANSIString,
    Colour::{Fixed, RGB},
    Style,
};
use std::collections::BTreeMap;
use zellij_tile::prelude::*;
use zellij_tile_utils::palette_match;
use zjkit::format;

/// Renders the current input mode as a styled prefix (e.g. " NORMAL "), shown
/// at the very start of the hints line when `show_mode` is enabled. Exists so
/// this plugin can stand on its own in a pane, with its own mode indicator,
/// instead of relying on zjstatus's `{mode}` widget on the same line: that's
/// the only way to get a mode indicator next to hints piped through a
/// headless setup, which doesn't receive nested-session state (see
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
pub(crate) fn render_mode_prefix(
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
pub(crate) fn mode_config_suffix(mode: InputMode) -> &'static str {
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
pub(crate) fn style_mode(mode_name: &str, palette: &Styling) -> Vec<ANSIString<'static>> {
    let bg = palette_match!(palette.ribbon_selected.background);
    let fg = palette_match!(palette.ribbon_selected.base);

    vec![Style::new()
        .fg(fg)
        .on(bg)
        .bold()
        .paint(format!(" {} ", mode_name))]
}
