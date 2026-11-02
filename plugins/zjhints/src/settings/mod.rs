//! zjhints on the settings screen: its options, and the few things only the
//! bar itself can do for the screen. The screen is `zjkit::settings`.

pub mod options;

#[cfg(test)]
pub use zjkit::settings::{editor, view};
pub use zjkit::settings::{live, screen};

use std::collections::BTreeMap;
use zellij_tile::prelude::{BareKey, KeyWithModifier, ModeInfo};
use zjkit::settings::schema::Schema;

/// Everything the settings screen knows about zjhints.
pub static SCHEMA: Schema = Schema {
    plugin: "zjhints",
    groups: options::GROUPS,
    options: options::OPTIONS,
    families: options::FAMILIES,
    screen_options: options::SCREEN_OPTIONS,
    internal_options: crate::popup::INTERNAL_OPTIONS,
    live_message: "zjhints::configure",
    preview_group: Some("Theme"),
    permissions: || crate::bar_permissions(true),
    layer: crate::theme::layer,
    sample_values,
    draw_bar,
};

/// The placeholders the bar fills in a format string under `key`, with
/// the sample value each gets in the list.
fn sample_values(key: &str, config: &BTreeMap<String, String>) -> Vec<(&'static str, String)> {
    if key == "key_format" || key.starts_with("key_format_") {
        // The keys as the bar composes them, so key and modifier aliases
        // show too.
        let sample = KeyWithModifier::new(BareKey::Char('p')).with_ctrl_modifier();
        vec![("key", crate::compose_key_text(&[sample], config))]
    } else if key == "desc_format" || key.starts_with("desc_format_") {
        vec![("desc", "Pane".to_string())]
    } else if key == "mode_format" {
        vec![("mode", "NORMAL".to_string())]
    } else if let Some(mode) = key.strip_prefix("mode_format_") {
        // The bar names a mode by its variant, which is the suffix without
        // underscores: enter_search is ENTERSEARCH.
        vec![("mode", mode.replace('_', "").to_uppercase())]
    } else {
        Vec::new()
    }
}

/// The bar as `configuration` draws it `width` columns wide, in the mode,
/// keybindings and Zellij theme of `mode_info`.
fn draw_bar(
    mut configuration: BTreeMap<String, String>,
    mode_info: &ModeInfo,
    width: usize,
) -> String {
    // Options that hide the whole bar would leave nothing to preview.
    for key in ["hidden", "hide_in_base_mode", "hide_when_nested"] {
        configuration.remove(key);
    }
    let mut state = crate::State {
        zellij_palette: Some(mode_info.style.colors),
        pane_width: Some(width),
        ..Default::default()
    };
    state.apply_configuration(configuration);
    state.mode_info = mode_info.clone();
    state.draw_bar().0
}

#[cfg(test)]
mod editor_tests;
#[cfg(test)]
mod view_tests;
