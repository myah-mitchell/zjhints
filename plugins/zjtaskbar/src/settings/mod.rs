//! zjtaskbar on the settings screen: its options, and the few things only
//! the bar itself can do for the screen. The screen is `zjkit::settings`.

pub mod options;

use std::collections::BTreeMap;
use zellij_tile::prelude::ModeInfo;
use zjkit::settings::schema::Schema;

/// Everything the settings screen knows about zjtaskbar.
pub static SCHEMA: Schema = Schema {
    plugin: "zjtaskbar",
    groups: options::GROUPS,
    options: options::OPTIONS,
    families: options::FAMILIES,
    screen_options: options::SCREEN_OPTIONS,
    deprecated_options: options::DEPRECATED_OPTIONS,
    internal_options: &[],
    live_message: "zjtaskbar::configure",
    preview_group: Some("Theme"),
    permissions: || crate::host::bar_permissions(&crate::host::Feature::ALL),
    layer: crate::theme::layer,
    sample_values,
    draw_bar,
};

/// The sample values the screen fills a format's placeholders with: the mode
/// prefix's, the only ones zjkit knows.
fn sample_values(key: &str, config: &BTreeMap<String, String>) -> Vec<(&'static str, String)> {
    zjkit::mode::sample_values(key, config).unwrap_or_default()
}

/// The preview of the bar on the screen, which is empty: zjtaskbar draws no
/// preview of itself.
fn draw_bar(
    _configuration: BTreeMap<String, String>,
    _mode_info: &ModeInfo,
    _width: usize,
) -> String {
    String::new()
}
