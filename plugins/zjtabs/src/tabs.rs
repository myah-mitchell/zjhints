//! Drawing the tabs: each tab from its format, the run of them that fits
//! around the active one, the markers for the rest, and which columns of the
//! line stand for which tab.

use crate::flags::FLAGS;
use crate::State;
use ansi_term::{ANSIString, ANSIStrings};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use zellij_tile::prelude::*;
use zjkit::ansi::{calculate_visible_length, cut_line, show_controls};
use zjkit::fit;
use zjkit::format::{self, Fill};
use zjkit::mouse::ColumnSpans;
use zjkit::nesting;
use zjkit::theme::color_value;

/// What the tab looks like in Zellij's own colors, when no format is set:
/// its index, its name and its flags on a ribbon.
const ZELLIJ_TAB: &str = " {index} {name}{flags} ";
/// What the markers for hidden tabs look like in Zellij's own colors, when no
/// format is set: the arrows and count of Zellij's own tab bar.
const ZELLIJ_OVERFLOW_LEFT: &str = " \u{2190} +{count} ";
const ZELLIJ_OVERFLOW_RIGHT: &str = " +{count} \u{2192} ";

/// One tab, as the bar draws it: what `TabUpdate` says about it, less what
/// the bar never shows, so a change to the rest of it does not redraw.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Tab {
    /// Where the tab stands among the others, counted from 0.
    pub position: usize,
    /// Zellij's id for the tab, which stays the same as tabs move.
    pub tab_id: usize,
    pub name: String,
    pub active: bool,
    /// Which indicators of `{flags}` the tab shows, in the order of
    /// `flags::FLAGS`.
    pub flags: [bool; FLAGS.len()],
}

impl Tab {
    pub(crate) fn of(info: &TabInfo) -> Self {
        Tab {
            position: info.position,
            tab_id: info.tab_id,
            name: info.name.clone(),
            active: info.active,
            flags: [
                info.is_fullscreen_active,
                info.is_sync_panes_active,
                info.are_floating_panes_visible,
                needs_attention(info),
            ],
        }
    }
}

/// Every tab of a `TabUpdate`, in the order they stand.
pub(crate) fn tabs_of(infos: &[TabInfo]) -> Vec<Tab> {
    let mut tabs: Vec<Tab> = infos.iter().map(Tab::of).collect();
    tabs.sort_by_key(|tab| tab.position);
    tabs
}

/// Whether a tab rang the bell, or has a pane whose program is waiting on
/// the user, failed, or finished. Zellij reports a bell only for a tab that
/// is not active, and leaves out a tab the user muted. A program still
/// working is not something the user has to look at, so it does not count.
fn needs_attention(info: &TabInfo) -> bool {
    let status_needs_user = !info.attention.muted
        && matches!(
            info.attention.status,
            AttentionStatus::NeedsYou | AttentionStatus::Failed | AttentionStatus::Done
        );
    info.has_bell_notification || info.is_flashing_bell || status_needs_user
}

/// What a stretch of the drawn line stands for, for the mouse.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Clickable {
    /// The tab at this position.
    Tab(usize),
    /// A marker for hidden tabs, with the position of the hidden tab nearest
    /// to it.
    Marker(usize),
}

/// What of `ModeInfo` the bar shows: whether a tab is being renamed, and the
/// nested-session state that hides or dims it. Compared to tell whether a
/// mode update changes anything on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ModeView {
    renaming: bool,
    nested: bool,
    host_ui_is_covered: bool,
    host_descended: bool,
    ascended_out_of: bool,
}

impl ModeView {
    pub(crate) fn of(mode_info: &ModeInfo) -> Self {
        ModeView {
            renaming: mode_info.mode == InputMode::RenameTab,
            nested: nesting::is_nested(mode_info),
            host_ui_is_covered: nesting::host_ui_is_covered(mode_info),
            host_descended: nesting::is_host_descended(mode_info),
            ascended_out_of: nesting::is_ascended_out_of(mode_info),
        }
    }
}

impl State {
    /// How wide the line may be, or `None` for no limit.
    ///
    /// `max_length` is a hard cap the user set; `auto_width` fits the bar's
    /// own pane once its width is known. With both in play the smaller wins,
    /// so an explicit cap is never exceeded just because the pane is wide.
    pub(crate) fn length_limit(&self) -> Option<usize> {
        let explicit = (self.max_length > 0).then_some(self.max_length);
        let auto = self.auto_width.then_some(self.bar.pane_width).flatten();
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
    /// host's own tab bar is on screen beside it. Once the host has covered
    /// its own bars to give this session the whole display, that is no
    /// longer true, and hiding would leave no tab bar at all.
    fn hidden_by_nesting(&self) -> bool {
        self.hide_when_nested
            && nesting::is_nested(&self.mode_info)
            && !nesting::host_ui_is_covered(&self.mode_info)
    }

    /// Whether the bar draws an empty line: it is hidden, by its option or a
    /// message, or by nesting, or there are no tabs to draw.
    pub(crate) fn draws_nothing(&self) -> bool {
        self.bar.visibility.is_hidden() || self.hidden_by_nesting() || self.tabs.is_empty()
    }

    /// The line to draw, and which of its columns stand for which tab or
    /// marker.
    pub(crate) fn draw_bar(&self) -> (String, ColumnSpans<Clickable>) {
        if self.draws_nothing() {
            return (String::new(), ColumnSpans::default());
        }
        let dim = self.dim_amount();
        let width = |text: &str| calculate_visible_length(text, self.wide_ambiguous);

        let drawn_tabs: Vec<String> = self
            .tabs
            .iter()
            .map(|tab| self.draw_tab(tab, dim))
            .collect();
        let tab_widths: Vec<usize> = drawn_tabs.iter().map(|tab| width(tab)).collect();
        let spacer = self.draw_format(&self.tab_spacer, dim);
        let left = |count: usize| self.draw_marker(Side::Left, count, dim);
        let right = |count: usize| self.draw_marker(Side::Right, count, dim);
        let active = self.tabs.iter().position(|tab| tab.active).unwrap_or(0);
        let limit = self.length_limit();

        let window = fit::window(
            &tab_widths,
            active,
            width(&spacer),
            limit.unwrap_or(usize::MAX),
            once_per_count(|count| width(&left(count))),
            once_per_count(|count| width(&right(count))),
        );

        // The pieces of the line in drawing order, each with its width and
        // what it stands for, and a spacer between every two of them. The
        // markers are left out when `with_markers` is false.
        let lay_out = |with_markers: bool| {
            let mut pieces: Vec<(String, usize, Option<Clickable>)> = vec![];
            let mut push = |text: String, target: Option<Clickable>| {
                if !pieces.is_empty() {
                    pieces.push((spacer.clone(), width(&spacer), None));
                }
                let columns = width(&text);
                pieces.push((text, columns, target));
            };
            if with_markers && window.hidden_before > 0 {
                let nearest = self.tabs[window.range.start - 1].position;
                push(left(window.hidden_before), Some(Clickable::Marker(nearest)));
            }
            for index in window.range.clone() {
                let tab = Clickable::Tab(self.tabs[index].position);
                push(drawn_tabs[index].clone(), Some(tab));
            }
            if with_markers && window.hidden_after > 0 {
                let nearest = self.tabs[window.range.end].position;
                push(right(window.hidden_after), Some(Clickable::Marker(nearest)));
            }
            pieces
        };
        let mut pieces = lay_out(true);
        // The window keeps the active tab even when it and its markers alone
        // are wider than the line may be, and the cut below takes from the
        // right, which could leave the left marker and none of the active
        // tab. So the markers go first, and the active tab is cut on its
        // own, as a lone tab too wide for the bar is.
        let line_width: usize = pieces.iter().map(|(_, columns, _)| columns).sum();
        let hides_tabs = window.hidden_before > 0 || window.hidden_after > 0;
        if hides_tabs && limit.is_some_and(|limit| line_width > limit) {
            pieces = lay_out(false);
        }

        let line: String = pieces.iter().map(|(text, _, _)| text.as_str()).collect();
        let cut = limit.and_then(|limit| {
            cut_line(
                &line,
                &self.overflow_str,
                limit,
                self.wide_ambiguous,
                &self.bar.configuration,
                dim,
            )
        });
        let (line, kept) = match cut {
            Some(cut) => (cut.line, cut.kept),
            None => (line, usize::MAX),
        };
        // What the cut left of each piece stands for it; the overflow string
        // after the cut stands for nothing.
        let mut spans = ColumnSpans::default();
        let mut column = 0;
        for (_, columns, target) in pieces {
            if let Some(target) = target {
                spans.push(column..(column + columns).min(kept), target);
            }
            column += columns;
        }
        (line, spans)
    }

    /// One tab, drawn from its format, or in Zellij's ribbon colors when its
    /// format is unset.
    fn draw_tab(&self, tab: &Tab, dim: f32) -> String {
        let index = if self.index_from_zero {
            tab.position
        } else {
            tab.position + 1
        };
        let index = index.to_string();
        // Zellij clears the name when renaming starts and updates it as the
        // user types, so the tab shows the name as it is being typed.
        let name = if tab.active && tab.name.is_empty() && ModeView::of(&self.mode_info).renaming {
            show_controls(&self.rename_placeholder)
        } else {
            show_controls(&tab.name)
        };
        // Each flag is drawn over the tab's own style, so a plain one takes
        // the tab's style whole, and a styled one changes only what its
        // `#[...]` sets and keeps the tab's background under it.
        let drawn_flags: Vec<Vec<ANSIString<'static>>> = FLAGS
            .iter()
            .zip(&self.flag_formats)
            .zip(tab.flags)
            .filter(|((_, format), shown)| *shown && !format.is_empty())
            .map(|((_, format), _)| self.format_parts(format, dim))
            .collect();
        let flags: Vec<Fill> = drawn_flags.iter().map(|flag| Fill::Drawn(flag)).collect();
        let own_format = if tab.active {
            self.active_tab_format.as_deref()
        } else {
            self.tab_format.as_deref()
        };
        let template = match own_format {
            Some(format) => Cow::Borrowed(format),
            None => {
                let ribbon = self.bar.zellij_palette.map(|palette| {
                    if tab.active {
                        palette.ribbon_selected
                    } else {
                        palette.ribbon_unselected
                    }
                });
                Cow::Owned(zellij_look(ribbon, ZELLIJ_TAB, true))
            }
        };
        let parts = format::render_filled(
            &template,
            &[
                ("index", &[Fill::Text(&index)]),
                ("name", &[Fill::Text(&name)]),
                ("flags", &flags),
            ],
            &self.bar.configuration,
            dim,
        );
        ANSIStrings(&parts).to_string()
    }

    /// The marker for `count` tabs hidden on `side`.
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
    fn draw_format(&self, format: &str, dim: f32) -> String {
        ANSIStrings(&self.format_parts(format, dim)).to_string()
    }

    /// The styled segments of a format string with no placeholders, such as
    /// a flag.
    fn format_parts(&self, format: &str, dim: f32) -> Vec<ANSIString<'static>> {
        format::render_template(format, &[], &self.bar.configuration, dim)
    }
}

/// `measure`, remembering its answer for each count so that it runs once per
/// count. `fit::window` asks for a marker's width at every step it tries, and
/// mostly for counts it has asked about before, while each answer means
/// drawing the marker.
fn once_per_count(measure: impl Fn(usize) -> usize) -> impl Fn(usize) -> usize {
    let known = RefCell::new(HashMap::new());
    move |count| {
        *known
            .borrow_mut()
            .entry(count)
            .or_insert_with(|| measure(count))
    }
}

/// Which side of the shown tabs a marker stands on.
#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

/// `text` as a format string drawn in the colors of a Zellij ribbon, or
/// unstyled before Zellij has sent its theme.
fn zellij_look(ribbon: Option<StyleDeclaration>, text: &str, bold: bool) -> String {
    match ribbon {
        Some(ribbon) => format!(
            "#[fg={},bg={}{}]{text}",
            color_value(ribbon.base),
            color_value(ribbon.background),
            if bold { ",bold" } else { "" },
        ),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::once_per_count;
    use std::cell::Cell;

    #[test]
    fn a_width_is_measured_once_for_each_count() {
        let measured = Cell::new(0);
        let width = once_per_count(|count| {
            measured.set(measured.get() + 1);
            count * 2
        });
        assert_eq!([width(3), width(3), width(10), width(3)], [6, 6, 20, 6]);
        assert_eq!(measured.get(), 2);
    }
}
