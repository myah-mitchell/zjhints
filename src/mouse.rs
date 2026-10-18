//! Mouse support: which columns of the bar stand for what, and what a click on
//! them should do.
//!
//! Everything here is plain data and pure functions, so the whole path from a
//! click to its outcome can be tested without a running Zellij. Carrying an
//! outcome out (opening the context menu, running actions, showing a menu) is
//! left to `State::perform_mouse_outcome` and `State::run_mouse_action`, the
//! only parts that talk to the host.

use std::collections::VecDeque;
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

    fn push(&mut self, columns: Range<usize>, target: ClickTarget) {
        if !columns.is_empty() {
            self.regions.push(ClickRegion { columns, target });
        }
    }

    fn target_at(&self, column: usize) -> Option<&ClickTarget> {
        self.regions
            .iter()
            .find(|region| region.columns.contains(&column))
            .map(|region| &region.target)
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
        Mouse::LeftClick(0, column) => match map.target_at(*column) {
            Some(ClickTarget::Hint(hint)) => hint.activate(),
            Some(ClickTarget::HiddenHints(hints)) => hidden_hints_menu(hints),
            None => MouseOutcome::Nothing,
        },
        _ => MouseOutcome::Nothing,
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
}
