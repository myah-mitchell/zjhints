//! The images in the zjtaskbar docs, drawn by the code that draws the bar.
//!
//! The engine that turns terminal cells into SVG, and decides which files to
//! write, compare or remove, is `zjkit::images`. This module holds what is
//! zjtaskbar's own: how to put the bar on a screen with sample tabs and
//! floating panes, and the list of images the docs show. Each image is filled
//! in from the ANSI the bar prints.
//!
//! `make images` rewrites the SVGs in `docs/img/zjtaskbar` that this module
//! draws, and removes the ones it drew before but no longer does. Without
//! `ZJKIT_WRITE_IMAGES` set, the same test compares what it draws with the
//! files, so `cargo test` in CI fails when a change to the bar leaves an image
//! out of date.
//!
//! A clock in the tray always shows the settings screen's sample time, 09:41
//! UTC, never the time the images are drawn at; see `draw_for_image`.

use crate::draw_for_image;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use zellij_tile::prelude::{InputMode, ModeInfo, PaneInfo, PaneManifest, TabInfo};
use zjkit::images::{mode_info, Pen, Screen};

/// The plugin the images are drawn for. The engine marks each SVG with it and
/// removes only the SVGs that carry it.
const PLUGIN: &str = "zjtaskbar";

/// How wide the images are, in columns, unless a shot is wider.
const IMAGE_COLUMNS: usize = 100;

/// How wide the flags image is: wide enough for every flag in Zellij's own
/// words, so no entry hides behind a marker.
const FLAG_COLUMNS: usize = 112;

/// The themes the images are drawn in, unless a shot overrides them.
const BASE_THEME: [(&str, &str); 3] = [
    ("color_theme", "catppuccin-mocha"),
    ("format_theme", "pills"),
    ("alias_theme", "zellij"),
];

/// The themes that leave the bar looking like Zellij's own.
const ZELLIJ_THEME: [(&str, &str); 3] = [
    ("color_theme", "zellij"),
    ("format_theme", "zellij"),
    ("alias_theme", "zellij"),
];

/// The name of the session every image is drawn in, which the tray's
/// `session` item shows.
const SESSION_NAME: &str = "work";

/// The names of the tabs the sample panes are in, by position. The first is
/// the active one, and shows its floating panes.
const TAB_NAMES: [&str; 2] = ["code", "docs"];

/// A floating pane an image lists.
#[derive(Clone, Copy)]
struct SamplePane {
    title: &'static str,
    /// The position in `TAB_NAMES` of the tab the pane is in.
    tab: usize,
    /// The pane has the keyboard. At most one of the active tab's does.
    focused: bool,
    /// The bar minimized the pane, so it is hidden from the screen.
    minimized: bool,
    /// The pane's command has finished.
    exited: bool,
    /// The pane runs a plugin rather than a terminal.
    plugin: bool,
}

/// A terminal pane in `tab`, titled `title`, in no particular state.
const fn pane(title: &'static str, tab: usize) -> SamplePane {
    SamplePane {
        title,
        tab,
        focused: false,
        minimized: false,
        exited: false,
        plugin: false,
    }
}

const fn focused(pane: SamplePane) -> SamplePane {
    SamplePane {
        focused: true,
        ..pane
    }
}

const fn minimized(pane: SamplePane) -> SamplePane {
    SamplePane {
        minimized: true,
        ..pane
    }
}

const fn exited(pane: SamplePane) -> SamplePane {
    SamplePane {
        exited: true,
        ..pane
    }
}

const fn plugin(pane: SamplePane) -> SamplePane {
    SamplePane {
        plugin: true,
        ..pane
    }
}

/// A session in use: two panes on screen, the editor focused, notes put
/// away, and a manual page left open in the other tab.
const SESSION_PANES: [SamplePane; 4] = [
    pane("htop", 0),
    focused(pane("editor", 0)),
    minimized(pane("notes", 0)),
    pane("man zellij", 1),
];

/// Panes that between them raise every flag: minimized, exited, a plugin,
/// and one in another tab.
const FLAGGED_PANES: [SamplePane; 5] = [
    focused(pane("editor", 0)),
    minimized(pane("notes", 0)),
    exited(pane("cargo build", 0)),
    plugin(pane("filepicker", 0)),
    pane("man zellij", 1),
];

/// More panes than fit in a narrow bar, with the focused one in the middle.
const MANY_PANES: [SamplePane; 10] = [
    pane("htop", 0),
    pane("logs", 0),
    pane("editor", 0),
    pane("server", 0),
    focused(pane("tests", 0)),
    pane("notes", 0),
    pane("mail", 0),
    pane("build", 0),
    pane("deploy", 0),
    pane("scratch", 0),
];

/// One pane, its title longer than a narrow bar.
const LONE_PANE: [SamplePane; 1] = [focused(pane(
    "a pane titled for everything it is used for",
    0,
))];

/// One bar of an image: what its caption says, and how it is drawn.
struct Shot<'a> {
    caption: String,
    options: Vec<(&'a str, &'a str)>,
    panes: &'a [SamplePane],
    columns: usize,
    /// The input mode the bar is drawn in, which only the tray's `mode`
    /// item shows.
    mode: InputMode,
}

impl<'a> Shot<'a> {
    /// The bar of `panes` at full width, with `options` laid over `theme`.
    fn new(
        caption: impl Into<String>,
        theme: &[(&'a str, &'a str)],
        options: &[(&'a str, &'a str)],
        panes: &'a [SamplePane],
    ) -> Self {
        let mut all = theme.to_vec();
        all.extend_from_slice(options);
        Shot {
            caption: caption.into(),
            options: all,
            panes,
            columns: IMAGE_COLUMNS,
            mode: InputMode::Normal,
        }
    }

    fn columns(mut self, columns: usize) -> Self {
        self.columns = columns;
        self
    }

    fn mode(mut self, mode: InputMode) -> Self {
        self.mode = mode;
        self
    }
}

fn configuration_of(options: &[(&str, &str)]) -> BTreeMap<String, String> {
    options
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// The tabs of `TAB_NAMES`, the first active and showing its floating panes.
fn sample_tabs() -> Vec<TabInfo> {
    TAB_NAMES
        .iter()
        .enumerate()
        .map(|(position, name)| TabInfo {
            position,
            tab_id: position,
            name: name.to_string(),
            active: position == 0,
            are_floating_panes_visible: position == 0,
            ..Default::default()
        })
        .collect()
}

/// `panes` as Zellij reports them, each under its tab, ids in the order
/// given. A minimized pane is reported hidden, as Zellij reports any hidden
/// pane, which `draw_for_image` takes as one the bar minimized.
fn manifest_of(panes: &[SamplePane]) -> PaneManifest {
    let mut by_tab: HashMap<usize, Vec<PaneInfo>> = HashMap::new();
    for (id, sample) in panes.iter().enumerate() {
        by_tab.entry(sample.tab).or_default().push(PaneInfo {
            id: id as u32,
            title: sample.title.to_string(),
            is_plugin: sample.plugin,
            is_focused: sample.focused,
            is_floating: !sample.minimized,
            is_suppressed: sample.minimized,
            is_selectable: true,
            exited: sample.exited,
            ..Default::default()
        });
    }
    PaneManifest { panes: by_tab }
}

/// The bar listing `panes`, `columns` wide and in `mode`, with `options` as
/// its configuration, in the session named `SESSION_NAME`.
fn bar(options: &[(&str, &str)], panes: &[SamplePane], columns: usize, mode: InputMode) -> String {
    let mode_info = ModeInfo {
        session_name: Some(SESSION_NAME.to_string()),
        ..mode_info(mode)
    };
    draw_for_image(
        configuration_of(options),
        mode_info,
        &sample_tabs(),
        manifest_of(panes),
        columns,
    )
}

/// A screen holding each shot under its caption, as wide as the widest.
fn shots(shots: &[Shot]) -> Screen {
    let columns = shots.iter().map(|shot| shot.columns).max().unwrap_or(0);
    let mut screen = Screen::new(shots.len() * 3 - 1, columns.max(IMAGE_COLUMNS));
    for (index, shot) in shots.iter().enumerate() {
        let row = index * 3;
        screen.print_with(row, 0, &shot.caption, |_| Pen {
            dim: true,
            ..Pen::default()
        });
        let line = bar(&shot.options, shot.panes, shot.columns, shot.mode);
        screen.print(row + 1, 0, &line);
    }
    screen
}

/// Every image of the docs, by file name.
fn images() -> Vec<(&'static str, String)> {
    let flat = [("format_theme", "flat")];
    let whole_tray = [("tray", "mode session clock")];
    let whole_tray_flat = [("format_theme", "flat"), ("tray", "mode session clock")];
    let whole_tray_powerline = [
        ("format_theme", "powerline"),
        ("tray", "mode session clock"),
    ];
    let dated_clock = [
        ("tray", "session clock"),
        ("clock_format", "%a %d %b %H:%M"),
    ];
    vec![
        (
            "overview-default.svg",
            shots(&[Shot::new(
                "the default look, in the Zellij theme's colors",
                &ZELLIJ_THEME,
                &[],
                &SESSION_PANES,
            )])
            .to_svg("The zjtaskbar bar in the Zellij theme's colors: a Menu button for the launcher, then four floating panes. The htop entry is a ribbon, the editor entry is highlighted as the focused one, and the notes entry is plain text ending in (MINIMIZED). After a line between the tabs comes the man zellij entry, from the docs tab. The rest of the bar is blank, since the tray is empty by default"),
        ),
        (
            "themes-shapes.svg",
            shots(
                &["zellij", "flat", "plain", "powerline", "pills"].map(|shape| {
                    Shot::new(
                        format!("format_theme \"{shape}\""),
                        &[("color_theme", "catppuccin-mocha"), ("alias_theme", "zellij")],
                        &[("format_theme", shape)],
                        &SESSION_PANES,
                    )
                }),
            )
            .to_svg("The launcher's Menu button and the same four floating panes under each format theme, in catppuccin-mocha colors: zellij ribbons, flat blocks, plain colored text, blocks joined by Powerline arrows, and rounded pills. The focused editor entry stands out in the accent color and the minimized notes entry is dimmer than the rest"),
        ),
        (
            "flags-alias-themes.svg",
            shots(&[
                Shot::new("alias_theme \"zellij\"", &BASE_THEME, &[("alias_theme", "zellij")], &FLAGGED_PANES).columns(FLAG_COLUMNS),
                Shot::new("alias_theme \"ascii\"", &BASE_THEME, &[("alias_theme", "ascii")], &FLAGGED_PANES).columns(FLAG_COLUMNS),
                Shot::new("alias_theme \"unicode\"", &BASE_THEME, &[("alias_theme", "unicode")], &FLAGGED_PANES).columns(FLAG_COLUMNS),
            ])
            .to_svg("Five floating panes as pills after the launcher's button: the focused editor, notes minimized, cargo build exited, the filepicker plugin, and man zellij in the docs tab. With the zellij alias theme the button reads Menu and the flags read (MINIMIZED), (EXITED) and (PLUGIN), with none for the other tab; with ascii the button is a plus and the flags are _, X, P and @docs; with unicode the button is three bars and the flags are a down arrow, a cross, a gear and an up-right arrow before docs"),
        ),
        (
            "overflow-markers.svg",
            shots(&[
                Shot::new("100 columns", &BASE_THEME, &flat, &MANY_PANES).columns(100),
                Shot::new("64 columns", &BASE_THEME, &flat, &MANY_PANES).columns(64),
                Shot::new("44 columns", &BASE_THEME, &flat, &MANY_PANES).columns(44),
            ])
            .to_svg("Ten floating panes as flat blocks after the launcher's button, in a bar of 100, 64 and 44 columns. At 100 columns all ten fit. At 64 and 44 the bar keeps the focused tests entry and the entries around it, with a marker on each side counting the entries hidden there"),
        ),
        (
            "overflow-lone-entry.svg",
            shots(&[
                Shot::new("a lone entry wider than the bar, 40 columns", &ZELLIJ_THEME, &[], &LONE_PANE).columns(40),
                Shot::new("the same with overflow_str \"…\"", &ZELLIJ_THEME, &[("overflow_str", "\u{2026}")], &LONE_PANE).columns(40),
            ])
            .to_svg("One focused floating pane titled for everything it is used for, after the launcher's Menu button in a bar of 40 columns. The entry is cut to fit and ends in three dots, then in an ellipsis with overflow_str set to one"),
        ),
        (
            "tray-items.svg",
            shots(&[
                Shot::new("tray \"mode session clock\", in the Zellij theme's colors", &ZELLIJ_THEME, &whole_tray, &SESSION_PANES),
                Shot::new("the same with format_theme \"flat\"", &BASE_THEME, &whole_tray_flat, &SESSION_PANES),
                Shot::new("format_theme \"powerline\", in locked mode", &BASE_THEME, &whole_tray_powerline, &SESSION_PANES).mode(InputMode::Locked),
                Shot::new("format_theme \"pills\"", &BASE_THEME, &whole_tray, &SESSION_PANES),
                Shot::new("tray \"session clock\", clock_format \"%a %d %b %H:%M\"", &BASE_THEME, &dated_clock, &SESSION_PANES),
            ])
            .to_svg("Five zjtaskbar bars with a tray at the right end. With the mode, the session and the clock in the Zellij theme's colors the tray reads NORMAL, work and 09:41 as blocks. With the flat theme each item is a block, with NORMAL in the mode's green and work in the accent color. With the powerline theme in locked mode the mode reads LOCKED in red, and each item sits behind an arrow pointing back at the entries. With the pills theme the mode and the session are whole pills, and the clock against the bar's right edge is a half-pill, rounded on the side facing the entries. The last bar shows only the session and a clock reading Tue 03 Nov 09:41"),
        ),
        (
            "fitting.svg",
            shots(&[
                Shot::new("100 columns", &BASE_THEME, &whole_tray_flat, &SESSION_PANES).columns(100),
                Shot::new("64 columns", &BASE_THEME, &whole_tray_flat, &SESSION_PANES).columns(64),
                Shot::new("44 columns", &BASE_THEME, &whole_tray_flat, &SESSION_PANES).columns(44),
                Shot::new("30 columns", &BASE_THEME, &whole_tray_flat, &SESSION_PANES).columns(30),
                Shot::new("22 columns", &BASE_THEME, &whole_tray_flat, &SESSION_PANES).columns(22),
                Shot::new("15 columns", &BASE_THEME, &whole_tray_flat, &SESSION_PANES).columns(15),
            ])
            .to_svg("The launcher's button, four floating panes and a tray of the mode, the session and the clock, as flat blocks in bars from 100 down to 15 columns. At 100 columns everything fits. At 64 the entries give way first, down to htop, the focused editor and a marker for the two hidden after them. At 44 the editor is left alone, as its markers no longer fit, and at 30 the entries are gone while the whole tray still fits. At 22 the tray has dropped the mode from its left end, and at 15 the session too, leaving the button and the clock"),
        ),
    ]
}

/// Where the images live: `docs/img/zjtaskbar` at the workspace root, two
/// levels above this crate.
fn image_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/img/zjtaskbar")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_the_images_twice_gives_the_same_bytes() {
        assert_eq!(images(), images());
    }

    #[test]
    fn the_images_in_the_docs_are_up_to_date() {
        zjkit::images::write_or_compare_images(PLUGIN, &image_directory(), &images());
    }
}
