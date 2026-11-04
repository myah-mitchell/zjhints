//! The tray: a row of small items at the bar's right end, flush with it, as
//! a desktop taskbar keeps its clock and status icons apart from the
//! windows.
//!
//! The `tray` option names the items, in the order they are drawn from left
//! to right. Each kind is a `TrayItem`, which says how it is drawn and what
//! a click on it does:
//!
//! - `mode`, the input mode, drawn as zjhints and zjtabs draw their mode
//!   prefix, from `zjkit::mode`. A left click opens the settings screen.
//! - `session`, the session's name, drawn as zjtabs draws it. A left click
//!   opens Zellij's session manager.
//! - `clock`, the time of day. See `clock`.
//!
//! A right click on any of them opens Zellij's bar menu, as a click anywhere
//! off an entry does.
//!
//! The tray outranks the entries: they are fitted into the columns the
//! launcher's button and the tray leave, and give way first. When the tray
//! does not fit beside the button either, it drops items from its left end,
//! whole, until it does, so the item listed last is the one kept longest.
//! See `State::draw_bar`.

use crate::draw::Clickable;
use crate::State;
use ansi_term::ANSIStrings;
use std::borrow::Cow;
use std::ops::Range;
use zellij_tile::prelude::ModeInfo;
use zjkit::ansi::{calculate_visible_length, show_controls};
use zjkit::format::{self, Fill};
use zjkit::mode::render_mode_prefix;
use zjkit::theme::zellij_look;

/// What the clock looks like in Zellij's own colors, when its format is
/// unset: the time on a ribbon, as an entry's title is.
const ZELLIJ_CLOCK: &str = " {clock} ";
/// What the session's name looks like in Zellij's own colors, when its
/// format is unset: the name on the selected ribbon, in bold, as zjtabs
/// draws it.
const ZELLIJ_SESSION: &str = " {session} ";

/// One kind of item the tray can show.
///
/// Adding a kind takes a name in `name`, a place in `ALL`, a way to draw it
/// in `State::draw_tray_item` and whether it has anything to show in
/// `State::tray_item_shown`; a click on it is `mouse::State::left_click`'s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrayItem {
    /// The current input mode.
    Mode,
    /// The session's name.
    Session,
    /// The time of day.
    Clock,
}

impl TrayItem {
    /// Every kind, in the order the docs list them.
    pub(crate) const ALL: [TrayItem; 3] = [TrayItem::Mode, TrayItem::Session, TrayItem::Clock];

    /// The name the `tray` option knows the kind by.
    pub(crate) fn name(self) -> &'static str {
        match self {
            TrayItem::Mode => "mode",
            TrayItem::Session => "session",
            TrayItem::Clock => "clock",
        }
    }
}

/// The items `tray` names, in order. Names are split by spaces or commas
/// and matched ignoring case. A name the bar does not know is skipped, as is
/// a second mention of one already listed, so a typo or a name a later
/// version adds costs only that item.
pub(crate) fn tray_items(tray: &str) -> Vec<TrayItem> {
    let mut items = Vec::new();
    for name in tray
        .split(|char: char| char.is_whitespace() || char == ',')
        .filter(|name| !name.is_empty())
    {
        let item = TrayItem::ALL
            .into_iter()
            .find(|item| item.name().eq_ignore_ascii_case(name));
        if let Some(item) = item.filter(|item| !items.contains(item)) {
            items.push(item);
        }
    }
    items
}

/// The tray as drawn, fitted to the columns it was given.
#[derive(Debug, Default)]
pub(crate) struct DrawnTray {
    /// The items that fit, joined by `tray_spacer`.
    pub line: String,
    /// How many columns `line` takes.
    pub width: usize,
    /// The columns of `line` each item was drawn on, counted from its start.
    pub spans: Vec<(Range<usize>, Clickable)>,
}

impl State {
    /// Whether `item` has anything to show: the mode always, the session's
    /// name while it is shown and Zellij has sent it, and the clock once it
    /// knows its offset from UTC.
    pub(crate) fn tray_item_shown(&self, item: TrayItem) -> bool {
        match item {
            TrayItem::Mode => true,
            TrayItem::Session => self.session_name().is_some(),
            TrayItem::Clock => self.clock.text.is_some(),
        }
    }

    /// Whether the tray draws nothing at all: it lists no items, or none of
    /// them has anything to show.
    pub(crate) fn tray_is_empty(&self) -> bool {
        !self.tray.iter().any(|item| self.tray_item_shown(*item))
    }

    /// The session's name, while the tray shows it. See
    /// `session_name_shown_for`.
    pub(crate) fn session_name(&self) -> Option<&str> {
        self.session_name_shown_for(&self.mode_info)
    }

    /// The session's name in `mode_info`, if the tray would show it with
    /// that mode: `session` is in the tray, `show_session` lets it show (see
    /// `zjkit::mode::session_shown`) and the name is not empty.
    pub(crate) fn session_name_shown_for<'mode>(
        &self,
        mode_info: &'mode ModeInfo,
    ) -> Option<&'mode str> {
        if !self.tray.contains(&TrayItem::Session)
            || !zjkit::mode::session_shown(
                self.show_session,
                crate::DEFAULT_SHOW_SESSION,
                mode_info,
            )
        {
            return None;
        }
        mode_info
            .session_name
            .as_deref()
            .filter(|name| !name.is_empty())
    }

    /// The tray's items that fit into `limit` columns, or all of them for
    /// `None`, joined by `tray_spacer`. Items are dropped from the left,
    /// whole, until the rest fit, down to none: an item is never cut.
    pub(crate) fn draw_tray(&self, limit: Option<usize>, dim: f32) -> DrawnTray {
        let width = |text: &str| calculate_visible_length(text, self.wide_ambiguous);
        let spacer = self.draw_format(&self.tray_spacer, dim);
        let mut drawn: Vec<(TrayItem, String)> = self
            .tray
            .iter()
            .filter(|item| self.tray_item_shown(**item))
            .map(|item| (*item, self.draw_tray_item(*item, dim)))
            .filter(|(_, text)| width(text) > 0)
            .collect();
        let total = |drawn: &[(TrayItem, String)]| {
            let items: usize = drawn.iter().map(|(_, text)| width(text)).sum();
            items + width(&spacer) * drawn.len().saturating_sub(1)
        };
        if let Some(limit) = limit {
            while !drawn.is_empty() && total(&drawn) > limit {
                drawn.remove(0);
            }
        }
        let mut tray = DrawnTray::default();
        for (index, (item, text)) in drawn.iter().enumerate() {
            if index > 0 {
                tray.line.push_str(&spacer);
                tray.width += width(&spacer);
            }
            let columns = width(text);
            tray.spans
                .push((tray.width..tray.width + columns, Clickable::Tray(*item)));
            tray.line.push_str(text);
            tray.width += columns;
        }
        tray
    }

    /// One item of the tray, drawn from its format, or in Zellij's colors
    /// while that is unset. Empty when the item has nothing to show.
    fn draw_tray_item(&self, item: TrayItem, dim: f32) -> String {
        match item {
            TrayItem::Mode => self.draw_mode(dim),
            TrayItem::Session => self.draw_session(dim),
            TrayItem::Clock => self.draw_clock(dim),
        }
    }

    /// The current input mode, drawn as `zjkit::mode::render_mode_prefix`
    /// draws it. Dimmed the way the entries are, through the Zellij theme's
    /// colors for the built-in look and through `dim` for a format string.
    fn draw_mode(&self, dim: f32) -> String {
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

    /// The session's name, drawn from `session_format`, or on Zellij's
    /// selected ribbon while it is unset. Nothing while `session_name` has
    /// none.
    ///
    /// The name is filled in after the format is read, with its control
    /// characters escaped, so a name with a `{` or a `#[` in it is drawn as
    /// typed.
    fn draw_session(&self, dim: f32) -> String {
        let Some(name) = self.session_name() else {
            return String::new();
        };
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

    /// The time, drawn from `clock_item_format`, or on Zellij's ribbon while
    /// it is unset. Nothing while the clock has no text.
    ///
    /// The time is filled in after the format is read, as the session's name
    /// is, so text from `clock_format` is drawn as typed.
    fn draw_clock(&self, dim: f32) -> String {
        let Some(text) = self.clock.text.as_deref() else {
            return String::new();
        };
        let text = show_controls(text);
        let template = match self.clock.item_format.as_deref() {
            Some(format) => Cow::Borrowed(format),
            None => {
                let ribbon = self
                    .bar
                    .zellij_palette
                    .map(|palette| palette.ribbon_unselected);
                Cow::Owned(zellij_look(ribbon, ZELLIJ_CLOCK, false))
            }
        };
        let parts = format::render_filled(
            &template,
            &[("clock", &[Fill::Text(&text)])],
            &self.bar.configuration,
            dim,
        );
        ANSIStrings(&parts).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_names_are_split_by_spaces_or_commas_and_matched_ignoring_case() {
        use TrayItem::*;
        assert_eq!(tray_items("mode session clock"), vec![Mode, Session, Clock]);
        assert_eq!(tray_items("clock,mode"), vec![Clock, Mode]);
        assert_eq!(tray_items(" Clock ,  SESSION\t"), vec![Clock, Session]);
        assert_eq!(tray_items(""), vec![]);
        assert_eq!(tray_items(" , "), vec![]);
    }

    #[test]
    fn unknown_names_and_repeats_are_skipped() {
        use TrayItem::*;
        assert_eq!(
            tray_items("battery clock weather clock mode"),
            vec![Clock, Mode]
        );
        assert_eq!(tray_items("clocks"), vec![]);
    }

    #[test]
    fn every_item_has_a_name_of_its_own() {
        for item in TrayItem::ALL {
            assert_eq!(tray_items(item.name()), vec![item]);
        }
    }
}
