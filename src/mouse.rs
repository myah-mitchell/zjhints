//! Mouse support: which columns of the bar stand for what, and what a click on
//! them should do.
//!
//! Everything here is plain data and pure functions, so the whole path from a
//! click to its outcome can be tested without a running Zellij. Carrying an
//! outcome out (opening the context menu, running actions, showing a menu) is
//! left to `State::perform_mouse_outcome` and `State::run_mouse_action`, the
//! only parts that talk to the host. The hover preview, which lists what a
//! menu would offer before it is clicked, is split the same way: what to show
//! and when to close it is decided here, and `State::perform_preview_step`
//! opens and closes the popup.

use std::collections::{BTreeMap, VecDeque};
use std::ops::Range;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::{
    ChoiceItem, Mouse, PromptPlacement, PromptRequest, PromptResult, PromptValue,
};

/// One hint, as something that can be clicked.
#[derive(Clone, Debug, PartialEq)]
pub struct HintTarget {
    /// How the hint is named in a menu of hints that did not fit: its keys,
    /// then its label, as the bar would have shown them.
    pub label: String,
    /// What the hint's keys do, one entry per distinct list of actions, in the
    /// order the keys are shown. A hint whose keys all do the same thing has
    /// exactly one.
    pub choices: Vec<HintChoice>,
}

/// The keys of a hint that share one list of actions.
#[derive(Clone, Debug, PartialEq)]
pub struct HintChoice {
    /// The keys, then a description of what they do, for the hint's picker.
    pub label: String,
    /// What pressing one of the keys runs, in order.
    pub actions: Vec<Action>,
}

impl HintTarget {
    /// What clicking the hint does: run its actions when every key does the
    /// same thing, otherwise ask which of them to run.
    pub fn activate(&self) -> MouseOutcome {
        match self.choices.as_slice() {
            [] => MouseOutcome::Nothing,
            [only] => MouseOutcome::Run(only.actions.clone()),
            choices => MouseOutcome::Menu(
                choices
                    .iter()
                    .map(|choice| MenuEntry {
                        label: choice.label.clone(),
                        pick: MenuPick::Run(choice.actions.clone()),
                    })
                    .collect(),
            ),
        }
    }
}

/// One piece of a laid out hint line, in drawing order, with how many columns
/// it takes up.
#[derive(Clone, Debug, PartialEq)]
pub struct LineSegment {
    pub width: usize,
    pub content: SegmentContent,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SegmentContent {
    Hint(HintTarget),
    /// `hint_spacer`, which does nothing when clicked.
    Spacer,
    /// The `drop_indicator`, standing in for the hints dropped to fit.
    Dropped(Vec<HintTarget>),
}

/// Where a line too long for its limit was cut, in columns from the start of
/// the line.
#[derive(Clone, Debug, PartialEq)]
pub struct Cut {
    /// The first column lost to the cut. Everything from here on is gone,
    /// replaced by `overflow_str`.
    pub at: usize,
    /// The columns `overflow_str` occupies.
    pub overflow: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClickTarget {
    Hint(HintTarget),
    /// A marker standing in for hints that are not on the bar: the
    /// `drop_indicator`, or `overflow_str`.
    HiddenHints(Vec<HintTarget>),
    /// The mode prefix, which opens the settings screen.
    Settings,
}

#[derive(Clone, Debug, PartialEq)]
struct ClickRegion {
    columns: Range<usize>,
    target: ClickTarget,
}

/// What each clickable stretch of the bar's one line stands for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClickMap {
    regions: Vec<ClickRegion>,
}

impl ClickMap {
    /// Place `segments` on the line starting at column `start`, then account
    /// for `cut`, if the line was cut short.
    ///
    /// A hint the cut runs through stays clickable on the columns still
    /// showing. It also joins the hints wholly past the cut, and any dropped
    /// ones the cut took, in the menu `overflow_str` opens, since it can no
    /// longer be read in full.
    pub fn lay_out(segments: Vec<LineSegment>, start: usize, cut: Option<Cut>) -> Self {
        let mut map = ClickMap::default();
        let mut hidden: Vec<HintTarget> = vec![];
        let mut column = start;
        for segment in segments {
            let columns = column..column + segment.width;
            column = columns.end;
            let target = match segment.content {
                SegmentContent::Spacer => continue,
                SegmentContent::Hint(hint) => {
                    if cut.as_ref().is_some_and(|cut| columns.end > cut.at) {
                        hidden.push(hint.clone());
                    }
                    ClickTarget::Hint(hint)
                }
                SegmentContent::Dropped(hints) => {
                    if cut.as_ref().is_some_and(|cut| columns.end > cut.at) {
                        hidden.extend(hints.iter().cloned());
                    }
                    ClickTarget::HiddenHints(hints)
                }
            };
            let end = cut
                .as_ref()
                .map_or(columns.end, |cut| columns.end.min(cut.at));
            map.push(columns.start..end, target);
        }
        if let Some(cut) = cut {
            if !hidden.is_empty() {
                map.push(cut.overflow, ClickTarget::HiddenHints(hidden));
            }
        }
        map
    }

    /// Make `columns`, the mode prefix, open the settings screen.
    pub fn with_settings(mut self, columns: Range<usize>) -> Self {
        self.push(columns, ClickTarget::Settings);
        self
    }

    fn push(&mut self, columns: Range<usize>, target: ClickTarget) {
        if !columns.is_empty() {
            self.regions.push(ClickRegion { columns, target });
        }
    }

    fn region_at(&self, column: usize) -> Option<&ClickRegion> {
        self.regions
            .iter()
            .find(|region| region.columns.contains(&column))
    }

    fn target_at(&self, column: usize) -> Option<&ClickTarget> {
        self.region_at(column).map(|region| &region.target)
    }
}

/// What a click, or an answer to a menu, should lead to.
#[derive(Clone, Debug, PartialEq)]
pub enum MouseOutcome {
    Nothing,
    /// Zellij's own context menu for the bar, opened at the click.
    ContextMenu {
        line: usize,
        column: usize,
    },
    /// Actions to run, in order, as if their key had been pressed.
    Run(Vec<Action>),
    /// A menu to show at the mouse.
    Menu(Vec<MenuEntry>),
    /// Open the settings screen.
    OpenSettings,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuEntry {
    pub label: String,
    pub pick: MenuPick,
}

/// What choosing a menu entry does.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuPick {
    Run(Vec<Action>),
    /// The same as clicking this hint, had it been on the bar.
    Hint(HintTarget),
}

/// What a click on the bar should do, given what the bar last drew.
///
/// Right-click opens Zellij's context menu anywhere on the bar, the way it
/// does on the built-in status bar. Left-click acts on whatever is under it on
/// the hint line, which is the pane's first line; anything else on the pane is
/// empty space.
pub fn click_outcome(map: &ClickMap, mouse: &Mouse) -> MouseOutcome {
    match mouse {
        Mouse::RightClick(line, column) => MouseOutcome::ContextMenu {
            line: (*line).max(0) as usize,
            column: *column,
        },
        Mouse::LeftClick(0, column) => map
            .target_at(*column)
            .map_or(MouseOutcome::Nothing, click_target_outcome),
        _ => MouseOutcome::Nothing,
    }
}

/// What a left-click on `target` does.
fn click_target_outcome(target: &ClickTarget) -> MouseOutcome {
    match target {
        ClickTarget::Hint(hint) => hint.activate(),
        ClickTarget::HiddenHints(hints) => hidden_hints_menu(hints),
        ClickTarget::Settings => MouseOutcome::OpenSettings,
    }
}

/// A menu of hints that are not on the bar, one entry per hint.
fn hidden_hints_menu(hints: &[HintTarget]) -> MouseOutcome {
    MouseOutcome::Menu(
        hints
            .iter()
            .map(|hint| MenuEntry {
                label: hint.label.clone(),
                pick: MenuPick::Hint(hint.clone()),
            })
            .collect(),
    )
}

/// What the answer to a menu should lead to. Anything but a choice of one of
/// its entries does nothing, as does a menu this plugin has no record of.
pub fn menu_answer_outcome(entries: Option<Vec<MenuEntry>>, result: &PromptResult) -> MouseOutcome {
    let PromptResult::Answered(PromptValue::Choice(value)) = result else {
        return MouseOutcome::Nothing;
    };
    let entry = entries.and_then(|entries| {
        let index = value.parse::<usize>().ok()?;
        entries.into_iter().nth(index)
    });
    match entry.map(|entry| entry.pick) {
        Some(MenuPick::Run(actions)) => MouseOutcome::Run(actions),
        Some(MenuPick::Hint(hint)) => hint.activate(),
        None => MouseOutcome::Nothing,
    }
}

/// Actions waiting to run, handed to Zellij one at a time.
///
/// Zellij runs each `run_action` on a thread of its own, so a binding's actions
/// sent back to back could land in any order. Holding each one back until the
/// last reports `ActionComplete` keeps them in the order the binding lists,
/// as pressing its key would.
#[derive(Debug, Default)]
pub struct ActionQueue {
    waiting: VecDeque<Action>,
    in_flight: bool,
}

impl ActionQueue {
    /// Queue `actions` behind anything already running, returning the action
    /// to start now, if nothing was running.
    pub fn enqueue(&mut self, actions: Vec<Action>) -> Option<Action> {
        self.waiting.extend(actions);
        if self.in_flight {
            None
        } else {
            self.start_next()
        }
    }

    /// The running action finished. Returns the next one to start, if any.
    pub fn completed(&mut self) -> Option<Action> {
        self.in_flight = false;
        self.start_next()
    }

    fn start_next(&mut self) -> Option<Action> {
        let action = self.waiting.pop_front()?;
        self.in_flight = true;
        Some(action)
    }
}

/// The prompt that shows `entries` as a menu at the mouse. Each entry's value
/// is its index, which is how `menu_answer_outcome` finds it again.
pub fn menu_request(entries: &[MenuEntry]) -> PromptRequest {
    PromptRequest::menu(
        entries
            .iter()
            .enumerate()
            .map(|(index, entry)| ChoiceItem::labeled(index.to_string(), entry.label.clone())),
    )
    .placement(PromptPlacement::Mouse)
}

/// What hovering a stretch of the bar shows: the entries of the menu a click
/// there would open.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    /// The columns of the hint or marker under the pointer. The popup hangs
    /// from the first. Hovering the same columns while they show the same
    /// entries is hovering the same thing.
    pub columns: Range<usize>,
    /// Each entry as the menu words it.
    pub entries: Vec<String>,
}

/// The preview a hover over the bar calls for, if any. Only a hint or marker
/// whose click opens a menu has one, and it lists exactly what that menu
/// would.
pub fn hover_preview(map: &ClickMap, mouse: &Mouse) -> Option<Preview> {
    let Mouse::Hover(0, column) = mouse else {
        return None;
    };
    let region = map.region_at(*column)?;
    match click_target_outcome(&region.target) {
        MouseOutcome::Menu(entries) if !entries.is_empty() => Some(Preview {
            columns: region.columns.clone(),
            entries: entries.into_iter().map(|entry| entry.label).collect(),
        }),
        _ => None,
    }
}

/// How long a preview waits to be hovered before it closes itself, which is
/// what keeps one from outliving a pointer that left the bar unnoticed.
pub const PREVIEW_IDLE_SECONDS: f64 = 3.0;

/// How long the bar remembers a preview it opened. A little longer than
/// `PREVIEW_IDLE_SECONDS`, so a preview that closed itself can be shown again.
pub const PREVIEW_MEMORY_SECONDS: f64 = 4.0;

/// The pipe message that closes previews. Its payload is the tag of the one to
/// close, and none closes every preview.
pub const PREVIEW_CLOSE_MESSAGE: &str = "zjhints::preview_close";

/// Which preview the bar has open, so that hovering the same hint again leaves
/// it be and hovering something else replaces it.
#[derive(Debug, Default)]
pub struct PreviewMemory {
    shown: Option<ShownPreview>,
    /// Previews opened so far, which numbers each one.
    opened: u64,
    /// Timers asked for and timers that have fired. Zellij runs them to
    /// completion in the order they were set, which is how a firing is
    /// matched to the preview it was set for.
    timers_set: u64,
    timers_fired: u64,
}

#[derive(Debug)]
struct ShownPreview {
    generation: u64,
    preview: Preview,
    timer: u64,
}

/// What a hover asks of the popup.
#[derive(Debug, PartialEq)]
pub enum PreviewStep {
    Keep,
    /// Close preview number `generation`.
    Close(u64),
    /// Open `preview` as number `generation`, closing the one shown, if any.
    Show {
        close: Option<u64>,
        generation: u64,
        preview: Preview,
    },
}

impl PreviewMemory {
    /// Take in a hover over the bar. The caller sets a timer of
    /// `PREVIEW_MEMORY_SECONDS` for every `Show`.
    pub fn hover(&mut self, map: &ClickMap, mouse: &Mouse) -> PreviewStep {
        let Some(preview) = hover_preview(map, mouse) else {
            return self.dismiss().map_or(PreviewStep::Keep, PreviewStep::Close);
        };
        if self
            .shown
            .as_ref()
            .is_some_and(|shown| shown.preview == preview)
        {
            return PreviewStep::Keep;
        }
        let close = self.dismiss();
        self.opened += 1;
        self.timers_set += 1;
        self.shown = Some(ShownPreview {
            generation: self.opened,
            preview: preview.clone(),
            timer: self.timers_set,
        });
        PreviewStep::Show {
            close,
            generation: self.opened,
            preview,
        }
    }

    /// Forget the preview shown, returning its number if there was one.
    pub fn dismiss(&mut self) -> Option<u64> {
        self.shown.take().map(|shown| shown.generation)
    }

    /// A timer fired. Returns the number of the preview it was set for if
    /// that one is still remembered, which is then forgotten.
    pub fn timer_fired(&mut self) -> Option<u64> {
        self.timers_fired += 1;
        if self.shown.as_ref()?.timer == self.timers_fired {
            self.dismiss()
        } else {
            None
        }
    }
}

/// Names preview number `generation` of the bar `owner`, so that closing it
/// leaves other bars' previews alone.
pub fn preview_tag(owner: u32, generation: u64) -> String {
    format!("{owner}-{generation}")
}

// What marks an instance as a preview popup rather than the bar, and what it
// shows. Written by `preview_configuration`, read by
// `PreviewPopup::from_configuration`.
const CONFIG_HOVER_PREVIEW: &str = "hover_preview";
const CONFIG_HOVER_PREVIEW_TAG: &str = "hover_preview_tag";
const CONFIG_HOVER_PREVIEW_ENTRIES: &str = "hover_preview_entries";

/// The configuration a preview popup is opened with.
pub fn preview_configuration(
    tag: &str,
    entries: &[String],
    wide_ambiguous: bool,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        (CONFIG_HOVER_PREVIEW.to_string(), "true".to_string()),
        (CONFIG_HOVER_PREVIEW_TAG.to_string(), tag.to_string()),
        (
            CONFIG_HOVER_PREVIEW_ENTRIES.to_string(),
            encode_entries(entries),
        ),
        (
            crate::CONFIG_AMBIGUOUS_WIDTH.to_string(),
            if wide_ambiguous { "2" } else { "1" }.to_string(),
        ),
    ])
}

/// Entries as one configuration value: each followed by a newline, with
/// backslashes and newlines inside an entry escaped.
fn encode_entries(entries: &[String]) -> String {
    entries
        .iter()
        .map(|entry| {
            let escaped = entry.replace('\\', "\\\\").replace('\n', "\\n");
            format!("{escaped}\n")
        })
        .collect()
}

fn decode_entries(encoded: &str) -> Vec<String> {
    encoded
        .split_terminator('\n')
        .map(|escaped| {
            let mut entry = String::new();
            let mut chars = escaped.chars();
            while let Some(ch) = chars.next() {
                if ch != '\\' {
                    entry.push(ch);
                    continue;
                }
                match chars.next() {
                    Some('n') => entry.push('\n'),
                    Some(other) => entry.push(other),
                    None => entry.push('\\'),
                }
            }
            entry
        })
        .collect()
}

/// The columns and rows a preview of `entries` needs: the entries inside a
/// frame with a space of padding on either side.
pub fn preview_size(entries: &[String], wide_ambiguous: bool) -> (usize, usize) {
    let widest = entries
        .iter()
        .map(|entry| crate::calculate_visible_length(entry, wide_ambiguous))
        .max()
        .unwrap_or(0);
    (widest + FRAME_COLUMNS, entries.len() + FRAME_ROWS)
}

/// What the frame around a preview's entries takes: a border and a space on
/// each side, a border above and below.
const FRAME_COLUMNS: usize = 4;
const FRAME_ROWS: usize = 2;

/// What a preview popup does about an event.
#[derive(Debug, PartialEq)]
pub enum PreviewAction {
    Stay,
    Close,
}

/// The popup side of the hover preview: this plugin running as a read-only
/// list of entries, in a popup the bar opened.
#[derive(Debug)]
pub struct PreviewPopup {
    tag: String,
    entries: Vec<String>,
    wide_ambiguous: bool,
    /// Whether the pointer has been over the popup.
    hovered: bool,
}

impl PreviewPopup {
    /// The popup `configuration` asks for, if it is a preview's.
    pub fn from_configuration(configuration: &BTreeMap<String, String>) -> Option<Self> {
        if configuration.get(CONFIG_HOVER_PREVIEW).map(String::as_str) != Some("true") {
            return None;
        }
        Some(Self {
            tag: configuration
                .get(CONFIG_HOVER_PREVIEW_TAG)
                .cloned()
                .unwrap_or_default(),
            entries: configuration
                .get(CONFIG_HOVER_PREVIEW_ENTRIES)
                .map(|encoded| decode_entries(encoded))
                .unwrap_or_default(),
            wide_ambiguous: configuration
                .get(crate::CONFIG_AMBIGUOUS_WIDTH)
                .is_some_and(|width| width.trim() == "2"),
            hovered: false,
        })
    }

    /// The pointer leaving the popup closes it, and so does a click, which
    /// has nothing to choose here.
    pub fn on_mouse(&mut self, mouse: &Mouse) -> PreviewAction {
        match mouse {
            Mouse::Hover(line, _) if *line < 0 => PreviewAction::Close,
            Mouse::Hover(..) => {
                self.hovered = true;
                PreviewAction::Stay
            }
            Mouse::LeftClick(..) | Mouse::RightClick(..) => PreviewAction::Close,
            _ => PreviewAction::Stay,
        }
    }

    /// A popup nobody hovered by the time `PREVIEW_IDLE_SECONDS` are up has
    /// been left behind. One the pointer is over closes when it leaves.
    pub fn on_timer(&self) -> PreviewAction {
        if self.hovered {
            PreviewAction::Stay
        } else {
            PreviewAction::Close
        }
    }

    /// A `PREVIEW_CLOSE_MESSAGE` with `payload` closes this popup if it names
    /// its tag, or names none.
    pub fn on_close_message(&self, payload: Option<&str>) -> PreviewAction {
        match payload {
            Some(tag) if !tag.is_empty() && tag != self.tag => PreviewAction::Stay,
            _ => PreviewAction::Close,
        }
    }

    /// The popup's rows for a pane of `rows` by `cols`: the entries in a
    /// frame, or as many as fit with `…` standing for the rest.
    pub fn lines(&self, rows: usize, cols: usize) -> Vec<String> {
        if rows <= FRAME_ROWS || cols <= FRAME_COLUMNS {
            return vec![];
        }
        let inner = cols - FRAME_COLUMNS;
        let room = rows - FRAME_ROWS;
        let cut = self.entries.len() > room;
        let shown = if cut { room - 1 } else { self.entries.len() };
        let border = "─".repeat(cols - 2);
        let mut lines = vec![format!("┌{border}┐")];
        for entry in self.entries.iter().take(shown) {
            lines.push(self.framed(entry, inner));
        }
        if cut {
            lines.push(self.framed("…", inner));
        }
        lines.push(format!("└{border}┘"));
        lines
    }

    fn framed(&self, entry: &str, inner: usize) -> String {
        let text = crate::truncate_ansi_string(entry, "…", inner, self.wide_ambiguous);
        let padding =
            inner.saturating_sub(crate::calculate_visible_length(&text, self.wide_ambiguous));
        format!("│ {text}{} │", " ".repeat(padding))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hint(name: &str, actions: Vec<Action>) -> HintTarget {
        HintTarget {
            label: name.to_string(),
            choices: vec![HintChoice {
                label: name.to_string(),
                actions,
            }],
        }
    }

    fn segment(width: usize, content: SegmentContent) -> LineSegment {
        LineSegment { width, content }
    }

    fn left(column: usize) -> Mouse {
        Mouse::LeftClick(0, column)
    }

    /// `a` (3 columns), a spacer, `b` (4 columns), starting at column 1.
    fn two_hints() -> Vec<LineSegment> {
        vec![
            segment(3, SegmentContent::Hint(hint("a", vec![Action::CloseFocus]))),
            segment(1, SegmentContent::Spacer),
            segment(4, SegmentContent::Hint(hint("b", vec![Action::Detach]))),
        ]
    }

    #[test]
    fn each_hint_answers_on_exactly_its_own_columns() {
        let map = ClickMap::lay_out(two_hints(), 1, None);
        let run = |actions| MouseOutcome::Run(actions);
        assert_eq!(click_outcome(&map, &left(0)), MouseOutcome::Nothing);
        assert_eq!(click_outcome(&map, &left(1)), run(vec![Action::CloseFocus]));
        assert_eq!(click_outcome(&map, &left(3)), run(vec![Action::CloseFocus]));
        assert_eq!(click_outcome(&map, &left(4)), MouseOutcome::Nothing);
        assert_eq!(click_outcome(&map, &left(5)), run(vec![Action::Detach]));
        assert_eq!(click_outcome(&map, &left(8)), run(vec![Action::Detach]));
        assert_eq!(click_outcome(&map, &left(9)), MouseOutcome::Nothing);
    }

    #[test]
    fn left_clicks_below_the_hint_line_do_nothing() {
        let map = ClickMap::lay_out(two_hints(), 1, None);
        assert_eq!(
            click_outcome(&map, &Mouse::LeftClick(1, 1)),
            MouseOutcome::Nothing
        );
    }

    #[test]
    fn right_click_opens_the_context_menu_anywhere() {
        let map = ClickMap::default();
        assert_eq!(
            click_outcome(&map, &Mouse::RightClick(0, 40)),
            MouseOutcome::ContextMenu {
                line: 0,
                column: 40
            }
        );
        assert_eq!(
            click_outcome(&map, &Mouse::RightClick(-2, 3)),
            MouseOutcome::ContextMenu { line: 0, column: 3 }
        );
    }

    #[test]
    fn other_mouse_events_do_nothing() {
        let map = ClickMap::lay_out(two_hints(), 1, None);
        assert_eq!(
            click_outcome(&map, &Mouse::Hover(0, 1)),
            MouseOutcome::Nothing
        );
        assert_eq!(
            click_outcome(&map, &Mouse::ScrollUp(1)),
            MouseOutcome::Nothing
        );
    }

    #[test]
    fn a_hint_with_several_actions_asks_which_to_run() {
        let focus = HintTarget {
            label: "hl  focus".to_string(),
            choices: vec![
                HintChoice {
                    label: "h  left".to_string(),
                    actions: vec![Action::FocusPreviousPane],
                },
                HintChoice {
                    label: "l  right".to_string(),
                    actions: vec![Action::FocusNextPane],
                },
            ],
        };
        let MouseOutcome::Menu(entries) = focus.activate() else {
            panic!("expected a menu");
        };
        let labels: Vec<&str> = entries.iter().map(|entry| entry.label.as_str()).collect();
        assert_eq!(labels, vec!["h  left", "l  right"]);
        assert_eq!(
            menu_answer_outcome(
                Some(entries),
                &PromptResult::Answered(PromptValue::Choice("1".to_string()))
            ),
            MouseOutcome::Run(vec![Action::FocusNextPane])
        );
    }

    #[test]
    fn a_hint_with_no_bound_keys_does_nothing() {
        let unbound = HintTarget {
            label: "x".to_string(),
            choices: vec![],
        };
        assert_eq!(unbound.activate(), MouseOutcome::Nothing);
    }

    #[test]
    fn the_drop_marker_lists_the_dropped_hints() {
        let dropped = vec![
            hint("c", vec![Action::TogglePaneFrames]),
            hint("d", vec![Action::TogglePanePinned]),
        ];
        let segments = vec![
            segment(3, SegmentContent::Hint(hint("a", vec![Action::CloseFocus]))),
            segment(1, SegmentContent::Spacer),
            segment(1, SegmentContent::Dropped(dropped.clone())),
        ];
        let map = ClickMap::lay_out(segments, 0, None);
        let MouseOutcome::Menu(entries) = click_outcome(&map, &left(4)) else {
            panic!("expected a menu");
        };
        let labels: Vec<&str> = entries.iter().map(|entry| entry.label.as_str()).collect();
        assert_eq!(labels, vec!["c", "d"]);
        // Choosing an entry is the same as clicking that hint.
        assert_eq!(
            menu_answer_outcome(
                Some(entries),
                &PromptResult::Answered(PromptValue::Choice("1".to_string()))
            ),
            MouseOutcome::Run(vec![Action::TogglePanePinned])
        );
    }

    #[test]
    fn a_cut_keeps_the_visible_part_of_a_hint_and_lists_the_rest() {
        // `a` is cut after its first column, then `...` takes columns 2 to 4.
        let cut = Cut {
            at: 2,
            overflow: 2..5,
        };
        let map = ClickMap::lay_out(two_hints(), 1, Some(cut));
        assert_eq!(
            click_outcome(&map, &left(1)),
            MouseOutcome::Run(vec![Action::CloseFocus])
        );
        let MouseOutcome::Menu(entries) = click_outcome(&map, &left(3)) else {
            panic!("expected a menu");
        };
        let labels: Vec<&str> = entries.iter().map(|entry| entry.label.as_str()).collect();
        assert_eq!(labels, vec!["a", "b"]);
        assert_eq!(click_outcome(&map, &left(5)), MouseOutcome::Nothing);
    }

    #[test]
    fn a_cut_that_takes_the_drop_marker_lists_its_hints_too() {
        let segments = vec![
            segment(3, SegmentContent::Hint(hint("a", vec![Action::CloseFocus]))),
            segment(1, SegmentContent::Spacer),
            segment(
                1,
                SegmentContent::Dropped(vec![hint("c", vec![Action::TogglePaneFrames])]),
            ),
        ];
        let cut = Cut {
            at: 3,
            overflow: 3..6,
        };
        let map = ClickMap::lay_out(segments, 0, Some(cut));
        let MouseOutcome::Menu(entries) = click_outcome(&map, &left(4)) else {
            panic!("expected a menu");
        };
        let labels: Vec<&str> = entries.iter().map(|entry| entry.label.as_str()).collect();
        assert_eq!(labels, vec!["c"]);
    }

    #[test]
    fn a_cut_that_hides_nothing_adds_no_menu() {
        // The cut falls in the spacer: `a` is whole and nothing follows it.
        let segments = vec![segment(
            3,
            SegmentContent::Hint(hint("a", vec![Action::CloseFocus])),
        )];
        let cut = Cut {
            at: 3,
            overflow: 3..6,
        };
        let map = ClickMap::lay_out(segments, 0, Some(cut));
        assert_eq!(click_outcome(&map, &left(4)), MouseOutcome::Nothing);
    }

    #[test]
    fn answers_other_than_a_known_choice_do_nothing() {
        let entries = || {
            Some(vec![MenuEntry {
                label: "a".to_string(),
                pick: MenuPick::Run(vec![Action::CloseFocus]),
            }])
        };
        let choice = |value: &str| PromptResult::Answered(PromptValue::Choice(value.to_string()));
        assert_eq!(
            menu_answer_outcome(entries(), &PromptResult::Cancelled),
            MouseOutcome::Nothing
        );
        assert_eq!(
            menu_answer_outcome(entries(), &PromptResult::TimedOut(None)),
            MouseOutcome::Nothing
        );
        assert_eq!(
            menu_answer_outcome(entries(), &PromptResult::Error("x".to_string())),
            MouseOutcome::Nothing
        );
        assert_eq!(
            menu_answer_outcome(entries(), &choice("1")),
            MouseOutcome::Nothing
        );
        assert_eq!(
            menu_answer_outcome(entries(), &choice("-1")),
            MouseOutcome::Nothing
        );
        assert_eq!(
            menu_answer_outcome(None, &choice("0")),
            MouseOutcome::Nothing
        );
        assert_eq!(
            menu_answer_outcome(entries(), &choice("0")),
            MouseOutcome::Run(vec![Action::CloseFocus])
        );
    }

    #[test]
    fn queued_actions_start_one_at_a_time_in_order() {
        let mut queue = ActionQueue::default();
        assert_eq!(
            queue.enqueue(vec![Action::CloseFocus, Action::Detach]),
            Some(Action::CloseFocus)
        );
        // A second click while the first binding runs waits its turn.
        assert_eq!(queue.enqueue(vec![Action::TogglePaneFrames]), None);
        assert_eq!(queue.completed(), Some(Action::Detach));
        assert_eq!(queue.completed(), Some(Action::TogglePaneFrames));
        assert_eq!(queue.completed(), None);
        // Idle again, so the next binding starts straight away.
        assert_eq!(
            queue.enqueue(vec![Action::TogglePanePinned]),
            Some(Action::TogglePanePinned)
        );
    }

    #[test]
    fn an_empty_binding_starts_nothing() {
        let mut queue = ActionQueue::default();
        assert_eq!(queue.enqueue(vec![]), None);
        assert_eq!(queue.enqueue(vec![Action::Detach]), Some(Action::Detach));
    }

    #[test]
    fn menus_open_at_the_mouse_with_each_entry_keyed_by_position() {
        let request = menu_request(&[
            MenuEntry {
                label: "first".to_string(),
                pick: MenuPick::Run(vec![]),
            },
            MenuEntry {
                label: "second".to_string(),
                pick: MenuPick::Run(vec![]),
            },
        ]);
        assert_eq!(request.placement, Some(PromptPlacement::Mouse));
        let zellij_tile::prelude::PromptSpec::Menu { items, .. } = request.spec else {
            panic!("expected a menu");
        };
        assert_eq!(
            items,
            vec![
                ChoiceItem::labeled("0", "first"),
                ChoiceItem::labeled("1", "second")
            ]
        );
    }

    /// A hint whose keys do different things, so clicking it opens a menu.
    fn choosy(name: &str) -> HintTarget {
        HintTarget {
            label: name.to_string(),
            choices: vec![
                HintChoice {
                    label: format!("{name}1  first"),
                    actions: vec![Action::FocusPreviousPane],
                },
                HintChoice {
                    label: format!("{name}2  second"),
                    actions: vec![Action::FocusNextPane],
                },
            ],
        }
    }

    fn hover(column: usize) -> Mouse {
        Mouse::Hover(0, column)
    }

    fn entries(labels: &[&str]) -> Vec<String> {
        labels.iter().map(|label| label.to_string()).collect()
    }

    /// `a` (3 columns, one action), a spacer, `f` (4 columns, two choices),
    /// a spacer, then the drop marker (1 column) standing for `c` and `d`,
    /// starting at column 1.
    fn preview_bar() -> ClickMap {
        ClickMap::lay_out(
            vec![
                segment(3, SegmentContent::Hint(hint("a", vec![Action::CloseFocus]))),
                segment(1, SegmentContent::Spacer),
                segment(4, SegmentContent::Hint(choosy("f"))),
                segment(1, SegmentContent::Spacer),
                segment(
                    1,
                    SegmentContent::Dropped(vec![
                        hint("c", vec![Action::TogglePaneFrames]),
                        hint("d", vec![Action::TogglePanePinned]),
                    ]),
                ),
            ],
            1,
            None,
        )
    }

    #[test]
    fn only_hints_that_open_a_menu_have_a_preview() {
        let map = preview_bar().with_settings(0..1);
        // The mode prefix, the one-action hint and the gaps between hints.
        for column in [0, 1, 3, 4, 9, 20] {
            assert_eq!(hover_preview(&map, &hover(column)), None, "{column}");
        }
        // The hint with two choices, on every column it covers.
        for column in 5..=8 {
            assert_eq!(
                hover_preview(&map, &hover(column)),
                Some(Preview {
                    columns: 5..9,
                    entries: entries(&["f1  first", "f2  second"]),
                }),
                "{column}"
            );
        }
        // The drop marker.
        assert_eq!(
            hover_preview(&map, &hover(10)),
            Some(Preview {
                columns: 10..11,
                entries: entries(&["c", "d"]),
            })
        );
    }

    #[test]
    fn only_a_hover_on_the_hint_line_has_a_preview() {
        let map = preview_bar();
        assert!(hover_preview(&map, &hover(6)).is_some());
        assert_eq!(hover_preview(&map, &Mouse::Hover(1, 6)), None);
        assert_eq!(hover_preview(&map, &Mouse::Hover(-1, 6)), None);
        assert_eq!(hover_preview(&map, &Mouse::Hover(0, usize::MAX)), None);
        assert_eq!(hover_preview(&map, &Mouse::LeftClick(0, 6)), None);
        assert_eq!(hover_preview(&map, &Mouse::ScrollUp(1)), None);
    }

    #[test]
    fn a_hint_with_no_bound_keys_or_a_marker_with_no_hints_has_no_preview() {
        let unbound = HintTarget {
            label: "x".to_string(),
            choices: vec![],
        };
        let segments = vec![
            segment(2, SegmentContent::Hint(unbound)),
            segment(2, SegmentContent::Dropped(vec![])),
        ];
        let map = ClickMap::lay_out(segments, 0, None);
        assert_eq!(hover_preview(&map, &hover(0)), None);
        assert_eq!(hover_preview(&map, &hover(2)), None);
    }

    #[test]
    fn the_overflow_marker_previews_the_hints_the_cut_took() {
        let cut = Cut {
            at: 2,
            overflow: 2..5,
        };
        let map = ClickMap::lay_out(two_hints(), 1, Some(cut));
        assert_eq!(
            hover_preview(&map, &hover(3)),
            Some(Preview {
                columns: 2..5,
                entries: entries(&["a", "b"]),
            })
        );
    }

    #[test]
    fn a_cut_hint_with_several_choices_previews_them_on_the_columns_still_showing() {
        let segments = vec![segment(6, SegmentContent::Hint(choosy("f")))];
        let cut = Cut {
            at: 3,
            overflow: 3..6,
        };
        let map = ClickMap::lay_out(segments, 0, Some(cut));
        let visible = hover_preview(&map, &hover(1)).expect("the visible part");
        assert_eq!(visible.columns, 0..3);
        assert_eq!(visible.entries, entries(&["f1  first", "f2  second"]));
        // The overflow marker lists the hint whole, by its name.
        let marker = hover_preview(&map, &hover(4)).expect("the marker");
        assert_eq!(marker.entries, entries(&["f"]));
    }

    #[test]
    fn a_preview_lists_what_clicking_would_offer() {
        let map = preview_bar();
        for column in [5, 10] {
            let MouseOutcome::Menu(menu) = click_outcome(&map, &left(column)) else {
                panic!("expected a menu");
            };
            let labels: Vec<String> = menu.into_iter().map(|entry| entry.label).collect();
            assert_eq!(
                hover_preview(&map, &hover(column)).map(|preview| preview.entries),
                Some(labels)
            );
        }
    }

    #[test]
    fn entries_survive_being_carried_in_configuration() {
        let original = entries(&[
            "h←  move focus left",
            "",
            "back\\slash",
            "two\nlines",
            "\\n is not a newline",
            "trailing\\",
        ]);
        assert_eq!(decode_entries(&encode_entries(&original)), original);
        assert_eq!(decode_entries(&encode_entries(&[])), Vec::<String>::new());
        assert_eq!(
            decode_entries(&encode_entries(&entries(&[""]))),
            entries(&[""])
        );
        let configuration = preview_configuration("7-2", &original, true);
        let popup = PreviewPopup::from_configuration(&configuration).expect("a preview");
        assert_eq!(popup.entries, original);
        assert_eq!(popup.tag, "7-2");
        assert!(popup.wide_ambiguous);
        let narrow = preview_configuration("7-2", &original, false);
        let popup = PreviewPopup::from_configuration(&narrow).expect("a preview");
        assert!(!popup.wide_ambiguous);
    }

    #[test]
    fn only_a_preview_configuration_makes_a_preview_popup() {
        assert!(PreviewPopup::from_configuration(&BTreeMap::new()).is_none());
        let bar = BTreeMap::from([("mouse".to_string(), "true".to_string())]);
        assert!(PreviewPopup::from_configuration(&bar).is_none());
        let off = BTreeMap::from([("hover_preview".to_string(), "false".to_string())]);
        assert!(PreviewPopup::from_configuration(&off).is_none());
    }

    #[test]
    fn hovering_the_same_hint_again_keeps_its_preview() {
        let map = preview_bar();
        let mut memory = PreviewMemory::default();
        let PreviewStep::Show {
            close: None,
            generation,
            preview,
        } = memory.hover(&map, &hover(5))
        else {
            panic!("expected the preview to open");
        };
        assert_eq!(preview.columns, 5..9);
        for column in [5, 6, 8, 7] {
            assert_eq!(memory.hover(&map, &hover(column)), PreviewStep::Keep);
        }
        assert_eq!(memory.dismiss(), Some(generation));
    }

    #[test]
    fn hovering_another_hint_replaces_the_preview() {
        let map = preview_bar();
        let mut memory = PreviewMemory::default();
        let PreviewStep::Show {
            generation: first, ..
        } = memory.hover(&map, &hover(5))
        else {
            panic!("expected the preview to open");
        };
        let PreviewStep::Show {
            close,
            generation: second,
            preview,
        } = memory.hover(&map, &hover(10))
        else {
            panic!("expected the preview to be replaced");
        };
        assert_eq!(close, Some(first));
        assert_ne!(first, second);
        assert_eq!(preview.entries, entries(&["c", "d"]));
    }

    #[test]
    fn hovering_off_every_picker_closes_the_preview_once() {
        let map = preview_bar();
        let mut memory = PreviewMemory::default();
        // Nothing open, nothing to close.
        assert_eq!(memory.hover(&map, &hover(1)), PreviewStep::Keep);
        let PreviewStep::Show { generation, .. } = memory.hover(&map, &hover(5)) else {
            panic!("expected the preview to open");
        };
        // A one-action hint, a gap and the line below are all off a picker.
        assert_eq!(
            memory.hover(&map, &hover(1)),
            PreviewStep::Close(generation)
        );
        assert_eq!(memory.hover(&map, &hover(4)), PreviewStep::Keep);
        let PreviewStep::Show { generation, .. } = memory.hover(&map, &hover(5)) else {
            panic!("expected the preview to open again");
        };
        assert_eq!(
            memory.hover(&map, &Mouse::Hover(1, 5)),
            PreviewStep::Close(generation)
        );
    }

    #[test]
    fn a_preview_whose_entries_changed_under_the_pointer_is_shown_again() {
        let mut memory = PreviewMemory::default();
        assert!(matches!(
            memory.hover(&preview_bar(), &hover(5)),
            PreviewStep::Show { .. }
        ));
        let changed = ClickMap::lay_out(
            vec![
                segment(4, SegmentContent::Hint(hint("a", vec![Action::CloseFocus]))),
                segment(4, SegmentContent::Hint(choosy("g"))),
            ],
            1,
            None,
        );
        let PreviewStep::Show { close, preview, .. } = memory.hover(&changed, &hover(5)) else {
            panic!("expected the preview to be shown again");
        };
        assert!(close.is_some());
        assert_eq!(preview.entries, entries(&["g1  first", "g2  second"]));
    }

    #[test]
    fn the_memory_forgets_a_preview_when_its_own_timer_fires() {
        let map = preview_bar();
        let mut memory = PreviewMemory::default();
        let PreviewStep::Show { generation, .. } = memory.hover(&map, &hover(5)) else {
            panic!("expected the preview to open");
        };
        assert_eq!(memory.timer_fired(), Some(generation));
        // Forgotten, so the same hint opens it again.
        assert!(matches!(
            memory.hover(&map, &hover(5)),
            PreviewStep::Show { close: None, .. }
        ));
    }

    #[test]
    fn a_timer_set_for_an_earlier_preview_leaves_a_later_one_alone() {
        let map = preview_bar();
        let mut memory = PreviewMemory::default();
        assert!(matches!(
            memory.hover(&map, &hover(5)),
            PreviewStep::Show { .. }
        ));
        let PreviewStep::Show { generation, .. } = memory.hover(&map, &hover(10)) else {
            panic!("expected the preview to be replaced");
        };
        // The first preview's timer, then the second's.
        assert_eq!(memory.timer_fired(), None);
        assert_eq!(memory.timer_fired(), Some(generation));
        assert_eq!(memory.timer_fired(), None);
    }

    #[test]
    fn a_dismissed_preview_is_not_closed_again_by_its_timer() {
        let map = preview_bar();
        let mut memory = PreviewMemory::default();
        assert!(matches!(
            memory.hover(&map, &hover(5)),
            PreviewStep::Show { .. }
        ));
        assert!(memory.dismiss().is_some());
        assert_eq!(memory.dismiss(), None);
        assert_eq!(memory.timer_fired(), None);
    }

    #[test]
    fn previews_are_named_by_bar_and_number() {
        assert_eq!(preview_tag(7, 2), "7-2");
        assert_ne!(preview_tag(7, 2), preview_tag(2, 7));
        assert_ne!(preview_tag(1, 11), preview_tag(11, 1));
    }

    fn popup(labels: &[&str]) -> PreviewPopup {
        PreviewPopup::from_configuration(&preview_configuration("7-2", &entries(labels), false))
            .expect("a preview")
    }

    #[test]
    fn the_pointer_leaving_the_popup_closes_it() {
        let mut popup = popup(&["a"]);
        assert_eq!(popup.on_mouse(&Mouse::Hover(1, 2)), PreviewAction::Stay);
        assert_eq!(popup.on_mouse(&Mouse::Hover(-1, 0)), PreviewAction::Close);
        assert_eq!(popup.on_mouse(&Mouse::ScrollUp(1)), PreviewAction::Stay);
    }

    #[test]
    fn clicking_the_popup_closes_it() {
        let mut popup = popup(&["a"]);
        assert_eq!(
            popup.on_mouse(&Mouse::LeftClick(1, 2)),
            PreviewAction::Close
        );
        assert_eq!(
            popup.on_mouse(&Mouse::RightClick(1, 2)),
            PreviewAction::Close
        );
    }

    #[test]
    fn a_popup_nobody_hovered_closes_when_its_time_is_up() {
        let mut popup = popup(&["a"]);
        assert_eq!(popup.on_timer(), PreviewAction::Close);
        popup.on_mouse(&Mouse::Hover(0, 0));
        assert_eq!(popup.on_timer(), PreviewAction::Stay);
    }

    #[test]
    fn a_close_message_closes_the_popup_it_names_or_every_popup() {
        let popup = popup(&["a"]);
        assert_eq!(popup.on_close_message(Some("7-2")), PreviewAction::Close);
        assert_eq!(popup.on_close_message(None), PreviewAction::Close);
        assert_eq!(popup.on_close_message(Some("")), PreviewAction::Close);
        assert_eq!(popup.on_close_message(Some("7-3")), PreviewAction::Stay);
        assert_eq!(popup.on_close_message(Some("8-2")), PreviewAction::Stay);
    }

    #[test]
    fn a_popup_is_sized_to_its_entries_and_frame() {
        let labels = entries(&["h←  move focus left", "j↓  down"]);
        let widest = "h←  move focus left".chars().count();
        assert_eq!(preview_size(&labels, false), (widest + 4, 2 + 2));
        assert_eq!(preview_size(&[], false), (4, 2));
        // A wide character takes two columns.
        assert_eq!(preview_size(&entries(&["漢字"]), false), (4 + 4, 1 + 2));
    }

    #[test]
    fn a_popup_draws_its_entries_in_a_frame_of_the_size_asked_for() {
        let popup = popup(&["h  left", "l  right"]);
        let (width, height) = preview_size(&popup.entries, false);
        assert_eq!(
            popup.lines(height, width),
            vec![
                "┌──────────┐",
                "│ h  left  │",
                "│ l  right │",
                "└──────────┘",
            ]
        );
    }

    #[test]
    fn a_popup_too_short_for_its_entries_ends_in_an_ellipsis() {
        let popup = popup(&["one", "two", "three"]);
        assert_eq!(
            popup.lines(4, 9),
            vec!["┌───────┐", "│ one   │", "│ …     │", "└───────┘"]
        );
        // With room for all of them there is no ellipsis.
        assert_eq!(popup.lines(5, 9).len(), 5);
    }

    #[test]
    fn a_popup_too_narrow_for_an_entry_cuts_it_short() {
        let popup = popup(&["a long entry"]);
        let lines = popup.lines(3, 9);
        assert_eq!(lines[0], "┌───────┐");
        // The cut ends with a reset, so no color leaks into the frame.
        assert_eq!(lines[1], "│ a lo…\u{1b}[0m │");
        assert_eq!(lines[2], "└───────┘");
    }

    #[test]
    fn a_popup_with_no_room_for_a_frame_draws_nothing() {
        let popup = popup(&["a"]);
        assert!(popup.lines(2, 20).is_empty());
        assert!(popup.lines(5, 4).is_empty());
        assert!(popup.lines(0, 0).is_empty());
    }
}
