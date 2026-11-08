//! Buttons on the frames of the session's panes, as a desktop window has:
//! minimize, maximize and close, at the right of the frame's top row.
//!
//! Each tab's panes are framed by one bar of the instance: the bar in that
//! tab, or for a tab with no bar, the instance's first bar, the one with the
//! lowest plugin id. See `owned_tabs`.
//!
//! Zellij keeps each bar's buttons on a pane apart and shows them side by
//! side, so a pane carries one bar's buttons at a time. The bar whose
//! buttons are on a pane holds it, as `zjkit::shared::frame_owner` tells,
//! the same record that sends a click on them to that bar. A bar puts its
//! buttons on a pane only when no bar holds it or it does. One that should
//! no longer frame a pane, as when the pane moved to another tab, takes its
//! buttons off, and the pane is free once they are off. Only then does the
//! bar that frames it now put its own on, which it tries again each time it
//! draws.
//!
//! Zellij changes a bar's buttons only while it draws that bar, and draws a
//! bar only while it is on screen, so what a bar places or takes off waits
//! for its next drawing (see `zjkit::shared`). A pane pulled out of a tab
//! not on screen so keeps that tab's bar's buttons until the tab is shown
//! again. They still work meanwhile, as the click goes to the bar that
//! placed them, and only the maximize button's text can be out of date.
//!
//! A pane minimized from any bar keeps its buttons, as its frame is not
//! drawn until it is restored. Every bar lists it, so any of them can bring
//! it back; see `instance`.
//!
//! Minimizing a tiled pane floats it first, so that it comes back as a
//! floating pane, as every pane on the bar is. Zellij leaves the last tiled
//! pane of a tab tiled, so that pane has no minimize button.
//!
//! The buttons are sent only when they change, as a frame keeps them until
//! the pane closes or the bar unloads, when Zellij takes them off itself.
//! Placing them needs `ChangeApplicationState`, so none are sent before it
//! is granted.

use crate::entries::{is_minimized, Minimized, PaneScope};
use crate::State;
use std::collections::{BTreeMap, BTreeSet};
use zellij_tile::prelude::*;
use zjkit::host::Host;
use zjkit::panes::pane_id_of;
use zjkit::shared::frame_owner;

/// One button of a pane's frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameButton {
    Minimize,
    /// Fills the tab with the pane, or gives it back its own size while it
    /// fills the tab.
    Maximize,
    Close,
}

impl FrameButton {
    /// Every button, in the order the frame shows them.
    const ALL: [FrameButton; 3] = [
        FrameButton::Minimize,
        FrameButton::Maximize,
        FrameButton::Close,
    ];

    /// The id a click on the button comes back with.
    fn id(self) -> &'static str {
        match self {
            FrameButton::Minimize => "minimize",
            FrameButton::Maximize => "maximize",
            FrameButton::Close => "close",
        }
    }

    /// The button a click's `id` names, if any.
    pub(crate) fn of_id(id: &str) -> Option<FrameButton> {
        FrameButton::ALL
            .into_iter()
            .find(|button| button.id() == id)
    }
}

/// What each button shows. An empty text leaves that button off.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FrameButtonTexts {
    pub minimize: String,
    pub maximize: String,
    /// The maximize button's text while the pane fills its tab.
    pub restore: String,
    pub close: String,
}

impl FrameButtonTexts {
    /// The buttons of a pane, as frame items. See `Framed`.
    fn items(&self, framed: Framed) -> Vec<FrameItem> {
        let fullscreen = framed.fullscreen;
        FrameButton::ALL
            .into_iter()
            .filter_map(|button| {
                let text = match button {
                    FrameButton::Minimize if !framed.can_minimize => return None,
                    FrameButton::Minimize => &self.minimize,
                    FrameButton::Maximize if fullscreen => &self.restore,
                    FrameButton::Maximize => &self.maximize,
                    FrameButton::Close => &self.close,
                };
                (!text.is_empty()).then(|| FrameItem::new(button.id(), text.as_str()).clickable())
            })
            .collect()
    }
}

/// What decides the buttons on one pane's frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Framed {
    /// The pane fills its tab, so its maximize button gives it back its own
    /// size.
    pub fullscreen: bool,
    /// The pane has a minimize button. See `framed_panes`.
    pub can_minimize: bool,
}

/// The positions of the tabs of `manifest` whose panes the bar `own_id`
/// frames, each with whether it may minimize panes there. See the module's
/// docs. Empty while the bar's own pane is not on screen.
///
/// The bars are the plugin panes on screen whose ids are in `bars`, the
/// instance's bars, and the bar's own pane. A settings screen of the same
/// plugin is not one. In a tab with no bar, the first bar minimizes panes
/// only while it lists every tab, as it would otherwise not list the pane
/// it minimized while that tab is shown.
pub(crate) fn owned_tabs(
    manifest: &PaneManifest,
    own_id: u32,
    bars: &BTreeSet<u32>,
    pane_scope: PaneScope,
) -> BTreeMap<usize, bool> {
    let is_bar = |pane: &PaneInfo| {
        pane.is_plugin && !pane.is_suppressed && (pane.id == own_id || bars.contains(&pane.id))
    };
    let tab_bars: BTreeMap<usize, u32> = manifest
        .panes
        .iter()
        .filter_map(|(position, panes)| {
            let first = panes
                .iter()
                .filter(|pane| is_bar(pane))
                .map(|pane| pane.id)
                .min()?;
            Some((*position, first))
        })
        .collect();
    if !tab_bars.values().any(|id| *id == own_id) {
        return BTreeMap::new();
    }
    let first_bar = tab_bars.values().min().copied();
    manifest
        .panes
        .keys()
        .filter_map(|position| match tab_bars.get(position) {
            Some(id) => (*id == own_id).then_some((*position, true)),
            None => {
                (first_bar == Some(own_id)).then_some((*position, pane_scope == PaneScope::All))
            }
        })
        .collect()
}

/// The panes of `manifest` whose frames the bar puts buttons on: those that
/// take focus and are on screen in the tabs of `owned` (see `owned_tabs`),
/// and the minimized panes there. The instance's bars, by their ids in
/// `bars`, have none.
///
/// A pane has no minimize button where `owned` says the bar may not
/// minimize, or when it is the only tiled pane of its tab that takes focus,
/// which Zellij will not float.
pub(crate) fn framed_panes(
    manifest: &PaneManifest,
    minimized: &Minimized,
    owned: &BTreeMap<usize, bool>,
    bars: &BTreeSet<u32>,
) -> BTreeMap<PaneId, Framed> {
    let mut framed = BTreeMap::new();
    for (position, may_minimize) in owned {
        let Some(panes) = manifest.panes.get(position) else {
            continue;
        };
        let on_screen = |pane: &&PaneInfo| pane.is_selectable && !pane.is_suppressed;
        let tiled = panes
            .iter()
            .filter(on_screen)
            .filter(|pane| !pane.is_floating)
            .count();
        for pane in panes {
            let is_bar = pane.is_plugin && bars.contains(&pane.id);
            if is_bar || !(on_screen(&pane) || is_minimized(pane, minimized)) {
                continue;
            }
            // A minimized pane keeps the buttons it had, as its frame is not
            // drawn until it is restored, floating.
            let can_minimize =
                *may_minimize && (pane.is_floating || is_minimized(pane, minimized) || tiled > 1);
            framed.insert(
                pane_id_of(pane),
                Framed {
                    fullscreen: pane.is_fullscreen,
                    can_minimize,
                },
            );
        }
    }
    framed
}

impl State {
    /// Put the buttons on the frames that should have them, take them off
    /// the ones that should not, and leave the rest alone. See the module's
    /// docs.
    pub(crate) fn sync_frame_buttons(&mut self) {
        self.place_frame_buttons(true);
    }

    /// Put the buttons on the frames that should have them and have none
    /// yet, as the pane was held by another bar the last time, and take
    /// none off. Called as the bar draws, when Zellij places them at once.
    ///
    /// None are taken off here: a pane the bar has just restored is still
    /// hidden and no longer minimized until Zellij reports it shown, and
    /// would lose its buttons in between.
    pub(crate) fn claim_frame_buttons(&mut self) {
        self.place_frame_buttons(false);
    }

    /// `sync_frame_buttons`, which takes buttons off only when `release`.
    fn place_frame_buttons(&mut self, release: bool) {
        let granted = self
            .bar
            .granted_permissions
            .contains(&PermissionType::ChangeApplicationState);
        if !granted {
            return;
        }
        let bars = self.bar_ids();
        let owned = match self.bar.plugin_id {
            Some(id) => owned_tabs(&self.panes, id, &bars, self.pane_scope),
            None => BTreeMap::new(),
        };
        let wanted: BTreeMap<PaneId, Vec<FrameItem>> = match &self.frame_buttons {
            Some(texts) => framed_panes(&self.panes, &self.minimized(), &owned, &bars)
                .into_iter()
                .map(|(pane, framed)| (pane, texts.items(framed)))
                .filter(|(_, items)| !items.is_empty())
                .collect(),
            None => BTreeMap::new(),
        };
        if release {
            let unwanted: Vec<PaneId> = self
                .frame_items_sent
                .keys()
                .filter(|pane| !wanted.contains_key(pane))
                .copied()
                .collect();
            for pane in unwanted {
                self.frame_items_sent.remove(&pane);
                self.host.set_pane_frame_items(pane, vec![]);
            }
        }
        for (pane, items) in wanted {
            if self.frame_items_sent.get(&pane) == Some(&items) || !self.may_frame(pane) {
                continue;
            }
            self.host.set_pane_frame_items(pane, items.clone());
            self.frame_items_sent.insert(pane, items);
        }
    }

    /// Whether the bar may put its buttons on `pane`: no other bar of the
    /// instance holds it. See the module's docs.
    fn may_frame(&self, pane: PaneId) -> bool {
        frame_owner(pane).is_none_or(|owner| Some(owner) == self.bar.plugin_id)
    }

    /// Whether the buttons on `pane` are the bar's: it placed them and has
    /// not taken them off, or it has asked to and Zellij has not done so
    /// yet, so they are still on the frame.
    fn holds_frame(&self, pane: PaneId) -> bool {
        self.frame_items_sent.contains_key(&pane)
            || frame_owner(pane).is_some_and(|owner| Some(owner) == self.bar.plugin_id)
    }

    /// A click on one of the bar's frame buttons, `item_id`, on `pane`.
    /// Only a left click acts. Minimize and close act as the bar's own
    /// mouse does, close asking first while `mouse_close_confirm` is on for
    /// this bar or any other bar with frame buttons, and minimize floating
    /// a tiled pane first.
    pub(crate) fn frame_button_clicked(&mut self, click: &FrameItemClick) {
        if click.button != MouseButton::Left {
            return;
        }
        let Some(button) = FrameButton::of_id(&click.item_id) else {
            return;
        };
        if !self.holds_frame(click.pane_id) {
            return;
        }
        match button {
            FrameButton::Minimize => {
                if !self.minimized().contains_key(&click.pane_id) {
                    self.minimize(click.pane_id);
                }
            }
            FrameButton::Maximize => self.host.toggle_pane_id_fullscreen(click.pane_id),
            // The buttons may be left from a bar whose tab the pane has
            // moved out of, into one whose bar asks before closing, so the
            // button asks if any bar of the instance would.
            FrameButton::Close => {
                let confirm =
                    self.mouse_close_confirm || !self.instance.get().confirm_frame_close.is_empty();
                self.close_pane_asking(click.pane_id, confirm);
            }
        }
    }
}
