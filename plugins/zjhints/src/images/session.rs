//! The sessions the README shows: a Zellij session drawn by hand around the
//! real bar, played through one of the plugin's features a keypress at a time.
//!
//! Only the chrome is a stand-in. The tab bar, the pane frames and what runs
//! in the panes are drawn here to look like Zellij's, while the bar, its hover
//! preview and the settings screen come from the code that draws them in the
//! plugin, so the demo cannot show the plugin doing anything it does not.

use super::{
    bar_state, draw_pointer, hover_at, hovers_of, palette, rgb_of, settings_screen, Animation,
    Frame, Hover, Pen, Rgb, Screen, BASE_THEME, PILL_RIGHT, POWERLINE_ARROW, TERMINAL_BACKGROUND,
};
use crate::{ActiveTab, ClickMap, NestedGuest, TabFocus};
use zellij_tile::prelude::{InputMode, PaneId};

/// The terminal's size, and the rows under it that narrate the demo: one
/// for the pointer under the bar, a rule setting the narration apart from
/// the session, and the narration itself.
const ROWS: usize = 24;
const COLUMNS: usize = 120;
const CAPTION_ROWS: usize = 3;
/// The row the bar sits on, in a borderless pane of its own at the bottom.
const BAR_ROW: usize = ROWS - 1;
/// How narrow the terminal gets when it is resized, and by how many columns
/// a frame. The steps land on both ends.
const NARROWEST: usize = 66;
const RESIZE_STEP: usize = 6;
const _: () = assert!((COLUMNS - NARROWEST) % RESIZE_STEP == 0);

/// The settings screen's size inside its floating pane's frame.
const SETTINGS_ROWS: usize = 18;
const SETTINGS_COLUMNS: usize = 84;

/// The options of the alias the session loads zjhints from, besides its
/// themes: the mode shown and the mouse on.
const BASE_OPTIONS: [(&str, &str); 2] = [("show_mode", "true"), ("mouse", "true")];

/// The themes the settings screen picks in its demo, one option at a time.
const THEME_EDITS: [(&str, &str); 2] = [
    ("format_theme", "powerline"),
    ("color_theme", "tokyo-night"),
];

/// A color, format and alias theme, as the alias sets them.
type Look = [(&'static str, &'static str); 3];

const fn look(color: &'static str, format: &'static str, alias: &'static str) -> Look {
    [
        ("color_theme", color),
        ("format_theme", format),
        ("alias_theme", alias),
    ]
}

/// The combinations of themes the themes demo shows, varied so that no two
/// in a row share a color or a format. The nerd-font symbols are left out,
/// as in the alias themes image: the image has no font to draw them.
static THEME_TOUR: [Look; 15] = [
    look("catppuccin-mocha", "pills", "unicode"),
    look("dracula", "powerline", "ascii"),
    look("nord", "flat", "unicode"),
    look("gruvbox-dark", "pills", "zellij"),
    look("tokyo-night", "plain", "unicode"),
    look("catppuccin-latte", "powerline", "unicode"),
    look("zellij", "zellij", "zellij"),
    look("dracula", "flat", "unicode"),
    look("nord", "pills", "ascii"),
    look("gruvbox-dark", "powerline", "unicode"),
    look("tokyo-night", "pills", "zellij"),
    look("catppuccin-latte", "flat", "ascii"),
    look("catppuccin-mocha", "powerline", "zellij"),
    look("dracula", "plain", "unicode"),
    look("nord", "powerline", "unicode"),
];

/// The pane a nested session runs in, as the host knows it.
const NESTED_PANE: PaneId = PaneId::Terminal(2);

/// What runs in the pane on the right once the terminal is split.
#[derive(Clone, Copy, PartialEq)]
enum RightPane {
    /// No split yet.
    None,
    /// A shell, with `typed` at its prompt.
    Shell { typed: &'static str },
    /// A nested Zellij session, in `mode`, which has the keyboard while
    /// `descended`.
    Nested { mode: InputMode, descended: bool },
}

/// The settings screen open in a floating pane.
#[derive(Clone, Copy)]
struct Settings {
    /// How many of `THEME_EDITS` are made.
    edits: usize,
    saved: bool,
}

/// The mouse pointer under the bar.
#[derive(Clone, Copy)]
struct Pointer {
    column: usize,
    /// Whether it has rested long enough for the hover preview to open.
    open: bool,
}

/// Everything on screen at one step of a demo.
#[derive(Clone, Copy)]
struct Scene {
    columns: usize,
    mode: InputMode,
    /// The themes the alias sets.
    look: &'static Look,
    /// How many of `THEME_EDITS` the bar has saved into its options.
    theme: usize,
    right: RightPane,
    settings: Option<Settings>,
    pointer: Option<Pointer>,
    /// The key pressed for this step, if any, and what it did.
    key: &'static str,
    caption: &'static str,
    /// Whether the narration names the themes of `look` after the caption.
    name_look: bool,
}

impl Scene {
    /// A session in normal mode with one pane, drawn in the base themes.
    fn new(caption: &'static str) -> Self {
        Scene {
            columns: COLUMNS,
            mode: InputMode::Normal,
            look: &BASE_THEME,
            theme: 0,
            right: RightPane::None,
            settings: None,
            pointer: None,
            key: "",
            caption,
            name_look: false,
        }
    }

    /// The options of the alias, with the themes saved from the settings
    /// screen over the ones it started with.
    fn saved_options(&self) -> Vec<(&'static str, &'static str)> {
        let mut options = BASE_OPTIONS.to_vec();
        options.extend_from_slice(self.look);
        options.extend_from_slice(&THEME_EDITS[..self.theme]);
        options
    }

    /// The options the bar runs with, which take the settings screen's edits
    /// as it previews them.
    fn options(&self) -> Vec<(&'static str, &'static str)> {
        let mut options = self.saved_options();
        if let Some(settings) = self.settings {
            options.extend_from_slice(&THEME_EDITS[..settings.edits]);
        }
        options
    }

    /// The nested session's mode while the host has descended into it.
    fn descended(&self) -> Option<InputMode> {
        match self.right {
            RightPane::Nested {
                mode,
                descended: true,
            } => Some(mode),
            _ => None,
        }
    }

    /// The bar, and the click map its hover preview is found from.
    fn bar(&self) -> (String, ClickMap) {
        let options = self.options();
        let mut state = bar_state(&options, self.mode, self.columns, false);
        if let Some(mode) = self.descended() {
            // The host has handed the keyboard to the session in the focused
            // pane, which has reported its mode and keybindings.
            state.mode_info.session_dimmed = Some(true);
            state.active_tab = Some(ActiveTab::default());
            state.focus_by_tab.insert(
                0,
                TabFocus {
                    tiled: Some(NESTED_PANE),
                    floating: None,
                },
            );
            state.nested_guests.insert(
                NESTED_PANE,
                NestedGuest {
                    session_path: vec!["work".to_string()],
                    mode,
                    base_mode: Some(InputMode::Normal),
                    keybinds: state.mode_info.keybinds.clone(),
                    fetched_generation: Some(1),
                },
            );
        }
        state.draw_bar()
    }

    /// Every hover preview the bar opens, left to right.
    fn hovers(&self) -> Vec<Hover> {
        hovers_of(&self.bar().1, self.columns)
    }
}

/// The colors the hand-drawn chrome takes from the Zellij theme.
struct Chrome {
    text: Rgb,
    focused_frame: Rgb,
    frame: Rgb,
    tab: Rgb,
    tab_text: Rgb,
    path: Rgb,
    prompt: Rgb,
    passed: Rgb,
    /// Fades everything drawn with it, as Zellij fades the chrome of a
    /// session that does not have the keyboard.
    dim: bool,
}

impl Chrome {
    fn new(dim: bool) -> Self {
        let styling = palette();
        let text = styling.text_unselected;
        Chrome {
            text: rgb_of(text.base),
            focused_frame: rgb_of(styling.frame_selected.base),
            frame: rgb_of(
                styling
                    .frame_unselected
                    .map_or(styling.list_unselected.base, |frame| frame.base),
            ),
            tab: rgb_of(styling.ribbon_selected.background),
            tab_text: rgb_of(styling.ribbon_selected.base),
            path: rgb_of(text.emphasis_0),
            prompt: rgb_of(text.emphasis_2),
            passed: rgb_of(text.emphasis_2),
            dim,
        }
    }

    fn pen(&self, color: Rgb) -> Pen {
        Pen {
            dim: self.dim,
            ..Pen::colored(color)
        }
    }
}

/// A rectangle of the screen.
#[derive(Clone, Copy)]
struct Area {
    row: usize,
    column: usize,
    rows: usize,
    columns: usize,
}

impl Area {
    /// What is inside the frame around this area.
    fn inside(&self) -> Area {
        Area {
            row: self.row + 1,
            column: self.column + 1,
            rows: self.rows.saturating_sub(2),
            columns: self.columns.saturating_sub(2),
        }
    }
}

/// `text` from a spot of the grid in one pen, and the column after it.
fn paint(screen: &mut Screen, row: usize, column: usize, text: &str, pen: Pen) -> usize {
    screen.print_with(row, column, text, |_| pen);
    column + text.chars().count()
}

/// Zellij's tab bar across the top of `area`: the session's name, then its
/// one tab as a ribbon.
fn draw_tab_bar(screen: &mut Screen, area: Area, session: &str, chrome: &Chrome) {
    let column = paint(
        screen,
        area.row,
        area.column,
        &format!(" Zellij ({session}) "),
        Pen {
            bold: true,
            ..chrome.pen(chrome.text)
        },
    );
    let ribbon = Pen {
        background: Some(chrome.tab),
        bold: true,
        ..chrome.pen(chrome.tab_text)
    };
    let column = paint(
        screen,
        area.row,
        column,
        &POWERLINE_ARROW.to_string(),
        Pen {
            background: Some(chrome.tab),
            ..chrome.pen(TERMINAL_BACKGROUND)
        },
    );
    let column = paint(screen, area.row, column, " Tab #1 ", ribbon);
    paint(
        screen,
        area.row,
        column,
        &POWERLINE_ARROW.to_string(),
        chrome.pen(chrome.tab),
    );
}

/// Whatever `draw` puts in `area`, cut off at its edges as a pane cuts off
/// what runs in it. `draw` is handed the area as its own grid's.
fn in_pane(screen: &mut Screen, area: Area, draw: impl FnOnce(&mut Screen, Area)) {
    let mut pane = Screen::new(area.rows, area.columns);
    draw(
        &mut pane,
        Area {
            row: 0,
            column: 0,
            ..area
        },
    );
    screen.paste(area.row, area.column, &pane);
}

/// A pane's frame around `area`, its title set into the top edge.
fn draw_frame(screen: &mut Screen, area: Area, title: &str, focused: bool, chrome: &Chrome) {
    let color = if focused {
        chrome.focused_frame
    } else {
        chrome.frame
    };
    let pen = Pen {
        bold: focused,
        ..chrome.pen(color)
    };
    let inner = area.columns.saturating_sub(2);
    let top = format!(
        "┌ {title} {}┐",
        "─".repeat(inner.saturating_sub(title.chars().count() + 2))
    );
    paint(screen, area.row, area.column, &top, pen);
    for row in area.row + 1..area.row + area.rows - 1 {
        paint(screen, row, area.column, "│", pen);
        paint(screen, row, area.column + area.columns - 1, "│", pen);
    }
    let bottom = format!("└{}┘", "─".repeat(inner));
    paint(screen, area.row + area.rows - 1, area.column, &bottom, pen);
}

/// A shell prompt at a spot of `area` with `typed` after it, and the column
/// after that.
fn draw_command(
    screen: &mut Screen,
    row: usize,
    column: usize,
    typed: &str,
    chrome: &Chrome,
) -> usize {
    let column = paint(
        screen,
        row,
        column,
        "~/src/zjhints ",
        chrome.pen(chrome.path),
    );
    let column = paint(screen, row, column, "❯ ", chrome.pen(chrome.prompt));
    paint(screen, row, column, typed, chrome.pen(chrome.text))
}

/// A shell prompt waiting at a spot of `area`, with `typed` after it and the
/// cursor.
fn draw_prompt(screen: &mut Screen, row: usize, column: usize, typed: &str, chrome: &Chrome) {
    let column = draw_command(screen, row, column, typed, chrome);
    paint(
        screen,
        row,
        column,
        " ",
        Pen {
            background: Some(chrome.text),
            ..chrome.pen(chrome.text)
        },
    );
}

/// The left pane's shell, with a test run in it.
fn draw_test_run(screen: &mut Screen, area: Area, chrome: &Chrome) {
    let (row, column) = (area.row, area.column + 1);
    draw_command(screen, row, column, "cargo test", chrome);
    let lines = [
        "   Compiling zjhints (~/src/zjhints)",
        "    Finished `test` profile in 4.21s",
        "     Running unittests src/main.rs",
    ];
    for (offset, line) in lines.iter().enumerate() {
        paint(
            screen,
            row + 1 + offset,
            column,
            line,
            chrome.pen(chrome.text),
        );
    }
    let column = paint(
        screen,
        row + 4,
        column,
        "test result: ",
        chrome.pen(chrome.text),
    );
    let column = paint(screen, row + 4, column, "ok", chrome.pen(chrome.passed));
    paint(
        screen,
        row + 4,
        column,
        ". 373 passed; 0 failed",
        chrome.pen(chrome.text),
    );
    draw_prompt(screen, row + 5, area.column + 1, "", chrome);
}

/// A nested Zellij session filling `area`: its own tab bar and one pane. Its
/// bar is left off, as `hide_when_nested` does.
fn draw_nested(screen: &mut Screen, area: Area, descended: bool) {
    let chrome = Chrome::new(!descended);
    draw_tab_bar(screen, Area { rows: 1, ..area }, "work", &chrome);
    let pane = Area {
        row: area.row + 1,
        rows: area.rows - 1,
        ..area
    };
    draw_frame(screen, pane, "~", true, &chrome);
    let inside = pane.inside();
    draw_prompt(screen, inside.row, inside.column + 1, "", &chrome);
}

/// The settings screen in a floating pane over the middle of the terminal.
fn draw_settings(screen: &mut Screen, scene: &Scene, settings: Settings, chrome: &Chrome) {
    let area = Area {
        row: 2,
        column: (scene.columns.saturating_sub(SETTINGS_COLUMNS + 2)) / 2,
        rows: SETTINGS_ROWS + 2,
        columns: SETTINGS_COLUMNS + 2,
    };
    // The settings screen shows the alias as saved, and its edits until
    // they are.
    let options = scene.saved_options();
    let edits = if settings.saved {
        Vec::new()
    } else {
        THEME_EDITS[..settings.edits].to_vec()
    };
    // The row of the option being changed, or of the first one before any is.
    let select = THEME_EDITS[settings.edits.saturating_sub(1)].0;
    let inside = area.inside();
    let blank = Screen::new(area.rows, area.columns);
    screen.paste(area.row, area.column, &blank);
    draw_frame(screen, area, "zjhints settings", true, chrome);
    screen.paste(
        inside.row,
        inside.column,
        &settings_screen(&options, select, &edits, inside.rows, inside.columns),
    );
}

/// The pointer under the bar, and the hover preview of the hint it rests
/// on once that has opened.
fn draw_mouse(screen: &mut Screen, pointer: Pointer, columns: usize, map: &ClickMap) {
    if pointer.open {
        hover_at(map, pointer.column, columns)
            .expect("a hint that opens a menu under the pointer")
            .draw_above(screen, BAR_ROW);
    }
    draw_pointer(screen, BAR_ROW + 1, pointer.column);
}

/// The key pressed for a step, as a key cap, and what it did beside it.
fn draw_caption(screen: &mut Screen, scene: &Scene) {
    let styling = palette();
    // A dashed rule, labeled, so the narration is not taken for the session.
    let rule = Pen {
        dim: true,
        ..Pen::colored(rgb_of(styling.text_unselected.base))
    };
    let label = "╌╌ narration, not part of the session ";
    let dashes = "╌".repeat(COLUMNS - label.chars().count());
    paint(screen, ROWS + 1, 0, &format!("{label}{dashes}"), rule);
    let row = ROWS + CAPTION_ROWS - 1;
    let mut column = 1;
    if !scene.key.is_empty() {
        let cap = Pen {
            foreground: Some(rgb_of(styling.ribbon_selected.base)),
            background: Some(rgb_of(styling.ribbon_selected.background)),
            bold: true,
            ..Pen::default()
        };
        column = paint(screen, row, column, &format!(" {} ", scene.key), cap);
        column = paint(
            screen,
            row,
            column,
            &PILL_RIGHT.to_string(),
            Pen::colored(rgb_of(styling.ribbon_selected.background)),
        );
        column += 1;
    }
    let narration = Pen {
        italic: true,
        ..Pen::colored(rgb_of(styling.text_unselected.base))
    };
    let column = paint(screen, row, column, scene.caption, narration);
    if scene.name_look {
        let names = scene
            .look
            .iter()
            .map(|(key, value)| format!("{key} \"{value}\""))
            .collect::<Vec<_>>()
            .join("  ");
        paint(screen, row, column, &names, narration);
    }
}

/// One step of the demo as a screen.
fn draw(scene: &Scene) -> Screen {
    let mut screen = Screen::new(ROWS + CAPTION_ROWS, COLUMNS);
    let descended = scene.descended().is_some();
    let chrome = Chrome::new(descended);
    let columns = scene.columns;

    draw_tab_bar(
        &mut screen,
        Area {
            row: 0,
            column: 0,
            rows: 1,
            columns,
        },
        "demo",
        &chrome,
    );
    let panes = Area {
        row: 1,
        column: 0,
        rows: ROWS - 2,
        columns,
    };
    let floating = scene.settings.is_some();
    if scene.right == RightPane::None {
        draw_frame(&mut screen, panes, "~/src/zjhints", !floating, &chrome);
        in_pane(&mut screen, panes.inside(), |pane, area| {
            draw_test_run(pane, area, &chrome)
        });
    } else {
        let left = Area {
            columns: columns / 2,
            ..panes
        };
        let right = Area {
            column: left.columns,
            columns: columns - left.columns,
            ..panes
        };
        draw_frame(&mut screen, left, "~/src/zjhints", false, &chrome);
        in_pane(&mut screen, left.inside(), |pane, area| {
            draw_test_run(pane, area, &chrome)
        });
        let title = match scene.right {
            RightPane::Nested { .. } => "zellij attach work",
            _ => "~/src/zjhints",
        };
        draw_frame(&mut screen, right, title, !floating, &chrome);
        match scene.right {
            RightPane::Shell { typed } => {
                in_pane(&mut screen, right.inside(), |pane, area| {
                    draw_prompt(pane, area.row, area.column + 1, typed, &chrome)
                });
            }
            RightPane::Nested { descended, .. } => {
                in_pane(&mut screen, right.inside(), |pane, area| {
                    draw_nested(pane, area, descended)
                });
            }
            RightPane::None => {}
        }
    }
    if let Some(settings) = scene.settings {
        draw_settings(&mut screen, scene, settings, &Chrome::new(false));
    }

    let (bar, map) = scene.bar();
    screen.print(BAR_ROW, 0, &bar);
    if let Some(pointer) = scene.pointer {
        draw_mouse(&mut screen, pointer, columns, &map);
    }
    if columns < COLUMNS {
        // The terminal's right edge, as it narrows.
        let edge = Pen::colored(chrome.frame);
        for row in 0..ROWS {
            screen.print_with(row, columns, "▕", |_| edge);
        }
    }
    draw_caption(&mut screen, scene);
    screen
}

/// The steps of a demo, each with how long it stays up, and the step a
/// reader who prefers reduced motion sees.
#[derive(Default)]
struct Script {
    steps: Vec<(Scene, u32)>,
    still: usize,
}

impl Script {
    fn show(&mut self, scene: &Scene, milliseconds: u32) {
        self.steps.push((*scene, milliseconds));
    }

    /// `show`, standing still on this step without motion.
    fn show_still(&mut self, scene: &Scene, milliseconds: u32) {
        self.still = self.steps.len();
        self.show(scene, milliseconds);
    }

    /// The pointer moving from `from` to `to` along the bar, too quickly for
    /// a hover preview to open, then resting on `to` until it does.
    fn move_pointer(&mut self, scene: &mut Scene, from: usize, to: usize) {
        for step in 1..4 {
            let column = from as isize + (to as isize - from as isize) * step / 4;
            scene.pointer = Some(Pointer {
                column: column as usize,
                open: false,
            });
            self.show(scene, 200);
        }
        scene.pointer = Some(Pointer {
            column: to,
            open: false,
        });
        self.show(scene, 600);
    }

    fn animation(self) -> Animation {
        let frames = self
            .steps
            .iter()
            .map(|(scene, milliseconds)| Frame::new(draw(scene), *milliseconds))
            .collect();
        Animation::new(frames, self.still)
    }
}

/// The bar following the session from mode to mode. It stands still in
/// pane mode.
pub(super) fn modes() -> Animation {
    use InputMode::{Locked, Normal, Pane, Resize, Tab};
    let mut script = Script::default();
    let mut scene = Scene::new("zjhints shows the keys of the mode you are in");
    script.show(&scene, 4000);
    scene.mode = Pane;
    (scene.key, scene.caption) = ("Ctrl p", "pane mode: the bar follows");
    script.show_still(&scene, 3200);
    scene.mode = Normal;
    scene.right = RightPane::Shell { typed: "" };
    (scene.key, scene.caption) = ("r", "split right, and back to normal mode");
    script.show(&scene, 3000);
    scene.mode = Tab;
    (scene.key, scene.caption) = ("Ctrl t", "tab mode");
    script.show(&scene, 2800);
    scene.mode = Normal;
    (scene.key, scene.caption) = ("Esc", "back to normal mode");
    script.show(&scene, 2000);
    scene.mode = Resize;
    (scene.key, scene.caption) = ("Ctrl n", "resize mode");
    script.show(&scene, 2800);
    scene.mode = Normal;
    (scene.key, scene.caption) = ("Esc", "back to normal mode");
    script.show(&scene, 2000);
    scene.mode = Locked;
    (scene.key, scene.caption) = ("Ctrl g", "locked mode: only the way out is shown");
    script.show(&scene, 3400);
    scene.mode = Normal;
    (scene.key, scene.caption) = ("Ctrl g", "unlocked, back to normal mode");
    script.show(&scene, 2400);
    script.animation()
}

/// The terminal narrowing and widening again, the bar dropping whole hints
/// to fit and the drop indicator offering the ones that did not. It stands
/// still on the drop indicator's preview.
pub(super) fn fitting() -> Animation {
    let mut script = Script::default();
    let mut scene = Scene::new("the bar at the full width of the terminal");
    scene.right = RightPane::Shell { typed: "" };
    script.show(&scene, 3000);
    scene.caption = "the terminal narrows: whole hints drop to fit";
    for columns in (NARROWEST..COLUMNS).step_by(RESIZE_STEP).rev() {
        scene.columns = columns;
        script.show(&scene, if columns == NARROWEST { 2000 } else { 350 });
    }
    // The drop indicator is the last thing on the bar that opens a preview.
    let indicator = scene
        .hovers()
        .last()
        .expect("a drop indicator once the bar narrows")
        .middle();
    (scene.key, scene.caption) = ("hover", "the ones that did not fit, a click away");
    scene.pointer = Some(Pointer {
        column: indicator,
        open: false,
    });
    script.show(&scene, 600);
    scene.pointer = Some(Pointer {
        column: indicator,
        open: true,
    });
    script.show_still(&scene, 4600);
    scene.pointer = None;
    (scene.key, scene.caption) = ("", "the terminal widens again");
    for columns in (NARROWEST..=COLUMNS).step_by(RESIZE_STEP).skip(1) {
        scene.columns = columns;
        script.show(&scene, if columns == COLUMNS { 2400 } else { 300 });
    }
    script.animation()
}

/// The settings screen restyling the bar, which previews each change, then
/// saving it. It stands still once every change is previewed.
pub(super) fn settings() -> Animation {
    let mut script = Script::default();
    let mut scene = Scene::new("the bar in catppuccin-mocha, pills and unicode");
    scene.right = RightPane::Shell { typed: "" };
    script.show(&scene, 3000);
    scene.settings = Some(Settings {
        edits: 0,
        saved: false,
    });
    (scene.key, scene.caption) = ("Alt ,", "open the settings screen");
    script.show(&scene, 3200);
    let captions = [
        "format_theme \"powerline\": the bar previews each change",
        "color_theme \"tokyo-night\"",
    ];
    for (edits, caption) in captions.into_iter().enumerate() {
        scene.settings = Some(Settings {
            edits: edits + 1,
            saved: false,
        });
        (scene.key, scene.caption) = ("", caption);
        if edits + 1 == THEME_EDITS.len() {
            script.show_still(&scene, 3400);
        } else {
            script.show(&scene, 3400);
        }
    }
    scene.settings = Some(Settings {
        edits: THEME_EDITS.len(),
        saved: true,
    });
    scene.theme = THEME_EDITS.len();
    (scene.key, scene.caption) = ("Ctrl a", "save it to the config file");
    script.show(&scene, 2800);
    scene.settings = None;
    (scene.key, scene.caption) = ("Esc", "close the settings screen: the new look stays");
    script.show(&scene, 3000);
    script.animation()
}

/// The hint the mouse demo rests on in a mode: the last before the drop
/// indicator, so the pointer crosses others on its way.
fn resting_hint(hovers: &[Hover]) -> &Hover {
    match hovers {
        [.., hint, _] | [hint] => hint,
        [] => panic!("a hint that opens a menu"),
    }
}

/// The pointer resting on hints of the bar, each one's menu opening over it,
/// in normal mode and then in pane mode. A menu closes when the mode
/// changes the bar under it, as the plugin closes it. It stands still on a
/// pane mode menu.
pub(super) fn mouse() -> Animation {
    let mut script = Script::default();
    let mut scene = Scene::new("the pointer on the bar's mode prefix opens nothing");
    scene.right = RightPane::Shell { typed: "" };
    let start = 2;
    scene.pointer = Some(Pointer {
        column: start,
        open: false,
    });
    script.show(&scene, 2000);

    let hint = resting_hint(&scene.hovers()).middle();
    scene.caption = "moving across hints opens nothing until the pointer rests";
    script.move_pointer(&mut scene, start, hint);
    (scene.key, scene.caption) = ("hover", "rest on a hint: its menu opens");
    scene.pointer = Some(Pointer {
        column: hint,
        open: true,
    });
    script.show(&scene, 3600);

    scene.mode = InputMode::Pane;
    (scene.key, scene.caption) = (
        "Ctrl p",
        "pane mode: the menu closes with the bar it came from",
    );
    scene.pointer = Some(Pointer {
        column: hint,
        open: false,
    });
    script.show(&scene, 2400);
    let hovers = scene.hovers();
    let pane_hint = resting_hint(&hovers).middle();
    (scene.key, scene.caption) = ("", "the menus follow the mode");
    script.move_pointer(&mut scene, hint, pane_hint);
    (scene.key, scene.caption) = ("hover", "a pane mode hint's menu");
    scene.pointer = Some(Pointer {
        column: pane_hint,
        open: true,
    });
    script.show_still(&scene, 3600);

    let indicator = hovers.last().expect("a drop indicator").middle();
    if indicator != pane_hint {
        (scene.key, scene.caption) = ("", "on to the drop indicator");
        script.move_pointer(&mut scene, pane_hint, indicator);
        (scene.key, scene.caption) = ("hover", "the pane mode hints that did not fit");
        scene.pointer = Some(Pointer {
            column: indicator,
            open: true,
        });
        script.show(&scene, 3600);
    }
    scene.mode = InputMode::Normal;
    scene.pointer = None;
    (scene.key, scene.caption) = ("Esc", "back to normal mode");
    script.show(&scene, 2000);
    script.animation()
}

/// The bar in one combination of themes after another. It stands still on
/// the first, the base themes.
pub(super) fn themes() -> Animation {
    let mut script = Script::default();
    let mut scene = Scene::new("");
    scene.right = RightPane::Shell { typed: "" };
    scene.name_look = true;
    for look in &THEME_TOUR {
        scene.look = look;
        script.show(&scene, 1600);
    }
    script.animation()
}

/// A nested session in the right pane: the bar shows the keys of the session
/// that has the keyboard, and the host's chrome fades while it does not. It
/// stands still descended into the nested session.
pub(super) fn nested() -> Animation {
    use InputMode::{Normal, Pane};
    let mut script = Script::default();
    let mut scene = Scene::new("a host session with a shell in the right pane");
    scene.right = RightPane::Shell { typed: "" };
    script.show(&scene, 3000);
    scene.right = RightPane::Shell {
        typed: "zellij attach work",
    };
    scene.caption = "start a nested session in the right pane";
    script.show(&scene, 2800);
    scene.right = RightPane::Nested {
        mode: Normal,
        descended: true,
    };
    scene.caption = "descended into it: the bar shows its keys";
    script.show_still(&scene, 4000);
    scene.right = RightPane::Nested {
        mode: Pane,
        descended: true,
    };
    (scene.key, scene.caption) = ("Ctrl p", "the nested session's pane mode");
    script.show(&scene, 3200);
    scene.right = RightPane::Nested {
        mode: Normal,
        descended: false,
    };
    (scene.key, scene.caption) = ("", "back in the host session: its own keys again");
    script.show(&scene, 3400);
    script.animation()
}
