//! Drawing the tabs: the mode prefix before them when `show_mode` is on,
//! the session name after them when it is shown (see `session_shown`), each
//! tab from its format, the run of them that fits around the active one, the
//! markers for the rest, and which columns of the line stand for which tab.

use crate::flags::FLAGS;
use crate::State;
use ansi_term::{ANSIString, ANSIStrings};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use zellij_tile::prelude::*;
use zjkit::ansi::{calculate_visible_length, cut_line, show_controls, CutLine};
use zjkit::fit;
use zjkit::format::{self, Fill};
use zjkit::mode::render_mode_prefix;
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
/// What the session name looks like in Zellij's own colors, when no format is
/// set: a block in the selected ribbon's colors, the same as the mode prefix
/// that opens the line.
const ZELLIJ_SESSION: &str = " {session} ";

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
    /// The tab at `position`, whose id was `tab_id` when the line was drawn.
    /// The id is kept here, not looked up at click time, so a click acts on
    /// the tab it landed on even if a `TabUpdate` has moved the tabs since.
    Tab { position: usize, tab_id: usize },
    /// A marker for hidden tabs, with the position of the hidden tab nearest
    /// to it.
    Marker(usize),
    /// The mode prefix, which opens the settings screen when clicked, as the
    /// zjhints mode prefix does.
    ModePrefix,
}

/// What of `ModeInfo` the bar shows: whether a tab is being renamed, the
/// nested-session state that hides or dims it, the mode itself while the
/// mode prefix is shown, and the session's name while that is shown.
/// Compared to tell whether a mode update changes anything on screen.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ModeView {
    /// The input mode, or `None` while `show_mode` is off, so that a change
    /// between two modes that look the same to the tabs draws nothing new.
    mode: Option<InputMode>,
    /// The session's name, or `None` while it is not shown, so that renaming
    /// a session whose name is not shown draws nothing new. See
    /// `session_shown`.
    session_name: Option<String>,
    renaming: bool,
    nested: bool,
    host_ui_is_covered: bool,
    host_descended: bool,
    ascended_out_of: bool,
}

impl ModeView {
    /// What the bar shows of `mode_info`, with the mode itself only when
    /// `show_mode` is on and the session's name only when `session_shown`
    /// says so for `show_session`, the option as the user set it.
    pub(crate) fn of(mode_info: &ModeInfo, show_mode: bool, show_session: Option<bool>) -> Self {
        let session_shown = session_shown(show_session, mode_info);
        ModeView {
            mode: show_mode.then_some(mode_info.mode),
            session_name: mode_info.session_name.clone().filter(|_| session_shown),
            renaming: mode_info.mode == InputMode::RenameTab,
            nested: nesting::is_nested(mode_info),
            host_ui_is_covered: nesting::host_ui_is_covered(mode_info),
            host_descended: nesting::is_host_descended(mode_info),
            ascended_out_of: nesting::is_ascended_out_of(mode_info),
        }
    }
}

/// Whether the bar draws the session's name in `mode_info`, with
/// `show_session` as the user set it, or `None` while it is unset.
///
/// A `show_session` the user set decides alone, so `true` shows the name on
/// this bar even when Zellij hides it from its own. Left unset, the name
/// follows Zellij's `pane_frames { hide_session_name }`, as Zellij's own tab
/// bar does, so turning that on hides the name from both: see
/// `zellij_hides_session_name`. Otherwise it is shown, by
/// `DEFAULT_SHOW_SESSION`.
pub(crate) fn session_shown(show_session: Option<bool>, mode_info: &ModeInfo) -> bool {
    show_session
        .unwrap_or_else(|| crate::DEFAULT_SHOW_SESSION && !zellij_hides_session_name(mode_info))
}

/// Whether Zellij's own tab bar leaves the session's name off in
/// `mode_info`: while its `hide_session_name` is on, except while the
/// session is being renamed, when the bar shows the name as it is typed.
/// The same test as `session_name_hidden` in Zellij's `bars` plugin.
fn zellij_hides_session_name(mode_info: &ModeInfo) -> bool {
    mode_info.style.hide_session_name && mode_info.mode != InputMode::RenameSession
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

    /// The line to draw, and which of its columns stand for which tab,
    /// marker or the mode prefix.
    ///
    /// The mode prefix, when shown, opens the line, with a `tab_spacer` after
    /// it, and the session name, when shown, closes it, with a `tab_spacer`
    /// before it, against the end of the line. The tabs are fitted into what
    /// the two leave. The columns between the last tab or marker and the
    /// spacer before the session name are left blank, in the bar's own
    /// background, as the rest of the line is when there is no session name
    /// at its end. Without a limit on the line's width, as with `auto_width`
    /// and `max_length` both off, there is no end to set the session name
    /// against, so it follows the tabs straight after its spacer.
    ///
    /// The prefix's columns stand for `Clickable::ModePrefix`; the spacers,
    /// the blank and the session name stand for nothing. A prefix that alone
    /// fills the line leaves no room for tabs, and one wider than the line is
    /// cut to fit, as a lone tab too wide for the bar is, with only what is
    /// left of it standing for the prefix and the `overflow_str` after it
    /// standing for nothing.
    ///
    /// The session name is the first to go when the line runs short: see
    /// `session_fits`.
    pub(crate) fn draw_bar(&self) -> (String, ColumnSpans<Clickable>) {
        if self.draws_nothing() {
            return (String::new(), ColumnSpans::default());
        }
        let dim = self.dim_amount();
        let limit = self.length_limit();
        let width = |text: &str| calculate_visible_length(text, self.wide_ambiguous);
        let spacer = self.draw_format(&self.tab_spacer, dim);
        let prefix = self.draw_mode_prefix(dim);
        let prefix_width = width(&prefix);
        // The prefix and the spacer after it, which the tabs begin after.
        let opening = if prefix_width == 0 {
            String::new()
        } else {
            format!("{prefix}{spacer}")
        };
        let start = width(&opening);
        // The spacer and the session name after it, or nothing when the name
        // is not shown or does not fit.
        let session = self.draw_session(dim);
        let ending = format!("{spacer}{session}");
        let ending = if width(&session) > 0 && self.session_fits(limit, start, width(&ending), dim)
        {
            ending
        } else {
            String::new()
        };
        let ending_width = width(&ending);

        // The columns left for the tabs, `None` for no limit and `Some(0)`
        // for none at all.
        let room = limit.map(|limit| limit.saturating_sub(start + ending_width));
        if prefix_width > 0 && room == Some(0) {
            let (line, kept) = match limit.and_then(|limit| self.cut(&prefix, limit, dim)) {
                Some(cut) => (cut.line, cut.kept),
                None => (prefix, prefix_width),
            };
            let mut spans = ColumnSpans::default();
            spans.push(0..kept, Clickable::ModePrefix);
            return (line, spans);
        }
        let (tabs, mut spans) = self.draw_tabs(room, start, dim);
        if prefix_width > 0 {
            spans.push(0..prefix_width, Clickable::ModePrefix);
        }
        let blank = match limit {
            Some(limit) if ending_width > 0 => {
                limit.saturating_sub(start + width(&tabs) + ending_width)
            }
            _ => 0,
        };
        let blank = " ".repeat(blank);
        (format!("{opening}{tabs}{blank}{ending}"), spans)
    }

    /// Whether the session name fits on a line `limit` columns wide, or of
    /// any width for `None`, taking `ending` columns with the spacer before
    /// it, when the tabs begin at `start`, after the mode prefix.
    ///
    /// It fits only when the active tab, drawn whole, fits beside it too,
    /// with a marker on each side that has tabs hidden behind it. The prefix,
    /// the active tab and the markers outrank the session name, so the name
    /// goes before the active tab would have to be cut for it, or a marker
    /// left off and the hidden tabs with it. The other tabs do not count:
    /// they give way to the name, and the window around the active tab
    /// shrinks to what is left.
    fn session_fits(&self, limit: Option<usize>, start: usize, ending: usize, dim: f32) -> bool {
        let Some(limit) = limit else {
            return true;
        };
        let width = |text: &str| calculate_visible_length(text, self.wide_ambiguous);
        // The tab `draw_tabs` keeps on screen: the active one, or the first
        // while none is.
        let Some(active) = self
            .tabs
            .iter()
            .position(|tab| tab.active)
            .or((!self.tabs.is_empty()).then_some(0))
        else {
            return start + ending <= limit;
        };
        let spacer = width(&self.draw_format(&self.tab_spacer, dim));
        let mut tabs = width(&self.draw_tab(&self.tabs[active], dim));
        // The markers for every tab hidden on each side, each with the spacer
        // between it and the active tab, as `draw_tabs` lays them out.
        let hidden_after = self.tabs.len() - 1 - active;
        if active > 0 {
            tabs += width(&self.draw_marker(Side::Left, active, dim)) + spacer;
        }
        if hidden_after > 0 {
            tabs += width(&self.draw_marker(Side::Right, hidden_after, dim)) + spacer;
        }
        start + tabs + ending <= limit
    }

    /// The session's name, drawn from `session_format`, or as a block in the
    /// colors of Zellij's selected ribbon while that is unset. Nothing while
    /// it is not shown (see `session_shown`), or while Zellij has not sent
    /// the session's name.
    ///
    /// The name is filled in after the format is read and its control
    /// characters are escaped, as a tab's name is, so a session named with a
    /// `{` or a `#[` in it is drawn as typed. Dimmed the way the tabs are.
    fn draw_session(&self, dim: f32) -> String {
        if !session_shown(self.show_session, &self.mode_info) {
            return String::new();
        }
        let Some(name) = self.mode_info.session_name.as_deref() else {
            return String::new();
        };
        if name.is_empty() {
            return String::new();
        }
        let name = show_controls(name);
        let template = match self.session_format.as_deref() {
            Some(format) => Cow::Borrowed(format),
            None => {
                let ribbon = self
                    .bar
                    .zellij_palette
                    .map(|palette| palette.ribbon_selected);
                Cow::Owned(zellij_look(ribbon, ZELLIJ_SESSION, true))
            }
        };
        let parts = format::render_filled(
            &template,
            &[("session", &[Fill::Text(&name)])],
            &self.bar.configuration,
            dim,
        );
        ANSIStrings(&parts).to_string()
    }

    /// The current input mode, drawn as `zjkit::mode::render_mode_prefix`
    /// draws it, or nothing while `show_mode` is off. Dimmed the way the tabs
    /// are, through the Zellij theme's colors for the built-in look and
    /// through `dim` for a format string.
    fn draw_mode_prefix(&self, dim: f32) -> String {
        if !self.show_mode {
            return String::new();
        }
        let colors = &self.mode_info.style.colors;
        let dimmed_colors;
        let colors = if dim > 0.0 {
            dimmed_colors = zjkit::dim::styling(colors, dim);
            &dimmed_colors
        } else {
            colors
        };
        let parts = render_mode_prefix(
            self.mode_info.mode,
            colors,
            self.mode_format.as_deref(),
            &self.bar.configuration,
            dim,
        );
        ANSIStrings(&parts).to_string()
    }

    /// The tabs and markers, fitted into `limit` columns, or any width for
    /// `None`, and which of their columns stand for which tab or marker,
    /// counting the line's columns from `start`, where the tabs begin.
    fn draw_tabs(
        &self,
        limit: Option<usize>,
        start: usize,
        dim: f32,
    ) -> (String, ColumnSpans<Clickable>) {
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
                let tab = Clickable::Tab {
                    position: self.tabs[index].position,
                    tab_id: self.tabs[index].tab_id,
                };
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
        let cut = limit.and_then(|limit| self.cut(&line, limit, dim));
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
                let end = (column + columns).min(kept);
                spans.push(start + column..start + end, target);
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
            self.wide_ambiguous,
            &self.bar.configuration,
            dim,
        )
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
        let renaming = self.mode_info.mode == InputMode::RenameTab;
        let name = if tab.active && tab.name.is_empty() && renaming {
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
