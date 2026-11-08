//! Which panes the bar lists, in what order, and what it shows of each.
//!
//! The bar lists a session's floating panes, terminals and plugins alike,
//! one entry each. Tiled panes are left out, and so are panes that cannot be
//! selected and the instance's bars, this one among them, wherever they
//! stand.
//!
//! A pane minimized from a bar stays listed, marked as minimized. Zellij
//! reports a hidden floating pane as suppressed and no longer floating, the
//! same as a hidden tiled pane, so the bar cannot tell from the manifest
//! which hidden panes were floating. The bars keep the ids of the panes they
//! minimized themselves instead, and list those. The instance's bars share
//! them (see `instance`), so a pane minimized from one bar is listed by every
//! bar whose `pane_scope` takes in its tab, and any of them brings it back.
//! With each id is kept where the pane stood, which Zellij forgets as it
//! hides a pane.
//!
//! The entries are grouped by tab, in the order the tabs stand, and within a
//! tab ordered by pane id, terminals before plugins. A pane's id never
//! changes, so the entries stay where they are as the focus moves between
//! them.

use std::collections::{BTreeMap, BTreeSet};
use zellij_tile::prelude::{FloatingPaneCoordinates, PaneId, PaneInfo, PaneManifest, TabInfo};

/// The panes the bars minimized themselves, each with where it stood on
/// screen before it was hidden, so restoring it puts it back there. One set
/// serves every bar of the instance; see `instance`.
///
/// A pane is kept from the moment a bar asks Zellij to hide it until a
/// `PaneUpdate` shows it is no longer minimized. See `forget_gone` for when
/// that is.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Minimized {
    panes: BTreeMap<PaneId, MinimizedPane>,
}

/// One pane the bar minimized.
#[derive(Clone, Debug, PartialEq)]
struct MinimizedPane {
    /// Where the pane stood on screen before the bar hid it.
    geometry: FloatingPaneCoordinates,
    /// Whether a `PaneUpdate` has shown the pane hidden since the bar hid
    /// it, which says the hide took effect. Until then, an update showing
    /// the pane on screen may be one Zellij sent before the hide.
    seen_hidden: bool,
}

impl Minimized {
    /// Keep `pane` as minimized, standing at `geometry` before it was
    /// hidden. Called as the bar asks Zellij to hide it.
    #[cfg(test)]
    pub(crate) fn insert(&mut self, pane: PaneId, geometry: FloatingPaneCoordinates) {
        let minimized = MinimizedPane {
            geometry,
            seen_hidden: false,
        };
        self.panes.insert(pane, minimized);
    }

    /// Whether the bar minimized `pane` and has not forgotten it since.
    pub(crate) fn contains_key(&self, pane: &PaneId) -> bool {
        self.panes.contains_key(pane)
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.panes.is_empty()
    }
}
use zjkit::panes::{focus_by_tab, pane_id_of, ActiveTab};

/// The values `pane_scope` takes.
pub(crate) const PANE_SCOPE_CHOICES: [&str; 2] = ["all", "tab"];

/// Which tabs' panes the bar lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PaneScope {
    /// The floating panes of every tab.
    #[default]
    All,
    /// Only the floating panes of the active tab.
    Tab,
}

impl PaneScope {
    /// The scope `value` names, in any case. Anything else is `All`, the
    /// default.
    pub(crate) fn of(value: &str) -> Self {
        if value.eq_ignore_ascii_case("tab") {
            PaneScope::Tab
        } else {
            PaneScope::All
        }
    }
}

/// One tab, as the entries need it: what `TabUpdate` says about it, less
/// what the bar never shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Tab {
    /// The tab's `TabInfo::tab_id`, which stays the same as tabs move.
    pub id: usize,
    /// Where the tab stands among the others, counted from 0. A pane
    /// manifest lists its panes under this.
    pub position: usize,
    pub name: String,
    pub active: bool,
    /// Whether the tab shows its floating panes over its tiled ones.
    pub floating_panes_visible: bool,
}

/// Every tab of a `TabUpdate`, in the order they stand.
pub(crate) fn tabs_of(infos: &[TabInfo]) -> Vec<Tab> {
    let mut tabs: Vec<Tab> = infos
        .iter()
        .map(|info| Tab {
            id: info.tab_id,
            position: info.position,
            name: info.name.clone(),
            active: info.active,
            floating_panes_visible: info.are_floating_panes_visible,
        })
        .collect();
    tabs.sort_by_key(|tab| tab.position);
    tabs
}

/// One pane, as the bar draws it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Entry {
    pub pane: PaneId,
    /// The position of the tab the pane is in.
    pub tab_position: usize,
    /// The name of the tab the pane is in, for `{tab}`: its name, or its
    /// number counted from 1 while it has none or its name is not known yet.
    pub tab: String,
    /// The pane's title as Zellij reports it, which for a terminal is
    /// usually the running command or the working directory.
    pub title: String,
    /// The keyboard goes to the pane: it is the focused floating pane of the
    /// active tab, and that tab shows its floating panes.
    pub focused: bool,
    /// Hidden from the screen by the bar, and listed so it can be brought
    /// back.
    pub minimized: bool,
    /// The pane's command has finished, and Zellij holds the pane open.
    pub exited: bool,
    /// The pane runs a plugin rather than a terminal.
    pub plugin: bool,
    /// The pane is in a tab other than the active one.
    pub other_tab: bool,
}

impl Entry {
    /// Which indicators of `{flags}` the entry shows, in the order of
    /// `flags::FLAGS`.
    pub(crate) fn flags(&self) -> [bool; crate::flags::FLAGS.len()] {
        [self.minimized, self.exited, self.plugin, self.other_tab]
    }
}

/// Every pane of `manifest` the bar lists, in the order it draws them.
///
/// `tabs` is the last `TabUpdate`, which says which tab is active and names
/// them. `minimized` holds the panes the bars minimized themselves, and
/// `bars` the plugin ids of the instance's bars, whose panes are never
/// listed. With `PaneScope::Tab` and no active tab known yet, nothing is
/// listed.
pub(crate) fn entries_of(
    manifest: &PaneManifest,
    tabs: &[Tab],
    minimized: &Minimized,
    scope: PaneScope,
    bars: &BTreeSet<u32>,
) -> Vec<Entry> {
    let active = tabs.iter().find(|tab| tab.active).map(|tab| ActiveTab {
        position: tab.position,
        floating_panes_visible: tab.floating_panes_visible,
    });
    // The floating pane the keyboard goes to, if it goes to one.
    let focused = active
        .filter(|active| active.floating_panes_visible)
        .and_then(|active| focus_by_tab(manifest).remove(&active.position)?.floating);
    let mut entries = vec![];
    for (tab_position, panes) in &manifest.panes {
        let in_scope = match scope {
            PaneScope::All => true,
            PaneScope::Tab => active.is_some_and(|active| active.position == *tab_position),
        };
        if !in_scope {
            continue;
        }
        let tab = tabs
            .iter()
            .find(|tab| tab.position == *tab_position)
            .map(|tab| tab.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| (tab_position + 1).to_string());
        for pane in panes {
            let pane_id = pane_id_of(pane);
            let minimized = is_minimized(pane, minimized);
            let is_bar = pane.is_plugin && bars.contains(&pane.id);
            if is_bar || !(is_listed_floating(pane) || minimized) {
                continue;
            }
            entries.push(Entry {
                pane: pane_id,
                tab_position: *tab_position,
                tab: tab.clone(),
                title: pane.title.clone(),
                focused: focused == Some(pane_id),
                minimized,
                exited: pane.exited,
                plugin: pane.is_plugin,
                other_tab: active.is_some_and(|active| active.position != *tab_position),
            });
        }
    }
    entries.sort_by_key(|entry| (entry.tab_position, entry.pane));
    entries
}

/// Whether `pane` is a floating pane on screen that the user can select.
pub(crate) fn is_listed_floating(pane: &PaneInfo) -> bool {
    pane.is_floating && pane.is_selectable && !pane.is_suppressed
}

/// Whether `pane` is one the bar minimized and is still hidden. A pane the
/// bar minimized that is back on screen, shown some other way, is listed as
/// any other floating pane.
pub(crate) fn is_minimized(pane: &PaneInfo, minimized: &Minimized) -> bool {
    pane.is_suppressed && minimized.contains_key(&pane_id_of(pane))
}

/// Forget the minimized panes `manifest` shows are no longer minimized:
/// closed, back on screen as tiled panes, which the bar does not list, or
/// back on screen as floating panes after a hide that took effect, shown
/// again some other way than through the bar.
///
/// A pane back on screen as a floating pane is kept while no update has
/// shown it hidden yet. The bar adds a pane here as it hides it, and a pane
/// update Zellij sent before the hide took effect still shows the pane on
/// screen; forgetting it then would lose the pane from the bar once the
/// hide arrives.
pub(crate) fn forget_gone(minimized: &mut Minimized, manifest: &PaneManifest) {
    minimized.panes.retain(|pane_id, minimized_pane| {
        let Some(pane) = manifest
            .panes
            .values()
            .flatten()
            .find(|pane| pane_id_of(pane) == *pane_id)
        else {
            return false;
        };
        if pane.is_suppressed {
            minimized_pane.seen_hidden = true;
            return true;
        }
        pane.is_floating && !minimized_pane.seen_hidden
    });
}
