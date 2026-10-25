//! Mouse support: which columns of the bar stand for what, and what a click on
//! them should do.
//!
//! Everything here is plain data and pure functions, so the whole path from a
//! click to its outcome can be tested without a running Zellij. Carrying an
//! outcome out (opening the context menu, running actions, showing a menu) is
//! left to `State::perform_mouse_outcome` and `State::run_mouse_action`, the
//! only parts that talk to the host. The hover preview, which lists what a
//! menu would offer before it is clicked, is split the same way: what to show
//! and when is decided here, and the popup itself is `crate::popup`.

use std::collections::VecDeque;
use std::ops::Range;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::{
    ChoiceItem, Mouse, OutsideClick, PromptPlacement, PromptRequest, PromptResult, PromptValue,
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
/// is its index, which is how `menu_answer_outcome` finds it again. A click
/// outside it closes it, as it does Zellij's own context menu.
pub fn menu_request(entries: &[MenuEntry]) -> PromptRequest {
    PromptRequest::menu(
        entries
            .iter()
            .enumerate()
            .map(|(index, entry)| ChoiceItem::labeled(index.to_string(), entry.label.clone())),
    )
    .placement(PromptPlacement::Mouse)
    .outside_click(OutsideClick::Close)
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

/// How long the pointer rests on a hint before its preview opens, so that
/// sweeping across the bar opens none.
pub const HOVER_DELAY_SECONDS: f64 = 0.3;

/// Sent by a preview popup to the bar that opened it, with the popup's tag:
/// `HOVER_OPENED_MESSAGE` once it can take messages, so the bar learns its
/// plugin id, and `HOVER_CLOSED_MESSAGE` as it closes.
pub const HOVER_OPENED_MESSAGE: &str = "zjhints::hover_opened";
pub const HOVER_CLOSED_MESSAGE: &str = "zjhints::hover_closed";
/// Sent by the bar to one of its popups to close it.
pub const HOVER_CLOSE_MESSAGE: &str = "zjhints::hover_close";

/// The column of the hover Zellij sends a plugin pane the pointer has left,
/// which includes moving onto a popup over the bar.
const LEFT_PANE_COLUMN: usize = u16::MAX as usize;

/// Which preview the bar is waiting to open or has open, so that hovering the
/// same hint again leaves it be and hovering something else replaces it.
#[derive(Debug, Default)]
pub struct PreviewMemory {
    /// The preview waiting out `HOVER_DELAY_SECONDS`, and its timer's number.
    waiting: Option<(Preview, u64)>,
    shown: Option<ShownPreview>,
    /// Previews opened so far, which numbers each one.
    opened: u64,
    /// Delay timers set and fired. Zellij fires timers of one length in the
    /// order they were set, so counting matches a firing to its hover.
    timers_set: u64,
    timers_fired: u64,
}

#[derive(Debug)]
struct ShownPreview {
    generation: u64,
    preview: Preview,
    /// The popup's plugin id, once it has said.
    popup: Option<u32>,
}

/// What the bar does about a mouse event, a timer or a popup's message.
#[derive(Debug, Default, PartialEq)]
pub struct PreviewStep {
    /// Close the popup with this plugin id.
    pub close: Option<u32>,
    /// Set a timer of `HOVER_DELAY_SECONDS`.
    pub wait: bool,
    /// Open this preview as number `.0`.
    pub open: Option<(u64, Preview)>,
}

impl PreviewMemory {
    pub fn hover(&mut self, map: &ClickMap, mouse: &Mouse) -> PreviewStep {
        // Perhaps onto the popup, which closes itself when the pointer leaves
        // it or never arrives.
        if matches!(mouse, Mouse::Hover(_, column) if *column >= LEFT_PANE_COLUMN) {
            self.waiting = None;
            return PreviewStep::default();
        }
        let Some(preview) = hover_preview(map, mouse) else {
            return self.dismiss();
        };
        let is_shown = self
            .shown
            .as_ref()
            .is_some_and(|shown| shown.preview == preview);
        let is_waiting = self
            .waiting
            .as_ref()
            .is_some_and(|(waiting, _)| *waiting == preview);
        if is_shown || is_waiting {
            return PreviewStep::default();
        }
        let mut step = self.dismiss();
        self.timers_set += 1;
        self.waiting = Some((preview, self.timers_set));
        step.wait = true;
        step
    }

    /// Forget the preview waiting or shown, closing its popup.
    pub fn dismiss(&mut self) -> PreviewStep {
        self.waiting = None;
        PreviewStep {
            close: self.shown.take().and_then(|shown| shown.popup),
            ..PreviewStep::default()
        }
    }

    /// Opens the preview the timer was set for, if the pointer stayed.
    pub fn timer_fired(&mut self) -> PreviewStep {
        self.timers_fired += 1;
        let is_due = self
            .waiting
            .as_ref()
            .is_some_and(|(_, timer)| *timer == self.timers_fired);
        let Some((preview, _)) = self.waiting.take_if(|_| is_due) else {
            return PreviewStep::default();
        };
        self.opened += 1;
        self.shown = Some(ShownPreview {
            generation: self.opened,
            preview: preview.clone(),
            popup: None,
        });
        PreviewStep {
            open: Some((self.opened, preview)),
            ..PreviewStep::default()
        }
    }

    /// Popup `popup` opened as preview number `generation`. A popup the bar
    /// has moved on from is closed: it loaded after its close was due.
    pub fn popup_opened(&mut self, popup: u32, generation: u64) -> PreviewStep {
        match &mut self.shown {
            Some(shown) if shown.generation == generation => {
                shown.popup = Some(popup);
                PreviewStep::default()
            }
            _ => PreviewStep {
                close: Some(popup),
                ..PreviewStep::default()
            },
        }
    }

    /// Preview number `generation` closed, so its hint can open it again.
    pub fn popup_closed(&mut self, generation: u64) {
        if self
            .shown
            .as_ref()
            .is_some_and(|shown| shown.generation == generation)
        {
            self.shown = None;
        }
    }

    #[cfg(test)]
    pub fn is_idle(&self) -> bool {
        self.waiting.is_none() && self.shown.is_none()
    }
}

/// Names preview number `generation` of the bar `owner`.
pub fn preview_tag(owner: u32, generation: u64) -> String {
    format!("{owner}-{generation}")
}

/// The bar and preview number a `preview_tag` names.
pub fn parse_preview_tag(tag: &str) -> Option<(u32, u64)> {
    let (owner, generation) = tag.split_once('-')?;
    Some((owner.parse().ok()?, generation.parse().ok()?))
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
    fn a_click_outside_a_menu_closes_it() {
        let entries = [MenuEntry {
            label: "a".to_string(),
            pick: MenuPick::Run(vec![Action::CloseFocus]),
        }];
        assert_eq!(
            menu_request(&entries).outside_click_or_default(),
            OutsideClick::Close
        );
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

    /// Hover `column` and have its delay timer fire, returning what opens.
    fn rest_on(memory: &mut PreviewMemory, map: &ClickMap, column: usize) -> (u64, Preview) {
        assert!(memory.hover(map, &hover(column)).wait, "{column} waits");
        memory.timer_fired().open.expect("the preview to open")
    }

    #[test]
    fn a_preview_opens_once_the_pointer_has_rested_on_its_hint() {
        let map = preview_bar();
        let mut memory = PreviewMemory::default();
        let step = memory.hover(&map, &hover(5));
        assert_eq!(
            step,
            PreviewStep {
                wait: true,
                ..PreviewStep::default()
            }
        );
        // Moving within the hint keeps waiting, with no second timer.
        assert_eq!(memory.hover(&map, &hover(7)), PreviewStep::default());
        let (_, preview) = memory.timer_fired().open.expect("the preview to open");
        assert_eq!(preview.columns, 5..9);
        assert_eq!(preview.entries, entries(&["f1  first", "f2  second"]));
    }

    #[test]
    fn sweeping_across_hints_opens_only_the_last() {
        let map = preview_bar();
        let mut memory = PreviewMemory::default();
        assert!(memory.hover(&map, &hover(5)).wait);
        assert!(memory.hover(&map, &hover(10)).wait);
        // The first hint's timer finds the pointer gone.
        assert_eq!(memory.timer_fired(), PreviewStep::default());
        let (_, preview) = memory.timer_fired().open.expect("the second to open");
        assert_eq!(preview.entries, entries(&["c", "d"]));
    }

    #[test]
    fn leaving_a_hint_before_its_delay_opens_nothing() {
        let map = preview_bar();
        let mut memory = PreviewMemory::default();
        assert!(memory.hover(&map, &hover(5)).wait);
        assert_eq!(memory.hover(&map, &hover(1)), PreviewStep::default());
        assert_eq!(memory.timer_fired(), PreviewStep::default());
        assert!(memory.is_idle());
    }

    #[test]
    fn hovering_the_same_hint_again_keeps_its_preview() {
        let map = preview_bar();
        let mut memory = PreviewMemory::default();
        let (generation, _) = rest_on(&mut memory, &map, 5);
        memory.popup_opened(30, generation);
        for column in [5, 6, 8, 7] {
            assert_eq!(memory.hover(&map, &hover(column)), PreviewStep::default());
        }
        assert_eq!(memory.dismiss().close, Some(30));
    }

    #[test]
    fn hovering_another_hint_closes_the_preview_and_waits_for_the_new_one() {
        let map = preview_bar();
        let mut memory = PreviewMemory::default();
        let (first, _) = rest_on(&mut memory, &map, 5);
        memory.popup_opened(30, first);
        let step = memory.hover(&map, &hover(10));
        assert_eq!(step.close, Some(30));
        assert!(step.wait);
        let (second, _) = memory.timer_fired().open.expect("the new one to open");
        assert_ne!(first, second);
    }

    #[test]
    fn hovering_off_every_picker_closes_the_preview_once() {
        let map = preview_bar();
        let mut memory = PreviewMemory::default();
        // Nothing open, nothing to close.
        assert_eq!(memory.hover(&map, &hover(1)), PreviewStep::default());
        let (generation, _) = rest_on(&mut memory, &map, 5);
        memory.popup_opened(30, generation);
        // A one-action hint, a gap and the line below are all off a picker.
        assert_eq!(memory.hover(&map, &hover(1)).close, Some(30));
        assert_eq!(memory.hover(&map, &hover(4)), PreviewStep::default());
        let (generation, _) = rest_on(&mut memory, &map, 5);
        memory.popup_opened(31, generation);
        assert_eq!(memory.hover(&map, &Mouse::Hover(1, 5)).close, Some(31));
    }

    #[test]
    fn the_pointer_leaving_the_bar_for_the_popup_keeps_it_open() {
        let map = preview_bar();
        let mut memory = PreviewMemory::default();
        let (generation, _) = rest_on(&mut memory, &map, 5);
        memory.popup_opened(30, generation);
        // What Zellij sends the bar as the pointer moves onto the popup.
        let left = Mouse::Hover(0, u16::MAX as usize);
        assert_eq!(memory.hover(&map, &left), PreviewStep::default());
        // Leaving the popup closes it, and the bar hears so.
        memory.popup_closed(generation);
        assert!(memory.is_idle());
        assert!(memory.hover(&map, &hover(5)).wait);
    }

    #[test]
    fn leaving_the_bar_cancels_a_preview_still_waiting() {
        let map = preview_bar();
        let mut memory = PreviewMemory::default();
        assert!(memory.hover(&map, &hover(5)).wait);
        memory.hover(&map, &Mouse::Hover(0, u16::MAX as usize));
        assert_eq!(memory.timer_fired(), PreviewStep::default());
    }

    #[test]
    fn a_popup_that_closed_itself_can_be_opened_again() {
        let map = preview_bar();
        let mut memory = PreviewMemory::default();
        let (generation, _) = rest_on(&mut memory, &map, 5);
        memory.popup_opened(30, generation);
        memory.popup_closed(generation);
        assert!(memory.hover(&map, &hover(6)).wait);
        // Nothing left to close.
        assert_eq!(memory.dismiss(), PreviewStep::default());
    }

    #[test]
    fn a_popup_the_bar_has_moved_on_from_is_closed_when_it_opens() {
        let map = preview_bar();
        let mut memory = PreviewMemory::default();
        let (first, _) = rest_on(&mut memory, &map, 5);
        // Dismissed before the popup said where it is.
        assert_eq!(memory.dismiss(), PreviewStep::default());
        assert_eq!(memory.popup_opened(30, first).close, Some(30));
        // The same goes for an earlier preview's popup while a later one shows.
        let (second, _) = rest_on(&mut memory, &map, 10);
        assert_eq!(memory.popup_opened(31, first).close, Some(31));
        assert_eq!(memory.popup_opened(32, second), PreviewStep::default());
        // An earlier popup closing leaves the later one remembered.
        memory.popup_closed(first);
        assert_eq!(memory.dismiss().close, Some(32));
    }

    #[test]
    fn a_preview_whose_entries_changed_under_the_pointer_is_shown_again() {
        let mut memory = PreviewMemory::default();
        rest_on(&mut memory, &preview_bar(), 5);
        let changed = ClickMap::lay_out(
            vec![
                segment(4, SegmentContent::Hint(hint("a", vec![Action::CloseFocus]))),
                segment(4, SegmentContent::Hint(choosy("g"))),
            ],
            1,
            None,
        );
        let (_, preview) = rest_on(&mut memory, &changed, 5);
        assert_eq!(preview.entries, entries(&["g1  first", "g2  second"]));
    }

    #[test]
    fn previews_are_named_by_bar_and_number() {
        assert_eq!(preview_tag(7, 2), "7-2");
        assert_eq!(parse_preview_tag("7-2"), Some((7, 2)));
        assert_ne!(preview_tag(1, 11), preview_tag(11, 1));
        for stray in ["", "7", "7-", "-2", "a-2", "7-b"] {
            assert_eq!(parse_preview_tag(stray), None, "{stray:?}");
        }
    }
}
