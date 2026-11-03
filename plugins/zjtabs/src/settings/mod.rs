//! zjtabs on the settings screen: its options, and the few things only the
//! bar itself can do for the screen. The screen is `zjkit::settings`.

pub mod options;

use crate::flags::FLAGS;
use crate::tabs::Tab;
use std::collections::BTreeMap;
use zellij_tile::prelude::ModeInfo;
use zjkit::settings::schema::Schema;

/// Everything the settings screen knows about zjtabs.
pub static SCHEMA: Schema = Schema {
    plugin: "zjtabs",
    groups: options::GROUPS,
    options: options::OPTIONS,
    families: options::FAMILIES,
    screen_options: options::SCREEN_OPTIONS,
    internal_options: &[],
    live_message: "zjtabs::configure",
    preview_group: Some("Theme"),
    permissions: || crate::host::bar_permissions(true, true),
    layer: crate::theme::layer,
    sample_values,
    draw_bar,
};

/// The widest the preview of the bar is drawn, in columns. The sample tabs
/// are more than fit in this, so the preview always shows the markers for
/// the tabs that did not, however wide the screen is.
const PREVIEW_COLUMNS: usize = 56;

/// The tabs the preview shows, as `(name, active, flags)`, the flags in the
/// order of `FLAGS`: fullscreen, sync, floating and the bell. The active tab
/// stands third with a flagged tab beside it, so both show whichever way the
/// bar is cut.
const SAMPLE_TABS: [(&str, bool, [bool; FLAGS.len()]); 8] = [
    ("shell", false, [false, false, false, false]),
    ("docs", false, [true, false, false, false]),
    ("editor", true, [false, false, false, false]),
    ("server", false, [false, false, false, true]),
    ("logs", false, [false, true, false, false]),
    ("mail", false, [false, false, false, false]),
    ("notes", false, [false, false, true, false]),
    ("build", false, [false, false, false, false]),
];

/// The placeholders the bar fills in a format string under `key`, with
/// the sample value each gets in the list.
fn sample_values(key: &str, config: &BTreeMap<String, String>) -> Vec<(&'static str, String)> {
    match key {
        "tab_format" | "active_tab_format" => vec![
            ("index", "2".to_string()),
            ("name", "editor".to_string()),
            ("flags", sample_flags(config)),
        ],
        "overflow_left_format" | "overflow_right_format" => vec![("count", "3".to_string())],
        _ => Vec::new(),
    }
}

/// What `{flags}` stands for in a sample tab: the bell flag as the bar would
/// draw it, as plain text so its styling cannot reach into the tab's own.
fn sample_flags(config: &BTreeMap<String, String>) -> String {
    let bell = FLAGS.last().expect("there is at least one flag");
    let format = config.get(bell.option).map_or(bell.default, String::as_str);
    zjkit::format::render_template(format, &[], config, 0.0)
        .iter()
        .map(|part| &**part)
        .collect()
}

/// The bar as `configuration` draws it `width` columns wide, in the mode and
/// Zellij theme of `mode_info`, showing `SAMPLE_TABS`.
fn draw_bar(
    mut configuration: BTreeMap<String, String>,
    mode_info: &ModeInfo,
    width: usize,
) -> String {
    // An option that hides the whole bar would leave nothing to preview.
    configuration.remove(zjkit::visibility::OPTION);
    // The screen may run in a nested session, or in one whose keyboard has
    // gone to a session it hosts, and the preview shows the bar as it looks
    // where it is neither hidden nor dimmed for that. Both options default
    // to true, so they are set to false rather than removed.
    for key in [
        crate::CONFIG_HIDE_WHEN_NESTED,
        crate::CONFIG_DIM_WHEN_UNFOCUSED,
    ] {
        configuration.insert(key.to_string(), "false".to_string());
    }
    // The preview is a bar in a pane of its own, however `auto_width` is set:
    // without a limit it would run past the end of the screen.
    configuration.insert(crate::CONFIG_AUTO_WIDTH.to_string(), "true".to_string());
    let width = width.min(PREVIEW_COLUMNS);
    let mut state = crate::State {
        bar: zjkit::bar::BarCore {
            zellij_palette: Some(mode_info.style.colors),
            pane_width: Some(width),
            ..Default::default()
        },
        ..Default::default()
    };
    state.apply_configuration(configuration);
    state.mode_info = mode_info.clone();
    state.tabs = SAMPLE_TABS
        .iter()
        .enumerate()
        .map(|(position, (name, active, flags))| Tab {
            position,
            tab_id: position,
            name: name.to_string(),
            active: *active,
            flags: *flags,
        })
        .collect();
    state.draw_bar().0
}

#[cfg(test)]
mod editor_tests;
#[cfg(test)]
mod view_tests;
