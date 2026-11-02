//! zjtabs on the settings screen: its options, and the few things only the
//! bar itself can do for the screen. The screen is `zjkit::settings`.

use std::collections::BTreeMap;
use zellij_tile::prelude::ModeInfo;
use zjkit::settings::schema::Schema;

/// Everything the settings screen knows about zjtabs.
pub static SCHEMA: Schema = Schema {
    plugin: "zjtabs",
    groups: &["General"],
    options: &[],
    families: &[],
    screen_options: &["settings", "alias"],
    internal_options: &[],
    live_message: "zjtabs::configure",
    preview_group: None,
    permissions: || crate::host::bar_permissions(true),
    layer: crate::theme::layer,
    sample_values,
    draw_bar,
};

/// The placeholders the bar fills in a format string under `key`, with the
/// sample value each gets in the list.
fn sample_values(_key: &str, _config: &BTreeMap<String, String>) -> Vec<(&'static str, String)> {
    Vec::new()
}

/// The bar as `configuration` draws it `width` columns wide, in the mode and
/// Zellij theme of `mode_info`.
fn draw_bar(
    _configuration: BTreeMap<String, String>,
    _mode_info: &ModeInfo,
    _width: usize,
) -> String {
    String::new()
}
