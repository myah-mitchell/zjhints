//! Buttons on the frames of the session's panes, as a desktop window has:
//! minimize, maximize and close, at the right of the frame's top row.
//!
//! Zellij shows each plugin's buttons side by side, so every tab's panes
//! take their buttons from one bar only: the bar in that tab, or for a tab
//! with no bar, the session's first bar, the one with the lowest plugin id.
//! A bar takes its buttons off a pane that moves to a tab it does not
//! frame. Minimizing a pane from its frame so keeps it on the bar that
//! framed it, which is the one that can restore it.
//!
//! Minimizing a tiled pane floats it first, so that it comes back as a
//! floating pane, as every pane on the bar is. Zellij leaves the last tiled
//! pane of a tab tiled, so that pane has no minimize button.
//!
//! The buttons are sent only when they change, as a frame keeps them until
//! the pane closes or the bar unloads. Placing them needs
//! `ChangeApplicationState`, so none are sent before it is granted.

use crate::entries::{is_minimized, Minimized, PaneScope};
use crate::State;
use std::collections::BTreeMap;
use zellij_tile::prelude::*;
use zjkit::host::Host;
use zjkit::panes::pane_id_of;

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

/// The positions of the tabs of `manifest` whose panes the bar with
/// `own_pane` frames, each with whether it may minimize panes there. See
/// the module's docs. Empty while the bar's own pane is not on screen.
///
/// The bars are the plugin panes on screen running the bar's own URL that
/// take no focus, as a bar does once it has its permissions, and its own
/// pane. A floating settings screen of the same plugin is not one. In a
/// tab with no bar, the first bar minimizes panes only while it lists every
/// tab, as it would otherwise not list the pane it minimized.
pub(crate) fn owned_tabs(
    manifest: &PaneManifest,
    own_pane: PaneId,
    own_url: Option<&str>,
    pane_scope: PaneScope,
) -> BTreeMap<usize, bool> {
    let is_bar = |pane: &PaneInfo| {
        pane.is_plugin
            && !pane.is_suppressed
            && (pane_id_of(pane) == own_pane
                || (!pane.is_floating
                    && !pane.is_selectable
                    && own_url.is_some()
                    && pane.plugin_url.as_deref() == own_url))
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
    let PaneId::Plugin(own_id) = own_pane else {
        return BTreeMap::new();
    };
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
/// and the panes there the bar minimized. Tiled panes running `own_url`,
/// which are bars, have none.
///
/// A pane has no minimize button where `owned` says the bar may not
/// minimize, or when it is the only tiled pane of its tab that takes focus,
/// which Zellij will not float.
pub(crate) fn framed_panes(
    manifest: &PaneManifest,
    minimized: &Minimized,
    owned: &BTreeMap<usize, bool>,
    own_url: Option<&str>,
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
            let is_bar =
                !pane.is_floating && own_url.is_some() && pane.plugin_url.as_deref() == own_url;
            if is_bar || !(on_screen(&pane) || is_minimized(pane, minimized)) {
                continue;
            }
            // A pane the bar minimized keeps the buttons it had, as its frame
            // is not drawn until it is restored, floating.
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
        let granted = self
            .bar
            .granted_permissions
            .contains(&PermissionType::ChangeApplicationState);
        if !granted {
            return;
        }
        let own_url = self.bar.own_url.as_deref();
        let owned = match self.bar.plugin_id {
            Some(id) => owned_tabs(&self.panes, PaneId::Plugin(id), own_url, self.pane_scope),
            None => BTreeMap::new(),
        };
        let wanted: BTreeMap<PaneId, Vec<FrameItem>> = match &self.frame_buttons {
            Some(texts) => framed_panes(&self.panes, &self.minimized, &owned, own_url)
                .into_iter()
                .map(|(pane, framed)| (pane, texts.items(framed)))
                .filter(|(_, items)| !items.is_empty())
                .collect(),
            None => BTreeMap::new(),
        };
        for pane in self.frame_items_sent.keys() {
            if !wanted.contains_key(pane) {
                self.host.set_pane_frame_items(*pane, vec![]);
            }
        }
        for (pane, items) in &wanted {
            if self.frame_items_sent.get(pane) != Some(items) {
                self.host.set_pane_frame_items(*pane, items.clone());
            }
        }
        self.frame_items_sent = wanted;
    }

    /// A click on one of the bar's frame buttons, `item_id`, on `pane`.
    /// Only a left click acts. Minimize and close act as the bar's own
    /// mouse does, close asking first while `mouse_close_confirm` is on,
    /// and minimize floating a tiled pane first.
    pub(crate) fn frame_button_clicked(&mut self, click: &FrameItemClick) {
        if click.button != MouseButton::Left {
            return;
        }
        let Some(button) = FrameButton::of_id(&click.item_id) else {
            return;
        };
        if !self.frame_items_sent.contains_key(&click.pane_id) {
            return;
        }
        match button {
            FrameButton::Minimize => {
                if !self.minimized.contains_key(&click.pane_id) {
                    self.minimize(click.pane_id);
                }
            }
            FrameButton::Maximize => self.host.toggle_pane_id_fullscreen(click.pane_id),
            FrameButton::Close => self.close_pane(click.pane_id),
        }
    }
}
