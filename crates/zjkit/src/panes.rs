//! What a pane manifest says about where the keyboard goes and how wide
//! things are.

use std::collections::BTreeMap;
use zellij_tile::prelude::{PaneId, PaneInfo, PaneManifest, TabInfo};

/// The pane id Zellij events use to refer to a pane, which `PaneInfo` carries
/// split across two fields.
pub fn pane_id_of(pane: &PaneInfo) -> PaneId {
    if pane.is_plugin {
        PaneId::Plugin(pane.id)
    } else {
        PaneId::Terminal(pane.id)
    }
}

/// The focused pane of each layer of one tab.
///
/// Zellij tracks focus per layer, so a tab can have a focused tiled pane and a
/// focused floating pane at once. Which of the two actually has the keyboard
/// depends on whether the floating layer is up, and only `TabUpdate` reports
/// that, so both are kept here and `focused_pane_id` picks between them.
#[derive(Default, Clone, Debug, PartialEq)]
pub struct TabFocus {
    pub tiled: Option<PaneId>,
    pub floating: Option<PaneId>,
}

/// The focused tab, and whether its floating layer is up.
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct ActiveTab {
    pub position: usize,
    pub floating_panes_visible: bool,
}

impl ActiveTab {
    /// The active tab of a `TabUpdate`, if any tab is active.
    pub fn of(tabs: &[TabInfo]) -> Option<Self> {
        tabs.iter().find(|tab| tab.active).map(|tab| ActiveTab {
            position: tab.position,
            floating_panes_visible: tab.are_floating_panes_visible,
        })
    }
}

/// Which pane holds the focus in each tab of a pane manifest.
///
/// Suppressed panes are skipped: they keep whatever focus flag they had when
/// they were hidden, and a hidden pane is not where the keyboard goes.
pub fn focus_by_tab(manifest: &PaneManifest) -> BTreeMap<usize, TabFocus> {
    let mut focus_by_tab = BTreeMap::new();
    for (tab_position, panes) in &manifest.panes {
        let mut focus = TabFocus::default();
        for pane in panes
            .iter()
            .filter(|pane| pane.is_focused && !pane.is_suppressed)
        {
            if pane.is_floating {
                focus.floating = Some(pane_id_of(pane));
            } else {
                focus.tiled = Some(pane_id_of(pane));
            }
        }
        if focus != TabFocus::default() {
            focus_by_tab.insert(*tab_position, focus);
        }
    }
    focus_by_tab
}

/// The pane the keyboard currently goes to, or `None` while either half of
/// the picture (`PaneUpdate`, `TabUpdate`) is still missing.
pub fn focused_pane_id(
    active_tab: Option<ActiveTab>,
    focus_by_tab: &BTreeMap<usize, TabFocus>,
) -> Option<PaneId> {
    let active_tab = active_tab?;
    let focus = focus_by_tab.get(&active_tab.position)?;
    if active_tab.floating_panes_visible {
        focus.floating.or(focus.tiled)
    } else {
        focus.tiled
    }
}

/// The terminal's width, taken as the right edge of the widest visible pane.
///
/// Tiled panes tile the whole terminal, so the largest `pane_x + pane_columns`
/// among them is its width. Two kinds are skipped:
///
/// - **Floating** panes sit within the terminal but outside the tiling, so
///   their edge says nothing about its width.
/// - **Suppressed** panes are not on screen and stop tracking resizes. Their
///   geometry is whatever it was when they were hidden, and being stale it is
///   often the largest, which would pin the measurement to an old width and
///   silently stop a bar from ever being refitted.
pub fn terminal_width(manifest: &PaneManifest) -> Option<usize> {
    manifest
        .panes
        .values()
        .flatten()
        .filter(|pane| !pane.is_floating && !pane.is_suppressed)
        .map(|pane| pane.pane_x + pane.pane_columns)
        .max()
        .filter(|width| *width > 0)
}

/// The pane of plugin `plugin_id`, if it has one.
fn own_pane(manifest: &PaneManifest, plugin_id: Option<u32>) -> Option<&PaneInfo> {
    let plugin_id = plugin_id?;
    manifest
        .panes
        .values()
        .flatten()
        .find(|pane| pane.is_plugin && pane.id == plugin_id)
}

/// The URL the pane of plugin `plugin_id` reports, if it has a pane.
pub fn own_pane_url(manifest: &PaneManifest, plugin_id: Option<u32>) -> Option<String> {
    own_pane(manifest, plugin_id).and_then(|pane| pane.plugin_url.clone())
}

/// The content width of plugin `plugin_id`'s own pane, when it has one the
/// user can see. A collapsed pane, with no width, counts as having none.
pub fn own_visible_pane_width(manifest: &PaneManifest, plugin_id: Option<u32>) -> Option<usize> {
    own_pane(manifest, plugin_id)
        .filter(|pane| !pane.is_suppressed)
        .map(|pane| pane.pane_content_columns)
        .filter(|&columns| columns > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn pane(x: usize, columns: usize, floating: bool) -> PaneInfo {
        PaneInfo {
            pane_x: x,
            pane_columns: columns,
            is_floating: floating,
            ..Default::default()
        }
    }

    fn suppressed_pane(x: usize, columns: usize) -> PaneInfo {
        PaneInfo {
            pane_x: x,
            pane_columns: columns,
            is_suppressed: true,
            ..Default::default()
        }
    }

    fn manifest(panes: Vec<PaneInfo>) -> PaneManifest {
        let mut map = HashMap::new();
        map.insert(0, panes);
        PaneManifest { panes: map }
    }

    fn own_pane(id: u32, columns: usize, suppressed: bool) -> PaneInfo {
        PaneInfo {
            id,
            is_plugin: true,
            pane_content_columns: columns,
            is_suppressed: suppressed,
            ..Default::default()
        }
    }

    #[test]
    fn terminal_width_is_the_right_edge_of_the_widest_pane() {
        // Two panes side by side across an 80-column terminal.
        let panes = vec![pane(0, 40, false), pane(40, 40, false)];
        assert_eq!(terminal_width(&manifest(panes)), Some(80));
    }

    #[test]
    fn floating_panes_do_not_define_the_terminal_width() {
        let panes = vec![pane(0, 80, false), pane(10, 30, true)];
        assert_eq!(terminal_width(&manifest(panes)), Some(80));
    }

    #[test]
    fn suppressed_panes_do_not_define_the_terminal_width() {
        // Suppressed panes stop tracking resizes, so their stale geometry is
        // often the widest. Trusting it pins the width to an old value and the
        // hints are never refitted.
        let panes = vec![
            pane(0, 103, false),
            suppressed_pane(53, 106),
            suppressed_pane(53, 106),
        ];
        assert_eq!(terminal_width(&manifest(panes)), Some(103));
    }

    #[test]
    fn width_is_unknown_when_there_is_nothing_to_measure() {
        assert_eq!(terminal_width(&manifest(vec![])), None);
        assert_eq!(terminal_width(&manifest(vec![pane(0, 0, false)])), None);
    }

    #[test]
    fn only_a_visible_pane_of_its_own_sizes_the_bar() {
        let visible = manifest(vec![pane(0, 200, false), own_pane(7, 70, false)]);
        assert_eq!(own_visible_pane_width(&visible, Some(7)), Some(70));
        // A load_plugins bar runs in a suppressed pane, half the screen wide.
        let headless = manifest(vec![pane(0, 200, false), own_pane(7, 98, true)]);
        assert_eq!(own_visible_pane_width(&headless, Some(7)), None);
        let collapsed = manifest(vec![own_pane(7, 0, false)]);
        assert_eq!(own_visible_pane_width(&collapsed, Some(7)), None);
        assert_eq!(own_visible_pane_width(&visible, Some(8)), None);
        assert_eq!(own_visible_pane_width(&visible, None), None);
    }

    #[test]
    fn a_plugin_learns_its_alias_from_its_own_pane() {
        let own = PaneInfo {
            id: 7,
            is_plugin: true,
            plugin_url: Some("zjhints".to_string()),
            ..Default::default()
        };
        let other = PaneInfo {
            id: 8,
            is_plugin: true,
            plugin_url: Some("zellij:tab-bar".to_string()),
            ..Default::default()
        };
        let terminal = PaneInfo {
            id: 7,
            is_plugin: false,
            ..Default::default()
        };
        let manifest = manifest(vec![terminal, other, own]);
        assert_eq!(
            own_pane_url(&manifest, Some(7)),
            Some("zjhints".to_string())
        );
        assert_eq!(own_pane_url(&manifest, Some(9)), None);
        assert_eq!(own_pane_url(&manifest, None), None);
    }
}
