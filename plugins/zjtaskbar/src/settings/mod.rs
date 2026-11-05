//! zjtaskbar on the settings screen: its options, and the few things only
//! the bar itself can do for the screen. The screen is `zjkit::settings`.

pub mod options;

use crate::entries::Tab;
use crate::flags::FLAGS;
use std::collections::{BTreeMap, HashMap};
use zellij_tile::prelude::{ModeInfo, PaneInfo, PaneManifest};
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

/// The names of the tabs the preview's sample panes are in, by position.
/// The first is the active one, and shows its floating panes, so one of its
/// panes has the focus. The other is named so `{tab}` and the other-tab flag
/// show a name rather than a number.
const SAMPLE_TABS: [&str; 2] = ["work", "docs"];

/// A floating pane the preview lists.
struct SamplePane {
    title: &'static str,
    /// The position in `SAMPLE_TABS` of the tab the pane is in.
    tab: usize,
    /// The pane has the keyboard. Only one of the active tab's does.
    focused: bool,
    /// The bar minimized the pane, so it is hidden from the screen.
    minimized: bool,
    /// The pane's command has finished.
    exited: bool,
    /// The pane runs a plugin rather than a terminal.
    plugin: bool,
}

/// A terminal pane in `tab`, titled `title`, in no particular state.
const fn sample_pane(title: &'static str, tab: usize) -> SamplePane {
    SamplePane {
        title,
        tab,
        focused: false,
        minimized: false,
        exited: false,
        plugin: false,
    }
}

/// The panes the preview shows, in the order the bar lists them: by tab,
/// then terminals before plugins. Between them they raise every flag. The
/// focused entry stands second, so it shows however the bar is cut, with
/// the minimized one next to it, the first to show as there is room. They
/// are more than fit in `PREVIEW_COLUMNS`, so a marker always shows too.
const SAMPLE_PANES: [SamplePane; 6] = [
    sample_pane("htop", 0),
    SamplePane {
        focused: true,
        ..sample_pane("editor", 0)
    },
    SamplePane {
        minimized: true,
        ..sample_pane("notes", 0)
    },
    SamplePane {
        exited: true,
        ..sample_pane("cargo build", 0)
    },
    sample_pane("man zellij", 1),
    SamplePane {
        plugin: true,
        ..sample_pane("filepicker", 1)
    },
];

/// The session's name in the samples of `session_format` in the list, and in
/// the preview while the screen has not been told its own session's name.
/// The same as zjtabs'.
const SAMPLE_SESSION_NAME: &str = "work";

/// The placeholders the bar fills in a format string under `key`, with
/// the sample value each gets in the list.
fn sample_values(key: &str, config: &BTreeMap<String, String>) -> Vec<(&'static str, String)> {
    match key {
        "session_format" => vec![("session", SAMPLE_SESSION_NAME.to_string())],
        "clock_item_format" => vec![("clock", sample_clock(config))],
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
        _ => zjkit::mode::sample_values(key, config).unwrap_or_default(),
    }
}

/// What `{clock}` stands for in the list: the sample time drawn from the
/// `clock_format` being edited. See `clock::Clock::sample_text`.
fn sample_clock(config: &BTreeMap<String, String>) -> String {
    let clock = crate::clock::Clock {
        format: config
            .get(crate::CONFIG_CLOCK_FORMAT)
            .map_or(crate::DEFAULT_CLOCK_FORMAT, String::as_str)
            .to_string(),
        offset_source: crate::clock::OffsetSource::of(
            config
                .get(crate::CONFIG_CLOCK_UTC_OFFSET)
                .map(String::as_str)
                .filter(|offset| !offset.is_empty()),
        ),
        ..Default::default()
    };
    clock.sample_text().unwrap_or_default()
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

/// The bar as `configuration` draws it `width` columns wide, in the mode and
/// Zellij theme of `mode_info`. See `preview_state` for what it shows.
fn draw_bar(configuration: BTreeMap<String, String>, mode_info: &ModeInfo, width: usize) -> String {
    preview_state(configuration, mode_info, width).draw_bar().0
}

/// A bar as `configuration` sets it up `width` columns wide, in the mode and
/// Zellij theme of `mode_info`, listing `SAMPLE_PANES` in `SAMPLE_TABS`.
///
/// The panes go through the same reading of a pane update as a running
/// bar's, so `pane_scope`, the order and the flags are the bar's own. The
/// tray's clock shows the sample time (see `clock::Clock::sample_text`),
/// whatever the time is and without asking the system for its offset, so
/// the preview is the same each time it is drawn. Its session name is the
/// one of the session the screen runs in, or `SAMPLE_SESSION_NAME` before
/// Zellij has sent it.
fn preview_state(
    mut configuration: BTreeMap<String, String>,
    mode_info: &ModeInfo,
    width: usize,
) -> crate::State {
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
        // Its sample panes are its own, and must not reach the bars of the
        // instance the screen runs in.
        instance: crate::instance::Instance::alone(),
        bar: zjkit::bar::BarCore {
            zellij_palette: Some(mode_info.style.colors),
            pane_width: Some(width),
            ..Default::default()
        },
        mode_info: mode_info.clone(),
        tabs: SAMPLE_TABS
            .iter()
            .enumerate()
            .map(|(position, name)| Tab {
                id: position,
                position,
                name: name.to_string(),
                active: position == 0,
                floating_panes_visible: position == 0,
            })
            .collect(),
        ..Default::default()
    };
    state
        .mode_info
        .session_name
        .get_or_insert_with(|| SAMPLE_SESSION_NAME.to_string());
    let mut panes: HashMap<usize, Vec<PaneInfo>> = HashMap::new();
    for (id, sample) in SAMPLE_PANES.iter().enumerate() {
        let pane = PaneInfo {
            id: id as u32,
            title: sample.title.to_string(),
            is_plugin: sample.plugin,
            is_focused: sample.focused,
            // A pane the bar hid is reported as Zellij reports any hidden
            // pane, and listed because the bar keeps its id.
            is_floating: !sample.minimized,
            is_suppressed: sample.minimized,
            is_selectable: true,
            exited: sample.exited,
            ..Default::default()
        };
        if sample.minimized {
            let pane_id = zjkit::panes::pane_id_of(&pane);
            state.minimized_mut().insert(pane_id, Default::default());
        }
        panes.entry(sample.tab).or_default().push(pane);
    }
    state.panes = PaneManifest { panes };
    // Reads the options and lists the panes, as `pane_scope` says.
    state.apply_configuration(configuration);
    state.clock.text = state.clock.sample_text();
    state
}

#[cfg(test)]
mod editor_tests;
#[cfg(test)]
mod view_tests;
