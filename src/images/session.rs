//! The session the README opens with: a Zellij session drawn by hand around
//! the real bar, played through the plugin's features a keypress at a time.
//!
//! Only the chrome is a stand-in. The tab bar, the pane frames and what runs
//! in the panes are drawn here to look like Zellij's, while the bar, its hover
//! preview and the settings screen come from the code that draws them in the
//! plugin, so the demo cannot show the plugin doing anything it does not.

use super::{
    bar_state, draw_pointer, hover_at, palette, rgb_of, settings_screen, Animation, Frame, Pen,
    Rgb, Screen, PILL_RIGHT, POWERLINE_ARROW, TERMINAL_BACKGROUND,
};
use crate::{ActiveTab, ClickMap, NestedGuest, TabFocus};
use zellij_tile::prelude::{InputMode, PaneId};

/// The terminal's size, and the rows under it that say what was pressed.
const ROWS: usize = 24;
const COLUMNS: usize = 120;
const CAPTION_ROWS: usize = 2;
/// The row the bar sits on, in a borderless pane of its own at the bottom.
const BAR_ROW: usize = ROWS - 1;
/// How narrow the terminal gets when it is resized.
const NARROWEST: usize = 66;

/// The settings screen's size inside its floating pane's frame.
const SETTINGS_ROWS: usize = 18;
const SETTINGS_COLUMNS: usize = 84;

/// The options of the alias the session loads zjhints from: Zellij's own
/// look, with the mode shown and the mouse on.
const BASE_OPTIONS: [(&str, &str); 2] = [("show_mode", "true"), ("mouse", "true")];

/// The theme the settings screen picks in the demo, one option at a time.
const THEME_EDITS: [(&str, &str); 3] = [
    ("format_theme", "pills"),
    ("color_theme", "dracula"),
    ("alias_theme", "unicode"),
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

/// Everything on screen at one step of the demo.
#[derive(Clone, Copy)]
struct Scene {
    columns: usize,
    mode: InputMode,
    /// How many of `THEME_EDITS` the bar has saved into its options.
    theme: usize,
    right: RightPane,
    settings: Option<Settings>,
    hover: bool,
    /// The key pressed for this step, if any, and what it did.
    key: &'static str,
    caption: &'static str,
}

impl Scene {
    fn options(&self) -> Vec<(&'static str, &'static str)> {
        let edits = self
            .settings
            .map_or(self.theme, |settings| settings.edits.max(self.theme));
        let mut options = BASE_OPTIONS.to_vec();
        options.extend_from_slice(&THEME_EDITS[..edits]);
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

/// A shell prompt at a spot of `area`, with `typed` after it and the cursor.
fn draw_prompt(screen: &mut Screen, row: usize, column: usize, typed: &str, chrome: &Chrome) {
    let column = paint(
        screen,
        row,
        column,
        "~/src/zjhints ",
        chrome.pen(chrome.path),
    );
    let column = paint(screen, row, column, "❯ ", chrome.pen(chrome.prompt));
    let column = paint(screen, row, column, typed, chrome.pen(chrome.text));
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
    let after = paint(
        screen,
        row,
        column,
        "~/src/zjhints ",
        chrome.pen(chrome.path),
    );
    let after = paint(screen, row, after, "❯ ", chrome.pen(chrome.prompt));
    paint(screen, row, after, "cargo test", chrome.pen(chrome.text));
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
    let (options, edits) = if settings.saved {
        let mut options = BASE_OPTIONS.to_vec();
        options.extend_from_slice(&THEME_EDITS[..settings.edits]);
        (options, Vec::new())
    } else {
        (
            BASE_OPTIONS.to_vec(),
            THEME_EDITS[..settings.edits].to_vec(),
        )
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

/// The hover preview of the bar's last hint that has one, the drop
/// indicator's, open over the bar with the pointer on that hint.
fn draw_hover(screen: &mut Screen, columns: usize, map: &ClickMap) {
    let hover = (0..columns)
        .rev()
        .find_map(|column| hover_at(map, column, columns))
        .expect("a hint that opens a menu");
    hover.draw_above(screen, BAR_ROW);
    draw_pointer(screen, BAR_ROW + 1, hover.hint.start);
}

/// The key pressed for a step, as a key cap, and what it did beside it.
fn draw_caption(screen: &mut Screen, scene: &Scene) {
    let styling = palette();
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
    paint(
        screen,
        row,
        column,
        scene.caption,
        Pen::colored(rgb_of(styling.text_unselected.base)),
    );
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
    if scene.hover {
        draw_hover(&mut screen, columns, &map);
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

/// The steps of the demo, each with how long it stays up.
fn script() -> Vec<(Scene, u32)> {
    use InputMode::{Locked, Normal, Pane, Tab};
    let mut steps = Vec::new();
    let mut scene = Scene {
        columns: COLUMNS,
        mode: Normal,
        theme: 0,
        right: RightPane::None,
        settings: None,
        hover: false,
        key: "",
        caption: "zjhints shows the keys of the mode you are in",
    };
    let mut step = |scene: &Scene, milliseconds: u32| steps.push((*scene, milliseconds));
    step(&scene, 3000);

    scene.mode = Pane;
    (scene.key, scene.caption) = ("Ctrl p", "pane mode: the bar follows");
    step(&scene, 2400);
    scene.mode = Normal;
    scene.right = RightPane::Shell { typed: "" };
    (scene.key, scene.caption) = ("r", "split right, and back to normal mode");
    step(&scene, 2400);
    scene.mode = Tab;
    (scene.key, scene.caption) = ("Ctrl t", "tab mode");
    step(&scene, 2000);
    scene.mode = Normal;
    (scene.key, scene.caption) = ("Esc", "back to normal mode");
    step(&scene, 1600);

    // The terminal narrows, and the bar drops hints to fit.
    (scene.key, scene.caption) = ("", "the terminal narrows: whole hints drop to fit");
    for columns in (NARROWEST..COLUMNS).step_by(6).rev() {
        scene.columns = columns;
        step(&scene, if columns == NARROWEST { 1400 } else { 250 });
    }
    scene.hover = true;
    (scene.key, scene.caption) = ("hover", "the ones that did not fit, a click away");
    step(&scene, 3600);
    scene.hover = false;
    (scene.key, scene.caption) = ("", "the terminal widens again");
    for columns in (NARROWEST..=COLUMNS).step_by(6).skip(1) {
        scene.columns = columns;
        step(&scene, if columns == COLUMNS { 1200 } else { 200 });
    }

    // The settings screen picks a theme, and the bar previews each change.
    scene.settings = Some(Settings {
        edits: 0,
        saved: false,
    });
    (scene.key, scene.caption) = ("Alt ,", "open the settings screen");
    step(&scene, 2400);
    let captions = [
        "format_theme \"pills\": the bar previews each change",
        "color_theme \"dracula\"",
        "alias_theme \"unicode\"",
    ];
    for (edits, caption) in captions.into_iter().enumerate() {
        scene.settings = Some(Settings {
            edits: edits + 1,
            saved: false,
        });
        (scene.key, scene.caption) = ("", caption);
        step(&scene, 2400);
    }
    scene.settings = Some(Settings {
        edits: THEME_EDITS.len(),
        saved: true,
    });
    scene.theme = THEME_EDITS.len();
    (scene.key, scene.caption) = ("Ctrl a", "save it to the config file");
    step(&scene, 2000);
    scene.settings = None;
    (scene.key, scene.caption) = ("Esc", "close the settings screen");
    step(&scene, 1600);

    // A nested session: the bar shows the keys of the session that has them.
    scene.right = RightPane::Shell {
        typed: "zellij attach work",
    };
    (scene.key, scene.caption) = ("", "start a nested session in the right pane");
    step(&scene, 2000);
    scene.right = RightPane::Nested {
        mode: Normal,
        descended: true,
    };
    (scene.key, scene.caption) = ("", "descended into it: the bar shows its keys");
    step(&scene, 3000);
    scene.right = RightPane::Nested {
        mode: Pane,
        descended: true,
    };
    (scene.key, scene.caption) = ("Ctrl p", "the nested session's pane mode");
    step(&scene, 2400);
    scene.right = RightPane::Nested {
        mode: Normal,
        descended: false,
    };
    (scene.key, scene.caption) = ("", "back in the host session");
    step(&scene, 2200);

    scene.mode = Locked;
    (scene.key, scene.caption) = ("Ctrl g", "locked mode: only the way out is shown");
    step(&scene, 2600);
    steps
}

/// The demo as an animation, standing still on the step with the settings
/// screen previewing a theme.
pub(super) fn demo() -> Animation {
    let steps = script();
    let still = steps
        .iter()
        .position(|(scene, _)| scene.settings.is_some_and(|settings| settings.edits == 2))
        .expect("a step with the settings screen open");
    let frames = steps
        .iter()
        .map(|(scene, milliseconds)| Frame::new(draw(scene), *milliseconds))
        .collect();
    Animation::new(frames, still)
}
