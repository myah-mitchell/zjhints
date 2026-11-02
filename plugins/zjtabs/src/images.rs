//! The images in the zjtabs docs, drawn by the code that draws the bar.
//!
//! The engine that turns terminal cells into SVG, and decides which files to
//! write, compare or remove, is `zjkit::images`. This module holds what is
//! zjtabs' own: how to put the bar on a screen with sample tabs, and the list
//! of images the docs show. Each image is filled in from the ANSI the bar
//! prints.
//!
//! `make images` rewrites the SVGs in `docs/img/zjtabs` that this module
//! draws, and removes the ones it drew before but no longer does. Without
//! `ZJKIT_WRITE_IMAGES` set, the same test compares what it draws with the
//! files, so `cargo test` in CI fails when a change to the bar leaves an image
//! out of date.

use crate::flags::FLAGS;
use crate::tabs::Tab;
use crate::State;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use zellij_tile::prelude::InputMode;
use zjkit::images::{mode_info, palette, Pen, Screen};

/// The plugin the images are drawn for. The engine marks each SVG with it and
/// removes only the SVGs that carry it.
const PLUGIN: &str = "zjtabs";

/// How wide the images are, in columns.
const IMAGE_COLUMNS: usize = 100;

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

/// A tab as `(name, active, flags)`, the flags in the order of `FLAGS`:
/// fullscreen, sync, floating and the bell.
type SampleTab = (&'static str, bool, [bool; FLAGS.len()]);

const NONE: [bool; FLAGS.len()] = [false; FLAGS.len()];
const FULLSCREEN: [bool; FLAGS.len()] = [true, false, false, false];
const SYNC: [bool; FLAGS.len()] = [false, true, false, false];
const FLOATING: [bool; FLAGS.len()] = [false, false, true, false];
const BELL: [bool; FLAGS.len()] = [false, false, false, true];

/// A few tabs with the editor active, one for each flag.
const FLAGGED_TABS: [SampleTab; 5] = [
    ("shell", false, NONE),
    ("editor", true, NONE),
    ("docs", false, FULLSCREEN),
    ("logs", false, SYNC),
    ("server", false, BELL),
];

/// Tabs that read as a session in use, a flag among them.
const SESSION_TABS: [SampleTab; 5] = [
    ("shell", false, NONE),
    ("editor", true, NONE),
    ("server", false, BELL),
    ("docs", false, NONE),
    ("notes", false, FLOATING),
];

/// More tabs than fit in a narrow bar, with the active one in the middle.
const MANY_TABS: [SampleTab; 10] = [
    ("shell", false, NONE),
    ("docs", false, NONE),
    ("editor", false, NONE),
    ("server", false, NONE),
    ("logs", true, NONE),
    ("mail", false, NONE),
    ("notes", false, NONE),
    ("build", false, NONE),
    ("deploy", false, NONE),
    ("scratch", false, NONE),
];

/// One tab, longer than a narrow bar.
const LONE_TAB: [SampleTab; 1] = [("a tab named for everything it is used for", true, NONE)];

/// One bar of an image: what its caption says, and how it is drawn.
struct Shot<'a> {
    caption: String,
    options: Vec<(&'a str, &'a str)>,
    tabs: &'a [SampleTab],
    columns: usize,
}

impl<'a> Shot<'a> {
    /// The bar of `tabs` at full width, with `options` laid over `theme`.
    fn new(
        caption: impl Into<String>,
        theme: &[(&'a str, &'a str)],
        options: &[(&'a str, &'a str)],
        tabs: &'a [SampleTab],
    ) -> Self {
        let mut all = theme.to_vec();
        all.extend_from_slice(options);
        Shot {
            caption: caption.into(),
            options: all,
            tabs,
            columns: IMAGE_COLUMNS,
        }
    }

    fn columns(mut self, columns: usize) -> Self {
        self.columns = columns;
        self
    }
}

fn configuration_of(options: &[(&str, &str)]) -> BTreeMap<String, String> {
    options
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// The bar of `tabs`, `columns` wide, with `options` as its configuration.
fn bar(options: &[(&str, &str)], tabs: &[SampleTab], columns: usize) -> String {
    let mut state = State::default();
    state.bar.zellij_palette = Some(palette());
    state.apply_configuration(configuration_of(options));
    state.bar.pane_width = Some(columns);
    state.mode_info = mode_info(InputMode::Normal);
    state.tabs = tabs
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

/// A screen holding each shot under its caption.
fn shots(shots: &[Shot]) -> Screen {
    let mut screen = Screen::new(shots.len() * 3 - 1, IMAGE_COLUMNS);
    for (index, shot) in shots.iter().enumerate() {
        let row = index * 3;
        screen.print_with(row, 0, &shot.caption, |_| Pen {
            dim: true,
            ..Pen::default()
        });
        screen.print(row + 1, 0, &bar(&shot.options, shot.tabs, shot.columns));
    }
    screen
}

/// Every image of the docs, by file name.
fn images() -> Vec<(&'static str, String)> {
    let flat = [("format_theme", "flat")];
    let no_sync_flag = [("format_theme", "flat"), ("sync_format", "")];
    let styled_bell = [
        ("format_theme", "flat"),
        ("bell_format", "#[fg=$red,bold] \u{25cf}"),
    ];
    vec![
        (
            "overview-default.svg",
            shots(&[Shot::new(
                "the default look, in the Zellij theme's colors",
                &ZELLIJ_THEME,
                &[],
                &SESSION_TABS,
            )])
            .to_svg("The zjtabs bar in the Zellij theme's colors: five tabs numbered from 1 as ribbons, the editor tab highlighted, the server tab ending in a dot, and the notes tab ending in (FLOATING)"),
        ),
        (
            "themes-shapes.svg",
            shots(
                &["zellij", "flat", "plain", "powerline", "pills"].map(|shape| {
                    Shot::new(
                        format!("format_theme \"{shape}\""),
                        &[("color_theme", "catppuccin-mocha"), ("alias_theme", "zellij")],
                        &[("format_theme", shape)],
                        &SESSION_TABS,
                    )
                }),
            )
            .to_svg("The same five tabs under each format theme: zellij ribbons, flat blocks, plain colored text, blocks joined by Powerline arrows, and rounded pills, all in catppuccin-mocha colors"),
        ),
        (
            "flags-alias-themes.svg",
            shots(&[
                Shot::new("alias_theme \"zellij\"", &BASE_THEME, &[("alias_theme", "zellij")], &FLAGGED_TABS),
                Shot::new("alias_theme \"ascii\"", &BASE_THEME, &[("alias_theme", "ascii")], &FLAGGED_TABS),
                Shot::new("alias_theme \"unicode\"", &BASE_THEME, &[("alias_theme", "unicode")], &FLAGGED_TABS),
            ])
            .to_svg("Five tabs with the docs tab fullscreen, the logs tab synced and the server tab belled. With the zellij alias theme the flags read (FULLSCREEN), (SYNC) and a dot; with ascii they read Z, S and !; with unicode they are symbols"),
        ),
        (
            "flags-formats.svg",
            shots(&[
                Shot::new("the default flag formats", &BASE_THEME, &flat, &FLAGGED_TABS),
                Shot::new("sync_format \"\" hides the sync flag", &BASE_THEME, &no_sync_flag, &FLAGGED_TABS),
                Shot::new("bell_format \"#[fg=$red,bold] ●\" styles the bell on its own", &BASE_THEME, &styled_bell, &FLAGGED_TABS),
            ])
            .to_svg("Five tabs with flat blocks. The default flag formats show (FULLSCREEN), (SYNC) and a dot after the names; with sync_format empty the logs tab shows no flag; with a styled bell_format the server tab's dot is red"),
        ),
        (
            "overflow-markers.svg",
            shots(&[
                Shot::new("100 columns", &BASE_THEME, &flat, &MANY_TABS).columns(100),
                Shot::new("64 columns", &BASE_THEME, &flat, &MANY_TABS).columns(64),
                Shot::new("44 columns", &BASE_THEME, &flat, &MANY_TABS).columns(44),
            ])
            .to_svg("Ten tabs in a bar of 100, 64 and 44 columns. At each width the bar keeps the logs tab and the tabs around it, with a marker counting the tabs hidden on each side"),
        ),
        (
            "overflow-lone-tab.svg",
            shots(&[
                Shot::new("a lone tab wider than the bar, 36 columns", &ZELLIJ_THEME, &[], &LONE_TAB).columns(36),
                Shot::new("the same with overflow_str \"…\"", &ZELLIJ_THEME, &[("overflow_str", "\u{2026}")], &LONE_TAB).columns(36),
            ])
            .to_svg("One active tab named for everything it is used for, in a bar of 36 columns. The tab is cut to fit and ends in three dots, then in an ellipsis with overflow_str set to one"),
        ),
    ]
}

/// Where the images live: `docs/img/zjtabs` at the workspace root, two levels
/// above this crate.
fn image_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/img/zjtabs")
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
