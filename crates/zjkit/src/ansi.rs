//! Measuring and cutting text that carries ANSI escape sequences.
//!
//! Everything here counts terminal columns, not characters or bytes, and
//! skips escape sequences, so styled text measures the way it draws.

use ansi_term::{ANSIString, ANSIStrings};
use std::borrow::Cow;
use unicode_width::UnicodeWidthChar;

/// SGR reset. Terminates a truncated styled run so its colors stop at the cut.
pub const ANSI_RESET: &str = "\u{1b}[0m";

/// Splits text into escape sequences and the characters between them.
pub struct AnsiParser<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> AnsiParser<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            chars: text.chars().peekable(),
        }
    }

    /// The next escape sequence or visible character, or `None` at the end.
    pub fn next_segment(&mut self) -> Option<AnsiSegment> {
        let ch = self.chars.next()?;

        if ch == '\x1b' {
            let mut escape_seq = String::from(ch);
            for escape_ch in self.chars.by_ref() {
                escape_seq.push(escape_ch);
                if escape_ch == 'm' {
                    break;
                }
            }
            Some(AnsiSegment::EscapeSequence(escape_seq))
        } else {
            Some(AnsiSegment::VisibleChar(ch))
        }
    }
}

/// One piece of text, as `AnsiParser` reads it.
pub enum AnsiSegment {
    EscapeSequence(String),
    VisibleChar(char),
}

/// Columns the text occupies on screen, ignoring escape sequences.
///
/// Counting characters is not the same as counting columns: a CJK ideograph or
/// an emoji takes two. `wide_ambiguous` decides the East Asian Ambiguous class,
/// which includes every Nerd Font glyph — narrow by the standard, but two
/// columns in a terminal actually configured to show them.
pub fn calculate_visible_length(text: &str, wide_ambiguous: bool) -> usize {
    let mut parser = AnsiParser::new(text);
    let mut len = 0;

    while let Some(segment) = parser.next_segment() {
        if let AnsiSegment::VisibleChar(ch) = segment {
            len += char_width(ch, wide_ambiguous);
        }
    }

    len
}

/// `text` with every control character written out as an escape, such as
/// `\u{1b}` for ESC or `\n` for a newline, so that it shows as text instead
/// of acting on the terminal. For plain text from the config or a nested
/// session, never for ANSI the plugin styled itself.
pub fn show_controls(text: &str) -> Cow<'_, str> {
    if !text.chars().any(char::is_control) {
        return Cow::Borrowed(text);
    }
    let mut shown = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_control() {
            shown.extend(ch.escape_default());
        } else {
            shown.push(ch);
        }
    }
    Cow::Owned(shown)
}

/// Columns `ch` takes on screen; see `calculate_visible_length` for
/// `wide_ambiguous`.
pub fn char_width(ch: char, wide_ambiguous: bool) -> usize {
    let width = if wide_ambiguous {
        ch.width_cjk()
    } else {
        ch.width()
    };
    // Control characters report no width; treat them as taking none.
    width.unwrap_or(0)
}

/// `text` cut to `max_len` columns, ending in `overflow_str` when anything
/// was cut. Escape sequences are kept whole, and a styled result is reset at
/// the end so the cut cannot leave a color painting what follows.
pub fn truncate_ansi_string(
    text: &str,
    overflow_str: &str,
    max_len: usize,
    wide_ambiguous: bool,
) -> String {
    let visible_len = calculate_visible_length(text, wide_ambiguous);
    // Columns on screen, not characters or bytes: `…` is one column, or two
    // where ambiguous characters are wide.
    let overflow_len = calculate_visible_length(overflow_str, wide_ambiguous);

    if visible_len <= max_len {
        return text.to_string();
    }

    // Too narrow for even the marker, so show as much of it as fits rather than
    // overshooting the limit the caller asked for. A styled marker keeps its
    // escape sequences whole, and is reset after the cut.
    if max_len <= overflow_len {
        let mut shown = String::new();
        let mut columns = 0;
        let mut styled = false;
        let mut parser = AnsiParser::new(overflow_str);
        while let Some(segment) = parser.next_segment() {
            match segment {
                AnsiSegment::EscapeSequence(seq) => {
                    shown.push_str(&seq);
                    styled = true;
                }
                AnsiSegment::VisibleChar(ch) => {
                    columns += char_width(ch, wide_ambiguous);
                    if columns > max_len {
                        break;
                    }
                    shown.push(ch);
                }
            }
        }
        if styled {
            shown.push_str(ANSI_RESET);
        }
        return shown;
    }

    let target_len = max_len - overflow_len;
    let mut result = String::new();
    let mut visible_count = 0;
    let mut styled = overflow_str.contains('\u{1b}');
    let mut parser = AnsiParser::new(text);

    while let Some(segment) = parser.next_segment() {
        match segment {
            AnsiSegment::EscapeSequence(seq) => {
                result.push_str(&seq);
                styled = true;
            }
            AnsiSegment::VisibleChar(ch) => {
                // A wide character that would cross the limit is left out
                // whole, so the cut can fall a column short of it.
                let width = char_width(ch, wide_ambiguous);
                if visible_count + width > target_len {
                    break;
                }
                result.push(ch);
                visible_count += width;
            }
        }
    }

    result.push_str(overflow_str);
    // `ANSIStrings` closes the styled run with a reset, which the loop above
    // discards by breaking early. Without one, whatever color was active at the
    // cut keeps painting the rest of the status bar. Plain text needs none.
    if styled && !result.ends_with(ANSI_RESET) {
        result.push_str(ANSI_RESET);
    }
    result
}

/// Columns a run of styled segments occupies on screen.
pub fn visible_width(parts: &[ANSIString<'static>], wide_ambiguous: bool) -> usize {
    calculate_visible_length(&format!("{}", ANSIStrings(parts)), wide_ambiguous)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ansi_term::{Colour::Fixed, Style};

    #[test]
    fn truncating_closes_the_styled_run_so_colour_stops_at_the_cut() {
        let truncated = truncate_ansi_string(&styled_line(), "...", 6, false);
        assert!(
            truncated.ends_with(ANSI_RESET),
            "colour would bleed past the cut: {:?}",
            truncated
        );
    }

    #[test]
    fn truncating_plain_text_adds_no_reset() {
        assert_eq!(truncate_ansi_string("a long entry", "…", 5, false), "a lo…");
    }

    #[test]
    fn truncating_keeps_the_visible_width_within_the_limit() {
        for limit in 1..=10 {
            let truncated = truncate_ansi_string(&styled_line(), "...", limit, false);
            assert!(
                calculate_visible_length(&truncated, false) <= limit,
                "limit {} produced {:?}",
                limit,
                truncated
            );
        }
    }

    #[test]
    fn control_characters_are_shown_as_escapes() {
        assert_eq!(
            show_controls("\u{1b}]52;c;aGk=\u{7}"),
            "\\u{1b}]52;c;aGk=\\u{7}"
        );
        assert_eq!(
            show_controls("a\nb\tc\u{7f}\u{9b}"),
            "a\\nb\\tc\\u{7f}\\u{9b}"
        );
        assert!(matches!(show_controls("漢字 → ok"), Cow::Borrowed(_)));
    }

    #[test]
    fn truncating_counts_wide_characters_as_two_columns() {
        let line = "漢字テスト";
        // Five columns leave four for the text and one for the marker.
        let truncated = truncate_ansi_string(line, "…", 6, false);
        assert_eq!(truncated, "漢字…");
        // A character that would cross the limit is left out, not split.
        let truncated = truncate_ansi_string(line, "…", 7, false);
        assert_eq!(truncated, "漢字テ…");
        for limit in 0..=10 {
            for wide_ambiguous in [false, true] {
                let truncated = truncate_ansi_string(line, "…", limit, wide_ambiguous);
                assert!(
                    calculate_visible_length(&truncated, wide_ambiguous) <= limit,
                    "limit {limit} produced {truncated:?}"
                );
            }
        }
    }

    #[test]
    fn an_ambiguous_marker_takes_two_columns_where_they_are_wide() {
        // `…` is two columns, so a three-column cut keeps one of the text.
        let truncated = truncate_ansi_string("abcdef", "…", 3, true);
        assert_eq!(truncated, "a…");
        assert_eq!(truncate_ansi_string("abcdef", "…", 1, true), "");
        assert_eq!(truncate_ansi_string("abcdef", "…", 1, false), "…");
    }

    #[test]
    fn a_styled_marker_cut_short_keeps_its_escapes_whole() {
        let marker = "\u{1b}[0m\u{1b}[38;2;255;0;0m…>";
        let truncated = truncate_ansi_string("abcdef", marker, 1, false);
        assert_eq!(truncated, "\u{1b}[0m\u{1b}[38;2;255;0;0m…\u{1b}[0m");
        assert_eq!(calculate_visible_length(&truncated, false), 1);
    }

    #[test]
    fn width_counts_columns_not_characters() {
        // A CJK ideograph is two columns in any terminal.
        assert_eq!(calculate_visible_length("ab", false), 2);
        assert_eq!(calculate_visible_length("字", false), 2);
        assert_eq!(calculate_visible_length("a字b", false), 4);
    }

    #[test]
    fn nerd_font_glyphs_are_ambiguous_width() {
        // U+F060 is the arrow glyph used for `key_alias_left`. It is East Asian
        // Ambiguous: one column by the standard, two in a terminal set up for
        // Nerd Fonts.
        assert_eq!(calculate_visible_length("\u{f060}", false), 1);
        assert_eq!(calculate_visible_length("\u{f060}", true), 2);
    }

    #[test]
    fn escape_sequences_take_no_columns() {
        assert_eq!(calculate_visible_length(&styled_line(), false), 8);
        assert_eq!(calculate_visible_length(&styled_line(), true), 8);
    }

    /// A styled line of the shape the plugin actually emits: coloured segments
    /// closed by the reset `ANSIStrings` appends.
    fn styled_line() -> String {
        let parts = vec![
            Style::new().on(Fixed(1)).paint("aaaa"),
            Style::new().on(Fixed(2)).paint("bbbb"),
        ];
        format!("{}", ANSIStrings(&parts))
    }

    #[test]
    fn an_untruncated_line_already_ends_reset() {
        assert!(styled_line().ends_with(ANSI_RESET));
    }

    #[test]
    fn a_line_that_fits_is_left_exactly_as_it_was() {
        let line = styled_line();
        assert_eq!(truncate_ansi_string(&line, "...", 100, false), line);
    }
}
