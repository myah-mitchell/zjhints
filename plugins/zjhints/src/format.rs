//! Parsing of zjstatus-style format strings.
//!
//! This mirrors the styling syntax used by [zjstatus](https://github.com/dj95/zjstatus)
//! so that hint styles can be configured in exactly the same way as any other
//! zjstatus widget. A format string is a sequence of literal text and `#[...]`
//! styling directives, e.g.:
//!
//! ```text
//! #[fg=$black,bg=$blue,bold] {key} #[fg=#cdd6f4,bg=#1e1e2e] {desc}
//! ```
//!
//! Everything after a `#[...]` block (up to the next block) is painted with the
//! style described by that block. Supported directives, matching zjstatus:
//!
//! - `fg=<color>` / `bg=<color>`: foreground / background color
//! - Effects: `bold`, `italic`/`italics`, `underscore`, `blink`, `hidden`,
//!   `dim`, `strikethrough`, `reverse`
//!
//! Colors accept the same forms as zjstatus:
//!
//! - `$alias`: looks up `color_<alias>` from the plugin configuration
//! - `#RRGGBB`: hex RGB
//! - a named color (`red`, `bright_black`, ...)
//! - `colour<N>` / `color<N>` or a bare `0`-`255`: an ANSI 256 color index
//!
//! Effects and color forms that the underlying `ansi_term` renderer cannot
//! express (e.g. `us=` underline colors, curly/dotted underlines) are parsed
//! but ignored, so a config shared with zjstatus never errors.

use ansi_term::{
    ANSIString,
    Colour::{self, Fixed, RGB},
    Style,
};
use std::collections::BTreeMap;

/// Render a format-string `template` into styled segments, substituting each
/// `{name}` placeholder in `values` with its plain-text value beforehand.
///
/// `config` is the plugin configuration, used to resolve `$alias` colors via
/// `color_<alias>` keys (the same mechanism zjstatus uses). `dim` fades any
/// resolved RGB color toward gray by that amount (`0.0` = unchanged, `1.0` =
/// fully dimmed) the same way `dim_when_unfocused` dims the theme palette, so
/// a custom format string dims along with everything else rather than
/// staying at fixed brightness regardless of focus. See `dim_rgb`.
pub fn render_template(
    template: &str,
    values: &[(&str, &str)],
    config: &BTreeMap<String, String>,
    dim: f32,
) -> Vec<ANSIString<'static>> {
    let mut substituted = template.to_string();
    for (name, value) in values {
        substituted = substituted.replace(&format!("{{{}}}", name), value);
    }
    parse_format(&substituted, config, dim)
}

/// Parse a format string into styled segments. Text before the first `#[...]`
/// block is emitted unstyled; each block sets the style for the text following
/// it, up to the next block.
fn parse_format(
    template: &str,
    config: &BTreeMap<String, String>,
    dim: f32,
) -> Vec<ANSIString<'static>> {
    let mut parts: Vec<ANSIString<'static>> = vec![];

    for (idx, segment) in template.split("#[").enumerate() {
        if idx == 0 {
            // Text preceding the first styling block has no directives.
            if !segment.is_empty() {
                parts.push(Style::default().paint(segment.to_string()));
            }
            continue;
        }

        // `segment` is `<directives>]<text>`. A missing `]` means the block has
        // no trailing text (just directives), which we simply drop.
        if let Some((directives, text)) = segment.split_once(']') {
            let style = parse_style(directives, config, dim);
            if !text.is_empty() {
                parts.push(style.paint(text.to_string()));
            }
        }
    }

    parts
}

/// Parse the comma-separated directives inside a `#[...]` block into a style.
fn parse_style(directives: &str, config: &BTreeMap<String, String>, dim: f32) -> Style {
    let mut style = Style::new();

    for directive in directives.split(',') {
        let directive = directive.trim();
        if directive.is_empty() {
            continue;
        }

        if let Some(color) = directive.strip_prefix("fg=") {
            if let Some(color) = parse_color(color, config, dim) {
                style = style.fg(color);
            }
        } else if let Some(color) = directive.strip_prefix("bg=") {
            if let Some(color) = parse_color(color, config, dim) {
                style = style.on(color);
            }
        } else if directive.starts_with("us=") {
            // Underline color: parsed for compatibility but unsupported by the
            // renderer, so intentionally ignored.
        } else {
            style = apply_effect(style, directive);
        }
    }

    style
}

/// Apply a single text-effect directive to `style`, ignoring unknown effects.
fn apply_effect(style: Style, effect: &str) -> Style {
    match effect {
        "bold" => style.bold(),
        "italic" | "italics" => style.italic(),
        // zjstatus spells underline "underscore"; accept "underline" too.
        "underscore" | "underline" | "double-underscore" | "curly-underscore"
        | "dotted-underscore" | "dashed-underscore" => style.underline(),
        "blink" => style.blink(),
        "hidden" => style.hidden(),
        "dim" => style.dimmed(),
        "strikethrough" => style.strikethrough(),
        "reverse" => style.reverse(),
        _ => style,
    }
}

/// Parse a color string using the same rules as zjstatus. `dim` fades a
/// resolved RGB color toward gray (see `dim_rgb`); indexed and named ANSI
/// colors pass through unchanged, since they're indices into a
/// terminal-defined palette rather than RGB triples this can fade.
fn parse_color(color: &str, config: &BTreeMap<String, String>, dim: f32) -> Option<Colour> {
    let color = color.trim();

    // `$alias` resolves to the `color_<alias>` configuration value.
    let color = if let Some(alias) = color.strip_prefix('$') {
        config.get(&format!("color_{}", alias))?.as_str()
    } else {
        color
    };

    if let Some(hex) = color.strip_prefix('#') {
        return hex_to_rgb(hex, dim);
    }

    if let Some(named) = color_by_name(color) {
        return Some(named);
    }

    let index = color
        .strip_prefix("colour")
        .or_else(|| color.strip_prefix("color"))
        .unwrap_or(color);
    if let Ok(n) = index.parse::<u8>() {
        return Some(Fixed(n));
    }

    None
}

/// Parse a `RRGGBB` hex string (without a leading `#`) into an RGB color,
/// dimmed by `dim` (`0.0` = unchanged).
fn hex_to_rgb(hex: &str, dim: f32) -> Option<Colour> {
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    let (r, g, b) = dim_rgb(r, g, b, dim);
    Some(RGB(r, g, b))
}

/// A fully dimmed color sits at this fraction of its own brightness: dimmed
/// output should read as visibly darker, not just the same brightness with
/// the hue washed out. Mirrors the constant of the same name in `main.rs`
/// and the matching one in the zjstatus fork, so a custom format string
/// dims by the same amount as the theme-palette styling next to it.
const DIM_BRIGHTNESS: f32 = 0.3;

/// Fades an `(r, g, b)` triple toward a dark, desaturated gray by `strength`
/// (`0.0` = unchanged, `1.0` = fully dimmed). Each channel moves toward
/// `DIM_BRIGHTNESS` of the color's own perceived luminance (ITU-R BT.601 luma
/// weights) rather than toward a fixed midpoint or toward the color's own
/// unchanged brightness, so the color fades out its hue while also
/// darkening, ending at a dim neutral gray. See `main.rs`'s `dim_color` for
/// the same blend applied to the theme palette.
pub(crate) fn dim_rgb(r: u8, g: u8, b: u8, strength: f32) -> (u8, u8, u8) {
    if strength <= 0.0 {
        return (r, g, b);
    }
    let luminance = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    let target = luminance * DIM_BRIGHTNESS;
    let blend = |channel: u8| -> u8 {
        let channel = channel as f32;
        (channel + (target - channel) * strength)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    (blend(r), blend(g), blend(b))
}

/// Map a named color to an `ansi_term` color, matching zjstatus's names.
fn color_by_name(name: &str) -> Option<Colour> {
    let color = match name {
        "black" => Colour::Black,
        "red" => Colour::Red,
        "green" => Colour::Green,
        "yellow" => Colour::Yellow,
        "blue" => Colour::Blue,
        // ansi_term names ANSI magenta "Purple".
        "magenta" | "purple" => Colour::Purple,
        "cyan" => Colour::Cyan,
        "white" => Colour::White,
        "bright_black" | "bright-black" => Fixed(8),
        "bright_red" | "bright-red" => Fixed(9),
        "bright_green" | "bright-green" => Fixed(10),
        "bright_yellow" | "bright-yellow" => Fixed(11),
        "bright_blue" | "bright-blue" => Fixed(12),
        "bright_magenta" | "bright-magenta" => Fixed(13),
        "bright_cyan" | "bright-cyan" => Fixed(14),
        "bright_white" | "bright-white" => Fixed(15),
        _ => return None,
    };
    Some(color)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn parses_hex_and_named_and_indexed_colors() {
        let config = cfg(&[]);
        assert_eq!(parse_color("#010203", &config, 0.0), Some(RGB(1, 2, 3)));
        assert_eq!(parse_color("red", &config, 0.0), Some(Colour::Red));
        assert_eq!(parse_color("magenta", &config, 0.0), Some(Colour::Purple));
        assert_eq!(parse_color("bright_black", &config, 0.0), Some(Fixed(8)));
        assert_eq!(parse_color("5", &config, 0.0), Some(Fixed(5)));
        assert_eq!(parse_color("colour200", &config, 0.0), Some(Fixed(200)));
        assert_eq!(parse_color("nonsense", &config, 0.0), None);
        assert_eq!(parse_color("#12", &config, 0.0), None);
    }

    #[test]
    fn resolves_color_aliases() {
        let config = cfg(&[("color_accent", "#89b4fa")]);
        assert_eq!(
            parse_color("$accent", &config, 0.0),
            Some(RGB(0x89, 0xb4, 0xfa))
        );
        assert_eq!(parse_color("$missing", &config, 0.0), None);
    }

    #[test]
    fn template_substitutes_placeholders_and_styles() {
        let config = cfg(&[]);
        let parts = render_template(
            "#[fg=red,bold] {key} #[fg=white] {desc} ",
            &[("key", "Ctrl + p"), ("desc", "pane")],
            &config,
            0.0,
        );
        // Rendered ANSI should contain the substituted, styled text.
        let rendered = ansi_term::ANSIStrings(&parts).to_string();
        assert!(rendered.contains("Ctrl + p"));
        assert!(rendered.contains("pane"));
        // Bold (SGR 1) and red foreground (SGR 31) should appear.
        assert!(rendered.contains("1;31") || rendered.contains("31;1"));
    }

    #[test]
    fn leading_literal_text_is_unstyled() {
        let config = cfg(&[]);
        let parts = render_template("x#[fg=red]y", &[], &config, 0.0);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].to_string(), "x");
    }

    #[test]
    fn dim_fades_an_rgb_color_toward_gray_but_leaves_indexed_colors_alone() {
        let config = cfg(&[]);
        // Pure red at full dim strength: mirrors main.rs's
        // dim_color_blends_rgb_toward_gray, same DIM_BRIGHTNESS.
        assert_eq!(parse_color("#ff0000", &config, 1.0), Some(RGB(23, 23, 23)));
        assert_eq!(parse_color("#ff0000", &config, 0.0), Some(RGB(255, 0, 0)));
        // Named and indexed colors have no RGB triple to fade, so they pass
        // through unchanged regardless of dim strength.
        assert_eq!(parse_color("red", &config, 1.0), Some(Colour::Red));
        assert_eq!(parse_color("5", &config, 1.0), Some(Fixed(5)));
    }
}
