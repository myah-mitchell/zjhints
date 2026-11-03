//! What the mouse does on the bar, the way a desktop taskbar works.
//!
//! A left click on an entry brings its pane up: it focuses the pane,
//! switching to its tab, and restores a minimized pane to where it stood. A
//! left click on the entry already focused minimizes its pane instead. A
//! left click on a marker acts on the hidden entry nearest to it.
//!
//! A double-click renames a pane, and a middle click closes one, asking
//! first while `mouse_close_confirm` is on. A right-click on an entry opens a
//! menu of what can be done to its pane, and anywhere else opens Zellij's
//! own bar menu. The wheel steps the focus through the panes on screen.
//!
//! A left click on the launcher's button opens its menu, and a right-click
//! there opens Zellij's bar menu, as anywhere off an entry does. See
//! `launcher`.
//!
//! Minimizing hides a pane with `Host::hide_pane_with_id`, which Zellij
//! reports as suppressed. Zellij forgets where a hidden floating pane stood:
//! focusing it again places it as if it were new, with a frame. So the bar
//! records the pane's place and size as it hides it, and puts it back there
//! as it restores it; see `geometry_of`.

use crate::draw::Clickable;
use crate::launcher::LauncherItem;
use crate::State;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;
use zellij_utils::input::layout::PercentOrFixed;
use zjkit::ansi::show_controls;
use zjkit::host::Host;
use zjkit::mouse::{
    classify_bar_mouse, labeled_menu_request, menu_choice, BarMouseEvent, PressKind, WheelDirection,
};
use zjkit::panes::pane_id_of;

/// How soon after one left click a second on the same column counts as the
/// second half of a double-click. The same as Zellij's own threshold,
/// `CLICK_TIME_THRESHOLD_MS` in its mouse handler.
///
/// Zellij reports a double-click to a plugin as a left click, a second left
/// click, and then the double-click. A left click on an entry toggles its
/// pane between focused and minimized, so acting on both left clicks would
/// undo the first with the second. The second is ignored instead.
pub(crate) const DOUBLE_CLICK_TIME: Duration = Duration::from_millis(400);

/// A left click on the bar, kept until the next one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LeftClick {
    /// When the click came.
    pub at: Instant,
    pub column: usize,
    /// What the click landed on, as the line drawn then had it. A
    /// double-click acts on this, since the first click may have moved the
    /// entries by the time the double-click arrives.
    pub target: Option<Clickable>,
}

/// What a prompt the bar is waiting on asked about.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Prompted {
    /// The close confirmation for the pane.
    Close(PaneId),
    /// The menu for the pane, with the items it showed, in order, so the
    /// index of the one chosen finds it again.
    PaneMenu {
        pane: PaneId,
        items: Vec<PaneMenuItem>,
    },
    /// The launcher's menu, with the items it showed, in order, as the
    /// pane menu keeps its own.
    Launcher { items: Vec<LauncherItem> },
}

/// One item of the menu a right-click on an entry opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaneMenuItem {
    /// Show a minimized pane again where it stood, and focus it.
    Restore,
    /// Focus a pane on screen that the keyboard does not go to.
    Focus,
    /// Hide a pane on screen, keeping it on the bar.
    Minimize,
    Rename,
    Close,
}

impl PaneMenuItem {
    /// How the item is named in the menu.
    pub(crate) fn label(self) -> &'static str {
        match self {
            PaneMenuItem::Restore => "Restore",
            PaneMenuItem::Focus => "Focus",
            PaneMenuItem::Minimize => "Minimize",
            PaneMenuItem::Rename => "Rename",
            PaneMenuItem::Close => "Close",
        }
    }

    /// The items that apply to a pane `minimized` or `focused`, in the order
    /// the menu lists them: a minimized pane can be restored, a pane on
    /// screen focused unless it already is, and minimized.
    pub(crate) fn applying_to(minimized: bool, focused: bool) -> Vec<PaneMenuItem> {
        let mut items = vec![];
        if minimized {
            items.push(PaneMenuItem::Restore);
        } else {
            if !focused {
                items.push(PaneMenuItem::Focus);
            }
            items.push(PaneMenuItem::Minimize);
        }
        items.extend([PaneMenuItem::Rename, PaneMenuItem::Close]);
        items
    }
}

/// Where `pane` stands on screen, as `Host::change_floating_panes_coordinates`
/// takes it to put the pane back there exactly.
///
/// Zellij reports a pane's place and size with its frame included, in
/// `pane_x`, `pane_y`, `pane_columns` and `pane_rows`, and takes a fixed
/// coordinate as that same outer edge: a fixed `x` is the screen column the
/// frame starts on, not one counted from the edge of the panes' area. So
/// each is passed back as it was read. Zellij keeps the pane inside the
/// screen, so a pane restored after the terminal shrank is moved in to fit.
///
/// Whether the pane is drawn without a frame is passed back too, as Zellij
/// gives a pane it places as new a frame. A floating pane has no frame
/// exactly when its content fills the whole of it. A fullscreen pane fills
/// the screen either way, so for one that is left as Zellij has it.
///
/// Whether the pane was pinned above the others is not in `PaneInfo`, so it
/// is left as Zellij has it too.
pub(crate) fn geometry_of(pane: &PaneInfo) -> FloatingPaneCoordinates {
    let borderless =
        pane.pane_content_columns == pane.pane_columns && pane.pane_content_rows == pane.pane_rows;
    FloatingPaneCoordinates {
        x: Some(PercentOrFixed::Fixed(pane.pane_x)),
        y: Some(PercentOrFixed::Fixed(pane.pane_y)),
        width: Some(PercentOrFixed::Fixed(pane.pane_columns)),
        height: Some(PercentOrFixed::Fixed(pane.pane_rows)),
        pinned: None,
        borderless: (!pane.is_fullscreen).then_some(borderless),
        border_style: None,
    }
}

impl State {
    /// Act on a mouse event on the bar, which came at `now`. Nothing here
    /// changes what the bar draws by itself: what a click does comes back as
    /// a `PaneUpdate`.
    ///
    /// A bar whose line is blank ignores the mouse. Its row stays on screen
    /// while `collapse_when_empty` is off, and the wheel or a right-click
    /// there would act on panes the user cannot see listed.
    pub(crate) fn follow_mouse(&mut self, mouse: &Mouse, now: Instant) {
        if self.draws_blank_line() {
            return;
        }
        match classify_bar_mouse(mouse) {
            BarMouseEvent::Press {
                kind: PressKind::Left,
                column,
            } => self.left_click(column, now),
            BarMouseEvent::Press {
                kind: PressKind::Double,
                column,
            } => {
                // The target of the first click of the pair, when this is
                // its double-click, as `LeftClick::target` explains.
                let target = match self.last_left_click {
                    Some(click) if click.column == column => click.target,
                    _ => self.click_spans.value_at(column).copied(),
                };
                if let Some(Clickable::Entry(pane)) = target {
                    self.rename_pane(pane);
                }
            }
            BarMouseEvent::Press { kind, column } => {
                match (kind, self.click_spans.value_at(column).copied()) {
                    // Only on an entry: a marker stands for several hidden
                    // entries, and closing one the user cannot see would be
                    // a surprise.
                    (PressKind::Middle, Some(Clickable::Entry(pane))) => self.close_pane(pane),
                    (PressKind::Right, Some(Clickable::Entry(pane))) => self.open_pane_menu(pane),
                    // The launcher's button, a spacer, a separator, a marker,
                    // the cut of an overlong line or the empty end of the
                    // bar: there is no one pane to act on.
                    (PressKind::Right, _) => {
                        self.host
                            .open_context_menu(ContextMenuTarget::Bar, 0, column)
                    }
                    _ => {}
                }
            }
            BarMouseEvent::Wheel(direction) => self.step_focus(direction),
            BarMouseEvent::Other => {}
        }
    }

    /// A left click at `column`, which came at `now`: bring the pane of the
    /// entry under it up, or minimize it if it is the one focused. A marker
    /// acts as its nearest hidden entry would, and the hidden entries are
    /// never focused, so a click on one never minimizes. On the launcher's
    /// button, open its menu.
    ///
    /// A click that follows the last on the same column within
    /// `DOUBLE_CLICK_TIME` is the second half of a double-click, and does
    /// nothing; see `DOUBLE_CLICK_TIME`.
    fn left_click(&mut self, column: usize, now: Instant) {
        let target = self.click_spans.value_at(column).copied();
        let repeated = self.last_left_click.is_some_and(|click| {
            click.column == column && now.saturating_duration_since(click.at) < DOUBLE_CLICK_TIME
        });
        self.last_left_click = Some(LeftClick {
            at: now,
            column,
            target,
        });
        if repeated {
            return;
        }
        let pane = match target {
            Some(Clickable::Entry(pane) | Clickable::Marker(pane)) => pane,
            Some(Clickable::Launcher) => {
                self.open_launcher_menu();
                return;
            }
            None => return,
        };
        let Some(entry) = self.entries.iter().find(|entry| entry.pane == pane) else {
            return;
        };
        if entry.minimized {
            self.restore(pane);
        } else if entry.focused {
            self.minimize(pane);
        } else {
            self.focus(pane);
        }
    }

    /// Focus `pane`, switching to its tab if it is in another, and showing
    /// the tab's floating panes if they are hidden.
    fn focus(&self, pane: PaneId) {
        // A hidden pane is shown as a floating pane, rather than tiled or in
        // place of another. Only `restore` puts one back where it stood.
        let should_float_if_hidden = true;
        let should_be_in_place_if_hidden = false;
        match pane {
            PaneId::Terminal(id) => self.host.focus_terminal_pane(
                id,
                should_float_if_hidden,
                should_be_in_place_if_hidden,
            ),
            PaneId::Plugin(id) => self.host.focus_plugin_pane(
                id,
                should_float_if_hidden,
                should_be_in_place_if_hidden,
            ),
        }
    }

    /// Show `pane`, which the bar minimized, again where it stood, focused.
    ///
    /// Focusing a hidden pane shows it floating at once, in the same step,
    /// and Zellij handles the move after it in the order sent, so the move
    /// can follow straight away rather than wait for the `PaneUpdate`
    /// showing the pane.
    ///
    /// The pane is forgotten as minimized at once, but its entry is left as
    /// it is until that `PaneUpdate` arrives. Taking the entries again now,
    /// while the last update still has the pane hidden, would drop it from
    /// the bar until then.
    fn restore(&mut self, pane: PaneId) {
        let geometry = self.minimized.remove(&pane);
        self.focus(pane);
        if let Some(geometry) = geometry {
            self.host
                .change_floating_panes_coordinates(vec![(pane, geometry)]);
        }
    }

    /// Hide `pane` from the screen and keep it on the bar as minimized, with
    /// where it stood, so that `restore` can put it back there.
    fn minimize(&mut self, pane: PaneId) {
        let Some(info) = self.pane_info(pane) else {
            return;
        };
        let geometry = geometry_of(info);
        self.minimized.insert(pane, geometry);
        self.host.hide_pane_with_id(pane);
    }

    /// Bring `pane` up: restore it if the bar minimized it, or focus it if
    /// the keyboard does not go to it already.
    ///
    /// Whether the bar minimized it is what the bar asked of Zellij, not what
    /// the last `PaneUpdate` says. A pane the bar has just hidden is still on
    /// screen in that update, and focusing it the plain way would show it
    /// placed as new and keep it listed as minimized.
    fn bring_up(&mut self, pane: PaneId) {
        if self.minimized.contains_key(&pane) {
            self.restore(pane);
        } else if !self
            .entries
            .iter()
            .any(|entry| entry.pane == pane && entry.focused)
        {
            self.focus(pane);
        }
    }

    /// Start renaming `pane`. Zellij renames the focused pane as the user
    /// types, so the pane is brought up first; see `bring_up`.
    fn rename_pane(&mut self, pane: PaneId) {
        if !self.lists(pane) {
            return;
        }
        self.bring_up(pane);
        let action = Action::StartRenamePaneByPaneId { pane_id: pane };
        self.host.run_action(action, BTreeMap::new());
    }

    /// Close `pane`, at once with `mouse_close_confirm` off. With it on, ask
    /// first in Zellij's prompt popup, naming the pane by its title, and
    /// close it only if the user confirms; see `answer_prompt`.
    ///
    /// A pane the bar no longer lists, gone since the line was drawn or the
    /// menu opened, is left alone either way.
    fn close_pane(&mut self, pane: PaneId) {
        let Some(entry) = self.entries.iter().find(|entry| entry.pane == pane) else {
            return;
        };
        if !self.mouse_close_confirm {
            self.close_now(pane);
            return;
        }
        let message = format!(
            "Close pane \"{}\" and end what runs in it?",
            show_controls(&entry.title)
        );
        // Cancel is the button selected at first, so `Enter` or `Space`
        // straight away keeps the pane, as `Esc` does. Without a default,
        // Zellij's prompt selects the first button, which is the one that
        // closes.
        let request = PromptRequest::confirm(message)
            .title("Close pane?")
            .yes("Close pane")
            .no("Cancel")
            .default("false");
        let request_id = self.host.prompt(request);
        self.pending_prompt
            .wait_for(request_id, Prompted::Close(pane));
    }

    fn close_now(&self, pane: PaneId) {
        match pane {
            PaneId::Terminal(id) => self.host.close_terminal_pane(id),
            PaneId::Plugin(id) => self.host.close_plugin_pane(id),
        }
    }

    /// Open the menu of what can be done to `pane`, at the mouse. See
    /// `PaneMenuItem::applying_to`.
    fn open_pane_menu(&mut self, pane: PaneId) {
        let Some(entry) = self.entries.iter().find(|entry| entry.pane == pane) else {
            return;
        };
        let items = PaneMenuItem::applying_to(entry.minimized, entry.focused);
        let labels = items.iter().map(|item| item.label().to_string());
        let request_id = self.host.prompt(labeled_menu_request(labels));
        self.pending_prompt
            .wait_for(request_id, Prompted::PaneMenu { pane, items });
    }

    /// Act on Zellij's answer `result` to the prompt `request_id`, when it is
    /// the pane menu, close confirmation or launcher menu the bar is waiting
    /// on, and for a pane's, the pane is still on the bar. An answer to any
    /// other prompt is left alone.
    ///
    /// A close confirmation closes the pane only when the user chose to. The
    /// pane menu acts on the item chosen, as the pane stands now, and the
    /// launcher's menu opens the item chosen, as the menu showed it. Any
    /// other answer, a dismissal or a timeout included, does nothing.
    pub(crate) fn answer_prompt(&mut self, request_id: u64, result: &PromptResult) {
        let Some(prompted) = self.pending_prompt.answer(request_id) else {
            return;
        };
        match prompted {
            Prompted::Close(pane) => {
                if *result == PromptResult::Confirmed(true) && self.lists(pane) {
                    self.close_now(pane);
                }
            }
            Prompted::PaneMenu { pane, items } => {
                let Some(item) = menu_choice(result).and_then(|index| items.get(index)) else {
                    return;
                };
                if !self.lists(pane) {
                    return;
                }
                match item {
                    PaneMenuItem::Restore | PaneMenuItem::Focus => self.bring_up(pane),
                    PaneMenuItem::Minimize => {
                        if !self.minimized.contains_key(&pane) {
                            self.minimize(pane);
                        }
                    }
                    PaneMenuItem::Rename => self.rename_pane(pane),
                    // Asks first with `mouse_close_confirm` on, as a middle
                    // click does: a menu item is as easy to pick by mistake.
                    PaneMenuItem::Close => self.close_pane(pane),
                }
            }
            Prompted::Launcher { items } => {
                if let Some(item) = menu_choice(result).and_then(|index| items.get(index)) {
                    self.launch(item);
                }
            }
        }
    }

    /// Focus the pane after the focused one for the wheel turned up, or
    /// before it for down, as zjtabs steps through tabs; `mouse_wheel_reversed`
    /// swaps the two. With none focused, up goes to the first and down to the
    /// last.
    ///
    /// Only the panes on screen are stepped through. Turning the wheel to
    /// look along the bar should not bring back panes the user put away.
    /// At the first or last, the wheel does nothing: wrapping around would
    /// make a fast scroll spin through every pane.
    fn step_focus(&self, direction: WheelDirection) {
        let shown: Vec<_> = self
            .entries
            .iter()
            .filter(|entry| !entry.minimized)
            .collect();
        let toward_next = match direction {
            WheelDirection::Up => !self.mouse_wheel_reversed,
            WheelDirection::Down => self.mouse_wheel_reversed,
        };
        let next = match shown.iter().position(|entry| entry.focused) {
            Some(index) if toward_next => shown.get(index + 1),
            Some(index) => index.checked_sub(1).and_then(|index| shown.get(index)),
            None if toward_next => shown.first(),
            None => shown.last(),
        };
        if let Some(entry) = next {
            self.focus(entry.pane);
        }
    }

    /// Whether `pane` is on the bar now.
    fn lists(&self, pane: PaneId) -> bool {
        self.entries.iter().any(|entry| entry.pane == pane)
    }

    /// What the last `PaneUpdate` says of `pane`.
    fn pane_info(&self, pane: PaneId) -> Option<&PaneInfo> {
        self.panes
            .panes
            .values()
            .flatten()
            .find(|info| pane_id_of(info) == pane)
    }
}
