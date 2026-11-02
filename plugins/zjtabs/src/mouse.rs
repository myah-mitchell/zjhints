//! What the mouse does on the bar: a click switches to the tab under it, or
//! to the nearest hidden tab for a marker; the wheel steps through the tabs;
//! a double-click renames a tab; a right-click opens Zellij's context menu.

use crate::tabs::Clickable;
use crate::State;
use std::collections::BTreeMap;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;
use zjkit::host::Host;
use zjkit::mouse::{classify_bar_mouse, BarMouseEvent, PressKind, WheelDirection};

impl State {
    /// Act on a mouse event on the bar. Nothing here changes what the bar
    /// draws by itself: a tab switch or rename comes back as a `TabUpdate`.
    ///
    /// A bar that draws nothing ignores the mouse. Its row stays on screen,
    /// blank, while `collapse_when_empty` is off, and the wheel or a
    /// right-click there would act on tabs the user cannot see.
    pub(crate) fn follow_mouse(&self, mouse: &Mouse) {
        if self.draws_nothing() {
            return;
        }
        match classify_bar_mouse(mouse) {
            BarMouseEvent::Press { kind, column } => {
                let target = self.click_spans.value_at(column).copied();
                match (kind, target) {
                    (PressKind::Left, Some(Clickable::Tab(position)))
                    | (PressKind::Left, Some(Clickable::Marker(position))) => {
                        self.host.switch_tab_to(one_based(position));
                    }
                    (PressKind::Double, Some(Clickable::Tab(position))) => {
                        self.rename_tab(position)
                    }
                    (PressKind::Right, Some(Clickable::Tab(position))) => self
                        .host
                        .open_context_menu(ContextMenuTarget::Tab(position), 0, column),
                    // A spacer, the cut of an overlong line, the empty end of
                    // the bar, or a marker: there is no one tab to act on.
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
