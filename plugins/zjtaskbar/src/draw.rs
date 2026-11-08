//! Drawing the bar: the entries, each from its format, the run of them that
//! fits around the focused one, the markers for the rest, the separators
//! between the tabs' groups, and what an empty bar says. Each line comes
//! with the columns its entries and markers were drawn on, for the mouse.

use crate::entries::Entry;
use crate::flags::FLAGS;
use crate::State;
use ansi_term::{ANSIString, ANSIStrings};
use std::borrow::Cow;
use zellij_tile::prelude::*;
use zjkit::ansi::{calculate_visible_length, cut_line, show_controls, CutLine};
use zjkit::fit::{self, once_per_count};
use zjkit::format::{self, Fill};
use zjkit::mouse::ColumnSpans;
use zjkit::nesting;
use zjkit::theme::zellij_look;

/// What an entry looks like in Zellij's own colors, when its format is
/// unset: its title and its flags on a ribbon, the way Zellij's own tab bar
/// draws a tab's name.
const ZELLIJ_ENTRY: &str = " {title}{flags} ";
/// What the markers for hidden entries look like in Zellij's own colors,
/// when no format is set: the arrows and count of Zellij's own tab bar.
const ZELLIJ_OVERFLOW_LEFT: &str = " \u{2190} +{count} ";
const ZELLIJ_OVERFLOW_RIGHT: &str = " +{count} \u{2192} ";
/// What of `ModeInfo` the bar shows: the nested-session state that hides or
/// dims it. Compared to tell whether a mode update changes anything on
/// screen. See `State::mode_view`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ModeView {
    nested: bool,
    host_ui_is_covered: bool,
    host_descended: bool,
    ascended_out_of: bool,
}

/// What a stretch of the drawn line stands for, for the mouse. The spacers,
/// the separators and the end of a cut line stand for nothing.
///
/// Each names a pane by its id, not its place on the bar, so a click acts on
/// the pane it landed on even if a `PaneUpdate` has moved the entries since
/// the line was drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Clickable {
    /// The entry of the pane.
    Entry(PaneId),
    /// A marker for hidden entries, with the pane of the hidden entry
    /// nearest to it. A left click on it acts as one on that entry would.
    Marker(PaneId),
}

/// How an entry is drawn, which picks its format and its Zellij look.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Look {
    Focused,
    Minimized,
    Normal,
}

impl Look {
    /// A focused pane cannot also be minimized, as the keyboard never goes
    /// to a hidden pane, so the order between the two never matters.
    fn of(entry: &Entry) -> Self {
        if entry.focused {
            Look::Focused
        } else if entry.minimized {
            Look::Minimized
        } else {
            Look::Normal
        }
    }
}

impl State {
    /// What of `mode_info` the bar shows, with the options as read now.
    pub(crate) fn mode_view(&self, mode_info: &ModeInfo) -> ModeView {
        ModeView {
            nested: nesting::is_nested(mode_info),
            host_ui_is_covered: nesting::host_ui_is_covered(mode_info),
            host_descended: nesting::is_host_descended(mode_info),
            ascended_out_of: nesting::is_ascended_out_of(mode_info),
        }
    }

    /// How wide the line may be, or `None` for no limit.
    ///
    /// `max_length` is a hard cap the user set; `auto_width` fits the bar's
    /// own pane once its width is known, up to its right edge as the last
    /// render drew it (see `BarCore::edge`). With both in play the smaller
    /// wins, so an explicit cap is never exceeded just because the pane is
    /// wide.
    pub(crate) fn length_limit(&self) -> Option<usize> {
        let explicit = (self.max_length > 0).then_some(self.max_length);
        let auto = self.auto_width.then(|| self.bar.edge()).flatten();
        match (explicit, auto) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (limit, None) | (None, limit) => limit,
        }
    }

    /// The dim strength to draw with right now. See `nesting::dim_amount`.
    fn dim_amount(&self) -> f32 {
        nesting::dim_amount(&self.mode_info, self.dim_when_unfocused, self.dim_strength)
    }

    /// Whether the bar draws nothing because this session is nested and the
    /// host's own bars are on screen beside it. Once the host has covered
    /// its own bars to give this session the whole display, that is no
    /// longer true. The same rule as zjtabs and zjhints.
    fn hidden_by_nesting(&self) -> bool {
        self.hide_when_nested
            && nesting::is_nested(&self.mode_info)
            && !nesting::host_ui_is_covered(&self.mode_info)
    }

    /// Whether the bar draws an empty line whatever it lists: it is hidden,
    /// by its option or a message, or by nesting.
    pub(crate) fn draws_nothing(&self) -> bool {
        self.bar.visibility.is_hidden() || self.hidden_by_nesting()
    }

    /// The line to draw: the entries that fit, or `empty_format` while there
    /// are none, or nothing while `draws_nothing` says so. With it, the
    /// columns each entry and marker it shows was drawn on.
    pub(crate) fn draw_bar(&self) -> (String, ColumnSpans<Clickable>) {
        if self.draws_nothing() {
            return (String::new(), ColumnSpans::default());
        }
        let dim = self.dim_amount();
        let limit = self.length_limit();
        let nothing_listed = self.entries.is_empty() && self.empty_format.is_none();
        if nothing_listed || limit == Some(0) {
            return (String::new(), ColumnSpans::default());
        }
        self.draw_listed(limit, dim, 0)
    }

    /// The entries that fit into `limit` columns, or `empty_format` while
    /// there are none, with the columns each was drawn on counted from
    /// `start`, the column the first of them is drawn on.
    fn draw_listed(
        &self,
        limit: Option<usize>,
        dim: f32,
        start: usize,
    ) -> (String, ColumnSpans<Clickable>) {
        if self.entries.is_empty() {
            return (self.draw_empty(limit, dim), ColumnSpans::default());
        }
        self.draw_entries(limit, dim, start)
    }

    /// What the bar draws with no entries: `empty_format`, cut to `limit`
    /// columns, or nothing while it is unset.
    fn draw_empty(&self, limit: Option<usize>, dim: f32) -> String {
        let Some(format) = self.empty_format.as_deref() else {
            return String::new();
        };
        let line = self.draw_format(format, dim);
        match limit.and_then(|limit| self.cut(&line, limit, dim)) {
            Some(cut) => cut.line,
            None => line,
        }
    }

    /// The entries and markers, fitted into `limit` columns, or any width
    /// for `None`, with the columns each was drawn on counted from `start`.
    ///
    /// The window of entries shown is kept around the focused one, or the
    /// first while none is focused. Neighboring entries are joined by
    /// `entry_spacer`, or by `tab_separator` where the one after starts the
    /// next tab's group, and a marker by the spacer.
    ///
    /// `fit::window` takes one gap width for every pair of neighbors. So
    /// that the line it picks still fits where a separator wider than the
    /// spacer takes a gap's place, each entry that starts a group is counted
    /// wider by the difference. That is exact when the entry before it is
    /// shown too, and leaves a few columns spare when it is behind a marker
    /// instead, so the window never picks more than fits.
    ///
    /// A marker stands for the hidden entry nearest to it, and what the cut
    /// of an overlong line leaves of an entry stands for that entry.
    fn draw_entries(
        &self,
        limit: Option<usize>,
        dim: f32,
        start: usize,
    ) -> (String, ColumnSpans<Clickable>) {
        let width = |text: &str| calculate_visible_length(text);

        let drawn_entries: Vec<String> = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| self.draw_entry(entry, index, dim))
            .collect();
        let spacer = self.draw_format(&self.entry_spacer, dim);
        let separator = self.draw_format(&self.tab_separator, dim);
        let separator_extra = width(&separator).saturating_sub(width(&spacer));
        let starts_group = |index: usize| {
            index > 0 && self.entries[index - 1].tab_position != self.entries[index].tab_position
        };
        let entry_widths: Vec<usize> = drawn_entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let extra = if starts_group(index) {
                    separator_extra
                } else {
                    0
                };
                width(entry) + extra
            })
            .collect();
        let left = |count: usize| self.draw_marker(Side::Left, count, dim);
        let right = |count: usize| self.draw_marker(Side::Right, count, dim);
        let pinned = self
            .entries
            .iter()
            .position(|entry| entry.focused)
            .unwrap_or(0);

        let window = fit::window(
            &entry_widths,
            pinned,
            width(&spacer),
            limit.unwrap_or(usize::MAX),
            once_per_count(|count| width(&left(count))),
            once_per_count(|count| width(&right(count))),
        );

        // The pieces of the line in drawing order, joined as described
        // above, each with what it stands for. The markers are left out when
        // `with_markers` is false.
        let lay_out = |with_markers: bool| {
            let mut pieces: Vec<(String, Option<Clickable>)> = vec![];
            let has_left_marker = with_markers && window.hidden_before > 0;
            if has_left_marker {
                let nearest = self.entries[window.range.start - 1].pane;
                let marker = left(window.hidden_before);
                pieces.push((marker, Some(Clickable::Marker(nearest))));
            }
            for index in window.range.clone() {
                if index > window.range.start || has_left_marker {
                    let shown_before = index > window.range.start;
                    if shown_before && starts_group(index) {
                        pieces.push((separator.clone(), None));
                    } else {
                        pieces.push((spacer.clone(), None));
                    }
                }
                let entry = Clickable::Entry(self.entries[index].pane);
                pieces.push((drawn_entries[index].clone(), Some(entry)));
            }
            if with_markers && window.hidden_after > 0 {
                let nearest = self.entries[window.range.end].pane;
                let marker = right(window.hidden_after);
                pieces.push((spacer.clone(), None));
                pieces.push((marker, Some(Clickable::Marker(nearest))));
            }
            pieces
        };
        let joined = |pieces: &[(String, Option<Clickable>)]| -> String {
            pieces.iter().map(|(text, _)| text.as_str()).collect()
        };
        let mut pieces = lay_out(true);
        // The window keeps the pinned entry even when it and its markers
        // alone are wider than the line may be, and the cut below takes from
        // the right, which could leave the left marker and none of the entry.
        // So the markers go first, and the entry is cut on its own, as a lone
        // entry too wide for the bar is.
        let hides_entries = window.hidden_before > 0 || window.hidden_after > 0;
        if hides_entries && limit.is_some_and(|limit| width(&joined(&pieces)) > limit) {
            pieces = lay_out(false);
        }
        let line = joined(&pieces);
        let (line, kept) = match limit.and_then(|limit| self.cut(&line, limit, dim)) {
            Some(cut) => (cut.line, cut.kept),
            None => (line, usize::MAX),
        };
        // What the cut left of each piece stands for it; the overflow string
        // after the cut stands for nothing.
        let mut spans = ColumnSpans::default();
        let mut column = 0;
        for (text, target) in &pieces {
            let columns = width(text);
            if let Some(target) = target {
                let end = (column + columns).min(kept);
                spans.push((start + column)..(start + end), *target);
            }
            column += columns;
        }
        (line, spans)
    }

    /// `line` cut to `limit` columns and ended with `overflow_str`, or `None`
    /// when it already fits. See `zjkit::ansi::cut_line`.
    fn cut(&self, line: &str, limit: usize, dim: f32) -> Option<CutLine> {
        cut_line(
            line,
            &self.overflow_str,
            limit,
            &self.bar.configuration,
            dim,
        )
    }

    /// One entry, the one at `index` of the bar, drawn from the format of
    /// its look, or in Zellij's colors when that format is unset.
    ///
    /// The title and the tab's name are filled in after the format is read
    /// and their control characters are escaped, so a title with a `{` or a
    /// `#[` in it is drawn as typed.
    fn draw_entry(&self, entry: &Entry, index: usize, dim: f32) -> String {
        let index = (index + 1).to_string();
        let title = show_controls(&entry.title);
        let tab = show_controls(&entry.tab);
        // Each flag is drawn over the entry's own style, so a plain one takes
        // the entry's style whole, and a styled one changes only what its
        // `#[...]` sets and keeps the entry's background under it.
        let drawn_flags: Vec<Vec<ANSIString<'static>>> = FLAGS
            .iter()
            .zip(&self.flag_formats)
            .zip(entry.flags())
            .filter(|((_, format), shown)| *shown && !format.is_empty())
            .map(|((_, format), _)| {
                format::render_filled(
                    format,
                    &[("tab", &[Fill::Text(&tab)])],
                    &self.bar.configuration,
                    dim,
                )
            })
            .collect();
        let flags: Vec<Fill> = drawn_flags.iter().map(|flag| Fill::Drawn(flag)).collect();
        let look = Look::of(entry);
        let own_format = match look {
            Look::Focused => self.focused_entry_format.as_deref(),
            Look::Minimized => self.minimized_entry_format.as_deref(),
            Look::Normal => self.entry_format.as_deref(),
        };
        let template = match own_format {
            Some(format) => Cow::Borrowed(format),
            None => {
                // A minimized pane is not on screen, so it is drawn in plain
                // text rather than on a ribbon.
                let ribbon = self.bar.zellij_palette.map(|palette| match look {
                    Look::Focused => palette.ribbon_selected,
                    Look::Minimized => palette.text_unselected,
                    Look::Normal => palette.ribbon_unselected,
                });
                Cow::Owned(zellij_look(ribbon, ZELLIJ_ENTRY, look == Look::Focused))
            }
        };
        let parts = format::render_filled(
            &template,
            &[
                ("title", &[Fill::Text(&title)]),
                ("index", &[Fill::Text(&index)]),
                ("tab", &[Fill::Text(&tab)]),
                ("flags", &flags),
            ],
            &self.bar.configuration,
            dim,
        );
        ANSIStrings(&parts).to_string()
    }

    /// The marker for `count` entries hidden on `side`.
    fn draw_marker(&self, side: Side, count: usize, dim: f32) -> String {
        let (own_format, zellij) = match side {
            Side::Left => (&self.overflow_left_format, ZELLIJ_OVERFLOW_LEFT),
            Side::Right => (&self.overflow_right_format, ZELLIJ_OVERFLOW_RIGHT),
        };
        let template = match own_format {
            Some(format) => Cow::Borrowed(format.as_str()),
            None => {
                let ribbon = self
                    .bar
                    .zellij_palette
                    .map(|palette| palette.ribbon_unselected);
                Cow::Owned(zellij_look(ribbon, zellij, false))
            }
        };
        let count = count.to_string();
        let parts = format::render_filled(
            &template,
            &[("count", &[Fill::Text(&count)])],
            &self.bar.configuration,
            dim,
        );
        ANSIStrings(&parts).to_string()
    }

    /// A format string with no placeholders, such as the spacer.
    pub(crate) fn draw_format(&self, format: &str, dim: f32) -> String {
        let parts = format::render_template(format, &[], &self.bar.configuration, dim);
        ANSIStrings(&parts).to_string()
    }
}

/// Which side of the shown entries a marker stands on.
#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}
