//! Measuring and cutting text that carries ANSI escape sequences.
//!
//! Everything here counts terminal columns, not characters or bytes, and
//! skips escape sequences, so styled text measures the way it draws.

use ansi_term::{ANSIString, ANSIStrings};
use std::borrow::Cow;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

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
/// Counting characters is not the same as counting columns: a CJK ideograph
/// takes two, and an emoji built from several characters, such as a flag or
/// a skin-toned hand, takes two for the whole of it. The text is measured the
/// way Zellij lays out a plugin's pane, one grapheme cluster at a time (see
/// `cluster_width`).
pub fn calculate_visible_length(text: &str) -> usize {
    runs(text)
        .map(|run| match run {
            Run::Text(text) => text_width(text),
            Run::Escape(_) => 0,
        })
        .sum()
}

/// A piece of ANSI text: an escape sequence, or the visible text between two.
enum Run<'a> {
    Escape(&'a str),
    Text(&'a str),
}

/// `text` split into escape sequences and the runs of visible text between
/// them. An escape sequence runs from ESC up to and including the first `m`,
/// as `AnsiParser` reads it, so both agree on what is visible.
fn runs(text: &str) -> impl Iterator<Item = Run<'_>> {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let (run, end) = if rest.starts_with('\u{1b}') {
            let end = rest.find('m').map_or(rest.len(), |at| at + 1);
            (Run::Escape(&rest[..end]), end)
        } else {
            let end = rest.find('\u{1b}').unwrap_or(rest.len());
            (Run::Text(&rest[..end]), end)
        };
        rest = &rest[end..];
        Some(run)
    })
}

/// Columns one grapheme cluster takes in a Zellij plugin pane.
///
/// A grapheme cluster is what a reader sees as one character: `e` with a
/// combining accent, `❤️` (a heart and the selector asking for its emoji
/// form), or `👩‍💻` (two emoji joined into one). Zellij gives each cluster
/// one cell of the width this returns, so measuring its characters one by one
/// gets the width wrong: `❤` alone is one column but `❤️` is two, and the
/// three characters of `👩‍💻` are two columns together, not four.
///
/// This is the rule of `zellij_utils::graphemes::cluster_width`, kept here so
/// zjkit builds against a zellij-utils without that module. ASCII counts its
/// printable bytes; anything else takes its Unicode width, with East Asian
/// Ambiguous characters, Nerd Font glyphs among them, one column wide.
pub fn cluster_width(cluster: &str) -> usize {
    if cluster.is_ascii() {
        cluster.bytes().filter(|b| !b.is_ascii_control()).count()
    } else {
        cluster.width()
    }
}

/// Columns plain `text` (no escape sequences) takes, cluster by cluster.
pub fn text_width(text: &str) -> usize {
    text.graphemes(true).map(cluster_width).sum()
}

/// The longest start of plain `text` that fits in `max` columns, and the
/// columns it takes. It ends between two grapheme clusters, so a cluster
/// that would cross the limit is left out whole rather than split, and the
/// result can fall short of `max`.
pub fn prefix_within(text: &str, max: usize) -> (&str, usize) {
    let mut used = 0;
    for (start, cluster) in text.grapheme_indices(true) {
        let width = cluster_width(cluster);
        if used + width > max {
            return (&text[..start], used);
        }
        used += width;
    }
    (text, used)
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

/// Columns `ch` takes on its own, with control characters taking none.
///
/// Only for a grid of one character per cell, such as the documentation
/// images draw. Text in a plugin's pane is laid out by grapheme cluster, so
/// measure it with `calculate_visible_length` or `text_width` instead: a
/// character's own width is wrong for one that is part of a larger cluster.
pub fn char_width(ch: char) -> usize {
    ch.width().unwrap_or(0)
}

/// `text` cut to `max_len` columns, ending in `overflow_str` when anything
/// was cut. Escape sequences are kept whole, a grapheme cluster crossing the
/// limit is dropped whole, and a styled result is reset at the end so the
/// cut cannot leave a color painting what follows.
pub fn truncate_ansi_string(text: &str, overflow_str: &str, max_len: usize) -> String {
    let visible_len = calculate_visible_length(text);
    // Columns on screen, not characters or bytes: `…` is one column.
    let overflow_len = calculate_visible_length(overflow_str);

    if visible_len <= max_len {
        return text.to_string();
    }

    // Too narrow for even the marker, so show as much of it as fits rather than
    // overshooting the limit the caller asked for. A styled marker keeps its
    // escape sequences whole, and is reset after the cut.
    if max_len <= overflow_len {
        let (mut shown, styled) = cut_runs(overflow_str, max_len);
        if styled {
            shown.push_str(ANSI_RESET);
        }
        return shown;
    }

    let (mut result, styled) = cut_runs(text, max_len - overflow_len);
    result.push_str(overflow_str);
    // `ANSIStrings` closes the styled run with a reset, which the cut
    // discards with the rest of the line. Without one, whatever color was
    // active at the cut keeps painting the rest of the status bar. Plain text
    // needs none.
    if (styled || overflow_str.contains('\u{1b}')) && !result.ends_with(ANSI_RESET) {
        result.push_str(ANSI_RESET);
    }
    result
}

/// The start of ANSI `text` that fits in `max` columns, every escape sequence
/// before the cut kept whole, and whether it holds any escape sequence.
fn cut_runs(text: &str, max: usize) -> (String, bool) {
    let mut kept = String::new();
    let mut styled = false;
    let mut room = max;
    for run in runs(text) {
        match run {
            Run::Escape(seq) => {
                kept.push_str(seq);
                styled = true;
            }
            Run::Text(text) => {
                let (fits, used) = prefix_within(text, room);
                kept.push_str(fits);
                if fits.len() < text.len() {
                    break;
                }
                room -= used;
            }
        }
    }
    (kept, styled)
}

/// Columns a run of styled segments occupies on screen.
pub fn visible_width(parts: &[ANSIString<'static>]) -> usize {
    calculate_visible_length(&format!("{}", ANSIStrings(parts)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ansi_term::{Colour::Fixed, Style};

    #[test]
    fn truncating_closes_the_styled_run_so_colour_stops_at_the_cut() {
        let truncated = truncate_ansi_string(&styled_line(), "...", 6);
        assert!(
            truncated.ends_with(ANSI_RESET),
            "colour would bleed past the cut: {:?}",
            truncated
        );
    }

    #[test]
    fn truncating_plain_text_adds_no_reset() {
        assert_eq!(truncate_ansi_string("a long entry", "…", 5), "a lo…");
    }

    #[test]
    fn truncating_keeps_the_visible_width_within_the_limit() {
        for limit in 1..=10 {
            let truncated = truncate_ansi_string(&styled_line(), "...", limit);
            assert!(
                calculate_visible_length(&truncated) <= limit,
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
        let truncated = truncate_ansi_string(line, "…", 6);
        assert_eq!(truncated, "漢字…");
        // A character that would cross the limit is left out, not split.
        let truncated = truncate_ansi_string(line, "…", 7);
        assert_eq!(truncated, "漢字テ…");
        for limit in 0..=10 {
            let truncated = truncate_ansi_string(line, "…", limit);
            assert!(
                calculate_visible_length(&truncated) <= limit,
                "limit {limit} produced {truncated:?}"
            );
        }
    }

    #[test]
    fn the_marker_takes_one_column() {
        // `…` is East Asian Ambiguous, which Zellij lays out one column wide.
        assert_eq!(truncate_ansi_string("abcdef", "…", 3), "ab…");
        assert_eq!(truncate_ansi_string("abcdef", "…", 1), "…");
        assert_eq!(truncate_ansi_string("abcdef", "…", 0), "");
    }

    #[test]
    fn a_styled_marker_cut_short_keeps_its_escapes_whole() {
        let marker = "\u{1b}[0m\u{1b}[38;2;255;0;0m…>";
        let truncated = truncate_ansi_string("abcdef", marker, 1);
        assert_eq!(truncated, "\u{1b}[0m\u{1b}[38;2;255;0;0m…\u{1b}[0m");
        assert_eq!(calculate_visible_length(&truncated), 1);
    }

    #[test]
    fn a_marker_cut_short_drops_a_cluster_crossing_the_limit_whole() {
        // `👍🏽` is a hand and a skin tone: two columns together, so a
        // one-column cut has no room for it and does not keep the hand alone.
        assert_eq!(truncate_ansi_string("abcdef", "👍🏽", 1), "");
        assert_eq!(truncate_ansi_string("abcdef", "👍🏽", 2), "👍🏽");
    }

    #[test]
    fn width_counts_columns_not_characters() {
        // A CJK ideograph is two columns in any terminal.
        assert_eq!(calculate_visible_length("ab"), 2);
        assert_eq!(calculate_visible_length("字"), 2);
        assert_eq!(calculate_visible_length("a字b"), 4);
    }

    /// Text whose characters measured one by one give a different width from
    /// the one Zellij lays it out at, and that width.
    const CLUSTERS: &[(&str, usize)] = &[
        // An emoji asked for in its emoji form by U+FE0F, the selector that
        // turns a one-column symbol into a two-column picture.
        ("❤️", 2),
        ("⚠️", 2),
        ("1️⃣", 2),
        // Emoji joined into one by U+200D, the zero-width joiner.
        ("👩‍💻", 2),
        // An emoji with a skin tone, and both together.
        ("👍🏽", 2),
        ("🧑🏽‍💻", 2),
        // A flag: two regional indicator letters, J and P.
        ("🇯🇵", 2),
        // `e` and a combining acute accent, drawn as one `é`.
        ("e\u{301}", 1),
        ("日本語", 6),
        // A Nerd Font glyph, in the Private Use Area: one column, as Zellij
        // lays out every East Asian Ambiguous character.
        ("\u{f060}", 1),
        ("\u{1b}[31m❤️\u{1b}[0m", 2),
    ];

    #[test]
    fn a_grapheme_cluster_is_measured_whole() {
        for &(text, width) in CLUSTERS {
            assert_eq!(calculate_visible_length(text), width, "{text:?}");
        }
    }

    #[test]
    fn a_cluster_crossing_the_limit_is_dropped_whole() {
        assert_eq!(truncate_ansi_string("👩‍💻 dev", "…", 3), "👩‍💻…");
        // One column is left for the text, too few for the heart.
        assert_eq!(truncate_ansi_string("❤️ love", "…", 2), "…");
        // Never one regional indicator of a flag on its own.
        assert_eq!(truncate_ansi_string("🇯🇵🇯🇵", "…", 2), "…");
        assert_eq!(truncate_ansi_string("🇯🇵🇯🇵", "…", 3), "🇯🇵…");
    }

    #[test]
    fn every_cut_fits_and_keeps_whole_clusters() {
        let samples = [
            "🚀 build",
            "日本語",
            "❤️❤️❤️",
            "❤️ love",
            "👩‍💻 dev",
            "👩‍💻👩‍💻",
            "👍🏽👍🏽",
            "🇯🇵🇯🇵",
            "e\u{301}e\u{301}e\u{301}x",
            "\u{f121} code",
            "1️⃣2️⃣3️⃣",
            "🧑🏽‍💻 at work",
        ];
        for text in samples {
            let clusters: Vec<&str> = text.graphemes(true).collect();
            for limit in 0..=10 {
                let cut = truncate_ansi_string(text, "…", limit);
                assert!(
                    calculate_visible_length(&cut) <= limit,
                    "{text:?} at {limit} gave {cut:?}"
                );
                // What is left of the text, without the marker, is a run of
                // its first clusters, each of them whole.
                let kept = cut.strip_suffix('…').unwrap_or(&cut);
                let kept: Vec<&str> = kept.graphemes(true).collect();
                assert_eq!(
                    kept,
                    clusters[..kept.len()],
                    "{text:?} at {limit} gave {cut:?}"
                );
            }
        }
    }

    #[test]
    fn a_styled_cut_keeps_whole_clusters() {
        let line = format!("{}", Style::new().on(Fixed(1)).paint("👩‍💻👩‍💻"));
        let cut = truncate_ansi_string(&line, "…", 3);
        assert_eq!(calculate_visible_length(&cut), 3);
        assert!(cut.contains("👩‍💻…"), "{cut:?}");
        assert!(cut.ends_with(ANSI_RESET), "{cut:?}");
    }

    #[test]
    fn a_prefix_ends_between_clusters() {
        assert_eq!(prefix_within("👩‍💻 dev", 1), ("", 0));
        assert_eq!(prefix_within("👩‍💻 dev", 3), ("👩‍💻 ", 3));
        assert_eq!(prefix_within("ab", 5), ("ab", 2));
    }

    /// The same widths as Zellij's own measure, which needs a zellij-utils
    /// with the `graphemes` module: the fork the workspace builds against,
    /// or an upstream release once it ships there.
    #[test]
    fn widths_match_zellij() {
        let samples = CLUSTERS
            .iter()
            .map(|&(text, _)| text)
            .filter(|text| !text.contains('\u{1b}'))
            .chain(["🏳️‍🌈", "⇧", "─", "…", "ab\u{200b}c", "\t", "\u{ad}x", "कि"]);
        for text in samples {
            assert_eq!(
                text_width(text),
                zellij_utils::graphemes::display_width(text),
                "{text:?}"
            );
            for cluster in text.graphemes(true) {
                assert_eq!(
                    cluster_width(cluster),
                    zellij_utils::graphemes::cluster_width(cluster),
                    "{cluster:?}"
                );
            }
        }
    }

    #[test]
    fn escape_sequences_take_no_columns() {
        assert_eq!(calculate_visible_length(&styled_line()), 8);
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
        assert_eq!(truncate_ansi_string(&line, "...", 100), line);
    }
}
