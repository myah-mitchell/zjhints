//! Parsing of zjstatus-style format strings.
//!
//! This mirrors the styling syntax used by [zjstatus](https://github.com/dj95/zjstatus)
//! so that a plugin's styles can be configured in exactly the same way as any
//! other zjstatus widget. A format string is a sequence of literal text and `#[...]`
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

/// A value for a placeholder of `render_filled`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fill<'a> {
    /// Plain text, drawn in the style of the text around the placeholder.
    Text(&'a str),
    /// Text styled already: the segments of another format string, as
    /// `render_template` returns them. Each segment is drawn over the style
    /// around the placeholder, so what its own style sets, such as a color
    /// or bold, takes the place of the surrounding one, and what it leaves
    /// unset, such as the background, stays as it is around it. A styled
    /// value so sits on the run it is placed in rather than cutting a hole
    /// in it. See `drawn_over`.
    Drawn(&'a [ANSIString<'a>]),
}

/// Render a format-string `template` into styled segments, like
/// `render_template`, but fill each `{name}` placeholder in `values` only
/// after the template is parsed, in one pass over it.
///
/// That makes a value safe to come from anywhere, such as a name the user
/// typed into Zellij: a `#[` in it is never read as a styling block, and a
/// `{name}` in it is never filled in turn. A placeholder takes each of its
/// pieces in order, so one placeholder can mix plain and styled text.
/// Placeholders not in `values` are left as they are written.
pub fn render_filled(
    template: &str,
    values: &[(&str, &[Fill<'_>])],
    config: &BTreeMap<String, String>,
    dim: f32,
) -> Vec<ANSIString<'static>> {
    let mut parts = vec![];
    for (style, text) in styled_runs(template, config, dim) {
        fill_run(style, text, values, &mut parts);
    }
    parts
}

/// Paint one run of template text in `style`, filling the placeholders in
/// it from `values`.
fn fill_run(
    style: Style,
    text: &str,
    values: &[(&str, &[Fill<'_>])],
    parts: &mut Vec<ANSIString<'static>>,
) {
    let mut plain = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        let placeholder = values.iter().find_map(|(name, pieces)| {
            let after = rest
                .strip_prefix('{')?
                .strip_prefix(name)?
                .strip_prefix('}')?;
            Some((after, pieces))
        });
        if let Some((after, pieces)) = placeholder {
            for piece in pieces.iter() {
                match piece {
                    Fill::Text(piece) => plain.push_str(piece),
                    Fill::Drawn(segments) => {
                        if !plain.is_empty() {
                            parts.push(style.paint(std::mem::take(&mut plain)));
                        }
                        for segment in segments.iter().filter(|segment| !segment.is_empty()) {
                            let own_style = *segment.style_ref();
                            let text = (**segment).to_string();
                            parts.push(drawn_over(style, own_style).paint(text));
                        }
                    }
                }
            }
            rest = after;
        } else {
            let mut characters = rest.chars();
            plain.extend(characters.next());
            rest = characters.as_str();
        }
    }
    if !plain.is_empty() {
        parts.push(style.paint(plain));
    }
}

/// `own` drawn over `surrounding`: each color `own` sets takes the place of
/// the surrounding one, and every effect either turns on, such as bold, is
/// on. A format string has no way to turn an effect off, so this is all a
/// segment can ask for.
fn drawn_over(surrounding: Style, own: Style) -> Style {
    Style {
        foreground: own.foreground.or(surrounding.foreground),
        background: own.background.or(surrounding.background),
        is_bold: own.is_bold || surrounding.is_bold,
        is_dimmed: own.is_dimmed || surrounding.is_dimmed,
        is_italic: own.is_italic || surrounding.is_italic,
        is_underline: own.is_underline || surrounding.is_underline,
        is_blink: own.is_blink || surrounding.is_blink,
        is_reverse: own.is_reverse || surrounding.is_reverse,
        is_hidden: own.is_hidden || surrounding.is_hidden,
        is_strikethrough: own.is_strikethrough || surrounding.is_strikethrough,
    }
}

/// Parse a format string into styled segments. Text before the first `#[...]`
/// block is emitted unstyled; each block sets the style for the text following
/// it, up to the next block.
fn parse_format(
    template: &str,
    config: &BTreeMap<String, String>,
    dim: f32,
) -> Vec<ANSIString<'static>> {
    styled_runs(template, config, dim)
        .into_iter()
        .map(|(style, text)| style.paint(text.to_string()))
        .collect()
}

/// The non-empty runs of text in a format string, each with the style it is
/// drawn in. Text before the first `#[...]` block has no style; each block
/// sets the style for the text following it, up to the next block.
fn styled_runs<'a>(
    template: &'a str,
    config: &BTreeMap<String, String>,
    dim: f32,
) -> Vec<(Style, &'a str)> {
    let mut runs = vec![];

    for (idx, segment) in template.split("#[").enumerate() {
        if idx == 0 {
            // Text preceding the first styling block has no directives.
            if !segment.is_empty() {
                runs.push((Style::default(), segment));
            }
            continue;
        }

        // `segment` is `<directives>]<text>`. A missing `]` means the block has
        // no trailing text (just directives), which we simply drop.
        if let Some((directives, text)) = segment.split_once(']') {
            let style = parse_style(directives, config, dim);
            if !text.is_empty() {
                runs.push((style, text));
            }
        }
    }

    runs
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
/// the hue washed out. Matches the one in the zjstatus fork, and the plugins
/// dim their theme palette through `dim_rgb` too, so a custom format string
/// dims by the same amount as the theme-palette styling next to it.
const DIM_BRIGHTNESS: f32 = 0.3;

/// Fades an `(r, g, b)` triple toward a dark, desaturated gray by `strength`
/// (`0.0` = unchanged, `1.0` = fully dimmed). Each channel moves toward
/// `DIM_BRIGHTNESS` of the color's own perceived luminance (ITU-R BT.601 luma
/// weights) rather than toward a fixed midpoint or toward the color's own
/// unchanged brightness, so the color fades out its hue while also
/// darkening, ending at a dim neutral gray. The plugins apply the same blend
/// to the theme palette through this function.
pub fn dim_rgb(r: u8, g: u8, b: u8, strength: f32) -> (u8, u8, u8) {
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

    fn rendered(parts: &[ANSIString<'static>]) -> String {
        ansi_term::ANSIStrings(parts).to_string()
    }

    #[test]
    fn filling_plain_values_draws_what_substituting_them_does() {
        let config = cfg(&[("color_accent", "#89b4fa")]);
        let template = "#[fg=$accent,bold] {index} #[fg=white]{name}{missing}";
        assert_eq!(
            rendered(&render_filled(
                template,
                &[
                    ("index", &[Fill::Text("1")]),
                    ("name", &[Fill::Text("editor")])
                ],
                &config,
                0.5,
            )),
            rendered(&render_template(
                template,
                &[("index", "1"), ("name", "editor")],
                &config,
                0.5,
            )),
        );
    }

    #[test]
    fn a_filled_value_is_never_read_as_styling_or_another_placeholder() {
        let config = cfg(&[]);
        let parts = render_filled(
            "#[bold]{name} {index}",
            &[
                ("name", &[Fill::Text("#[fg=red]{index}$x")]),
                ("index", &[Fill::Text("2")]),
            ],
            &config,
            0.0,
        );
        assert_eq!(
            rendered(&parts),
            Style::new()
                .bold()
                .paint("#[fg=red]{index}$x 2")
                .to_string()
        );
    }

    #[test]
    fn plain_text_takes_the_surrounding_style_and_drawn_text_is_drawn_over_it() {
        let config = cfg(&[]);
        let drawn = vec![
            Style::new().fg(Colour::Red).paint("!"),
            Style::default().paint("?"),
        ];
        let parts = render_filled(
            "#[bg=blue,bold]<{flags}>",
            &[(
                "flags",
                &[Fill::Text(" a"), Fill::Drawn(&drawn), Fill::Text("b")],
            )],
            &config,
            0.0,
        );
        let surrounding = Style::new().on(Colour::Blue).bold();
        let expected = vec![
            surrounding.paint("< a"),
            // The drawn color over the surrounding background and bold.
            surrounding.fg(Colour::Red).paint("!"),
            // An unstyled drawn segment is drawn as the text around it.
            surrounding.paint("?"),
            surrounding.paint("b>"),
        ];
        assert_eq!(rendered(&parts), rendered(&expected));
    }

    #[test]
    fn a_drawn_color_takes_the_place_of_the_surrounding_one_and_effects_add_up() {
        let config = cfg(&[]);
        let drawn = render_template("#[fg=red,bg=green,italic]!", &[], &config, 0.0);
        let parts = render_filled(
            "#[fg=white,bg=blue,bold]{flags}",
            &[("flags", &[Fill::Drawn(&drawn)])],
            &config,
            0.0,
        );
        let expected = Style::new()
            .fg(Colour::Red)
            .on(Colour::Green)
            .bold()
            .italic()
            .paint("!");
        assert_eq!(rendered(&parts), expected.to_string());
    }

    #[test]
    fn a_placeholder_with_no_pieces_or_an_unknown_name_draws_as_written() {
        let config = cfg(&[]);
        let parts = render_filled("{flags}{other}", &[("flags", &[])], &config, 0.0);
        assert_eq!(rendered(&parts), "{other}");
    }

    #[test]
    fn dim_fades_an_rgb_color_toward_gray_but_leaves_indexed_colors_alone() {
        let config = cfg(&[]);
        // Pure red at full dim strength: the same blend zjhints'
        // dim_color_blends_rgb_toward_gray checks for the theme palette.
        assert_eq!(parse_color("#ff0000", &config, 1.0), Some(RGB(23, 23, 23)));
        assert_eq!(parse_color("#ff0000", &config, 0.0), Some(RGB(255, 0, 0)));
        // Named and indexed colors have no RGB triple to fade, so they pass
        // through unchanged regardless of dim strength.
        assert_eq!(parse_color("red", &config, 1.0), Some(Colour::Red));
        assert_eq!(parse_color("5", &config, 1.0), Some(Fixed(5)));
    }
}
