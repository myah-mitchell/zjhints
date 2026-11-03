//! zjtaskbar on the settings screen: its options, and the few things only
//! the bar itself can do for the screen. The screen is `zjkit::settings`.

pub mod options;

use crate::entries::Entry;
use crate::flags::FLAGS;
use std::collections::BTreeMap;
use zellij_tile::prelude::{ModeInfo, PaneId};
use zjkit::settings::schema::Schema;

/// Everything the settings screen knows about zjtaskbar.
pub static SCHEMA: Schema = Schema {
    plugin: "zjtaskbar",
    groups: options::GROUPS,
    options: options::OPTIONS,
    families: options::FAMILIES,
    screen_options: options::SCREEN_OPTIONS,
    internal_options: &[],
    live_message: "zjtaskbar::configure",
    preview_group: Some("Theme"),
    permissions: || crate::host::bar_permissions(&crate::host::Feature::ALL),
    layer: crate::theme::layer,
    sample_values,
    draw_bar,
};

/// The widest the preview of the bar is drawn, in columns. The sample
/// entries are more than fit in this, so the preview always shows the
/// markers for the entries that did not, however wide the screen is.
const PREVIEW_COLUMNS: usize = 56;

/// The entries the preview shows, as `(title, tab, focused, flags)`, the
/// flags in the order of `FLAGS`: minimized, exited, plugin and in another
/// tab. The active tab is `work`. The focused entry stands second with a
/// minimized one beside it, so both show whichever way the bar is cut.
const SAMPLE_ENTRIES: [(&str, &str, bool, [bool; FLAGS.len()]); 6] = [
    ("htop", "work", false, [false, false, false, false]),
    ("editor", "work", true, [false, false, false, false]),
    ("notes", "work", false, [true, false, false, false]),
    ("cargo build", "work", false, [false, true, false, false]),
    ("filepicker", "docs", false, [false, false, true, true]),
    ("man zellij", "docs", false, [false, false, false, true]),
];

/// The placeholders the bar fills in a format string under `key`, with
/// the sample value each gets in the list.
fn sample_values(key: &str, config: &BTreeMap<String, String>) -> Vec<(&'static str, String)> {
    match key {
        "entry_format" | "focused_entry_format" | "minimized_entry_format" => vec![
            ("title", "editor".to_string()),
            ("index", "2".to_string()),
            ("tab", "work".to_string()),
            ("flags", sample_flags(config)),
        ],
        "other_tab_format" => vec![("tab", "docs".to_string())],
        "overflow_left_format" | "overflow_right_format" => vec![("count", "3".to_string())],
        "launcher_format" => vec![(
            "symbol",
            config
                .get(crate::CONFIG_LAUNCHER_SYMBOL)
                .map_or(crate::DEFAULT_LAUNCHER_SYMBOL, String::as_str)
                .to_string(),
        )],
        _ => vec![],
    }
}

/// What `{flags}` stands for in a sample entry: the exited flag as the bar
/// would draw it, as plain text so its styling cannot reach into the
/// entry's own.
fn sample_flags(config: &BTreeMap<String, String>) -> String {
    let exited = FLAGS
        .iter()
        .find(|flag| flag.option == crate::CONFIG_EXITED_FORMAT)
        .expect("the exited flag is one of the flags");
    let format = config
        .get(exited.option)
        .map_or(exited.default, String::as_str);
    zjkit::format::render_template(format, &[], config, 0.0)
        .iter()
        .map(|part| &**part)
        .collect()
}

/// The bar as `configuration` draws it `width` columns wide, in the Zellij
/// theme of `mode_info`, showing `SAMPLE_ENTRIES`.
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
    state.entries = SAMPLE_ENTRIES
        .iter()
        .enumerate()
        .map(|(id, (title, tab, focused, flags))| {
            let [minimized, exited, plugin, other_tab] = *flags;
            Entry {
                pane: if plugin {
                    PaneId::Plugin(id as u32)
                } else {
                    PaneId::Terminal(id as u32)
                },
                tab_position: usize::from(other_tab),
                tab: tab.to_string(),
                title: title.to_string(),
                focused: *focused,
                minimized,
                exited,
                plugin,
                other_tab,
            }
        })
        .collect();
    state.draw_bar().0
}
