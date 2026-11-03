//! What the mouse does on the bar: a click switches to the tab under it, or
//! to the nearest hidden tab for a marker, and opens the settings screen on
//! the mode prefix; the wheel steps through the tabs; a double-click renames
//! a tab; a middle click closes it, asking first with `mouse_close_confirm`
//! on; a right-click opens Zellij's context menu. The session name at the
//! end of the bar is no target of its own: it stands for nothing in the
//! click spans, so only a right-click acts on it, opening the bar's menu as
//! the empty end does.

use crate::host::BAR;
use crate::tabs::Clickable;
use crate::State;
use std::collections::BTreeMap;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;
use zjkit::ansi::show_controls;
use zjkit::host::Host;
use zjkit::mouse::{classify_bar_mouse, BarMouseEvent, PressKind, WheelDirection};

/// A middle click waiting on the user's answer to the close confirmation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PendingClose {
    /// The id `Host::prompt` returned, which the answer comes back with in
    /// `Event::PromptResult`.
    pub request_id: u64,
    /// The tab the click landed on, as the click map had it, so the answer
    /// closes that tab even if the tabs have moved since.
    pub tab_id: usize,
}

impl State {
    /// Act on a mouse event on the bar. Nothing here changes what the bar
    /// draws by itself: a tab switch, rename or close comes back as a
    /// `TabUpdate`.
    ///
    /// A bar that draws nothing ignores the mouse. Its row stays on screen,
    /// blank, while `collapse_when_empty` is off, and the wheel or a
    /// right-click there would act on tabs the user cannot see.
    pub(crate) fn follow_mouse(&mut self, mouse: &Mouse) {
        if self.draws_nothing() {
            return;
        }
        match classify_bar_mouse(mouse) {
            BarMouseEvent::Press { kind, column } => {
                let target = self.click_spans.value_at(column).copied();
                match (kind, target) {
                    (PressKind::Left, Some(Clickable::Tab { position, .. }))
                    | (PressKind::Left, Some(Clickable::Marker(position))) => {
                        self.host.switch_tab_to(one_based(position));
                    }
                    (PressKind::Left, Some(Clickable::ModePrefix)) => {
                        self.bar.open_settings(&self.host, &BAR)
                    }
                    (PressKind::Double, Some(Clickable::Tab { position, .. })) => {
                        self.rename_tab(position)
                    }
                    // Only on a tab: a marker stands for several hidden tabs,
                    // and closing one the user cannot see would be a surprise.
                    (PressKind::Middle, Some(Clickable::Tab { tab_id, .. })) => {
                        self.close_tab(tab_id)
                    }
                    (PressKind::Right, Some(Clickable::Tab { position, .. })) => self
                        .host
                        .open_context_menu(ContextMenuTarget::Tab(position), 0, column),
                    // A spacer, the cut of an overlong line, the empty end of
                    // the bar, a marker, the mode prefix or the session name:
                    // there is no one tab to act on.
                    (PressKind::Right, _) => {
                        self.host
                            .open_context_menu(ContextMenuTarget::Bar, 0, column)
                    }
                    _ => {}
                }
            }
            BarMouseEvent::Wheel(direction) => self.step_tab(direction),
            BarMouseEvent::Other => {}
        }
    }

    /// Start renaming the tab at `position`. Zellij renames a tab by id
    /// whether it is active or not, but the user types the new name into
    /// the active tab, so it is switched to first, as Zellij's own tab bar
    /// does.
    fn rename_tab(&self, position: usize) {
        let Some(tab) = self.tabs.iter().find(|tab| tab.position == position) else {
            return;
        };
        if !tab.active {
            self.host.switch_tab_to(one_based(position));
        }
        let action = Action::StartRenameTabByTabId {
            id: tab.tab_id as u64,
        };
        self.host.run_action(action, BTreeMap::new());
    }

    /// Close the tab whose id is `tab_id`, at once with
    /// `mouse_close_confirm` off. With it on, ask first in Zellij's prompt
    /// popup, naming the tab by its number and name as the bar draws them,
    /// and close it only if the user confirms; see `answer_close`.
    ///
    /// A middle click while a confirmation is already showing replaces it:
    /// the answer to the newer one is the one acted on, and the older one's
    /// is ignored. Keeping the older one instead would leave the bar unable
    /// to close anything if its answer never came, as when Zellij closes the
    /// popup without sending one.
    fn close_tab(&mut self, tab_id: usize) {
        if !self.mouse_close_confirm {
            self.host.close_tab_with_id(tab_id as u64);
            return;
        }
        // Gone since the line was drawn: there is nothing to ask about.
        let Some(tab) = self.tabs.iter().find(|tab| tab.tab_id == tab_id) else {
            return;
        };
        let index = if self.index_from_zero {
            tab.position
        } else {
            tab.position + 1
        };
        let message = format!(
            "Close tab {index} \"{}\" and every pane in it?",
            show_controls(&tab.name)
        );
        // Cancel is the button selected at first, so `Enter` or `Space`
        // straight away keeps the tab, as `Esc` does. Without a default,
        // Zellij's prompt selects the first button, which is the one that
        // closes.
        let request = PromptRequest::confirm(message)
            .title("Close tab?")
            .yes("Close tab")
            .no("Cancel")
            .default("false");
        let request_id = self.host.prompt(request);
        self.pending_close = Some(PendingClose { request_id, tab_id });
    }

    /// Act on Zellij's answer `result` to the prompt `request_id`. Closes the
    /// tab of the close confirmation being shown only when this is its answer
    /// and the user chose to close it, and the tab is still there in the
    /// last `TabUpdate`. Any other answer, including a dismissal, closes
    /// nothing, and an answer to another prompt is left alone.
    pub(crate) fn answer_close(&mut self, request_id: u64, result: &PromptResult) {
        let Some(pending) = self.pending_close else {
            return;
        };
        if pending.request_id != request_id {
            return;
        }
        self.pending_close = None;
        let still_there = self.tabs.iter().any(|tab| tab.tab_id == pending.tab_id);
        if *result == PromptResult::Confirmed(true) && still_there {
            self.host.close_tab_with_id(pending.tab_id as u64);
        }
    }

    /// Switch to the tab after the active one for the wheel turned up, or
    /// before it for down, as Zellij's own tab bar does; `mouse_wheel_reversed`
    /// swaps the two. At the first or last tab, the wheel does nothing:
    /// wrapping around would make a fast scroll spin through every tab.
    fn step_tab(&self, direction: WheelDirection) {
        let Some(active) = self.tabs.iter().position(|tab| tab.active) else {
            return;
        };
        let toward_next = match direction {
            WheelDirection::Up => !self.mouse_wheel_reversed,
            WheelDirection::Down => self.mouse_wheel_reversed,
        };
        let next = if toward_next {
            Some(active + 1)
        } else {
            active.checked_sub(1)
        };
        if let Some(tab) = next.and_then(|index| self.tabs.get(index)) {
            self.host.switch_tab_to(one_based(tab.position));
        }
    }
}

/// `position`, counted from 0 as `TabInfo` does, counted from 1 as
/// `switch_tab_to` takes it.
fn one_based(position: usize) -> u32 {
    (position + 1) as u32
}
