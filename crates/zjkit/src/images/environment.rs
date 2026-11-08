//! The Zellij the images are drawn in: its theme and its keybindings, fixed so
//! that what a plugin draws is the same on every machine.

use std::sync::OnceLock;
use zellij_tile::prelude::{InputMode, KeybindsVec, ModeInfo, Styling};
use zellij_utils::input::config::Config;
use zellij_utils::setup::get_default_themes;

/// The Zellij theme the images draw in. The `zellij` theme follows the active
/// Zellij theme, so an image of it needs one fixed.
pub fn palette() -> Styling {
    // Reading every theme Zellij ships takes long enough to add up.
    static PALETTE: OnceLock<Styling> = OnceLock::new();
    *PALETTE.get_or_init(|| {
        get_default_themes()
            .get_theme("catppuccin-mocha")
            .expect("Zellij ships a catppuccin-mocha theme")
            .palette
    })
}

/// Zellij's default keybindings. The table is a hash map underneath, so it is
/// sorted to keep what is drawn from one run to the next the same.
pub fn default_keybinds() -> KeybindsVec {
    // Parsing the default config takes long enough to add up over the images.
    static KEYBINDS: OnceLock<KeybindsVec> = OnceLock::new();
    KEYBINDS.get_or_init(sorted_default_keybinds).clone()
}

fn sorted_default_keybinds() -> KeybindsVec {
    let config = Config::from_default_assets().expect("Zellij ships a default config");
    let mut keybinds = config.keybinds.to_keybinds_vec();
    keybinds.sort_by_key(|(mode, _)| format!("{mode:?}"));
    for (_, bindings) in &mut keybinds {
        bindings.sort_by(|(left, _), (right, _)| left.cmp(right));
    }
    keybinds
}

/// What Zellij tells a plugin about `mode`: the default keybindings, in the
/// Zellij theme the images are drawn in.
pub fn mode_info(mode: InputMode) -> ModeInfo {
    ModeInfo {
        mode,
        base_mode: Some(InputMode::Normal),
        keybinds: default_keybinds(),
        style: zellij_tile::prelude::Style {
            colors: palette(),
            ..Default::default()
        },
        ..Default::default()
    }
}
