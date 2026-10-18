//! The images in the README and the docs, drawn by the code that draws the
//! bar and the settings screen.
//!
//! Every image is a grid of terminal cells filled in from the ANSI the bar
//! prints, or from the lines `settings::view` lays out for the settings
//! screen, and written out as an SVG. Nothing here needs a terminal or a
//! running Zellij, and the output is the same on every machine: the cell size
//! is fixed, the numbers are integers, and each character is placed at its own
//! column so the font a viewer falls back to cannot shift anything.
//!
//! `make images` rewrites `docs/img/*.svg`. Without `ZJHINTS_WRITE_IMAGES`
//! set, the same test compares what it draws with the files, so `cargo test`
//! in CI fails when a change to the bar leaves an image out of date.

use crate::mouse::{self, PreviewPopup};
use crate::settings::editor::{Editor, Field, Focus};
use crate::settings::view::{self, FormatView, Layout, Line, Rgb, FOOTER_LINES};
use crate::{char_width, AnsiParser, AnsiSegment, ClickMap, State};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use zellij_tile::prelude::{
    InputMode, KeybindsVec, ModeInfo, Mouse, PaletteColor, PluginAliasEntry, PluginEntry, Styling,
};
use zellij_utils::input::config::Config;
use zellij_utils::setup::get_default_themes;

/// Set to rewrite the images instead of comparing against them.
const WRITE_ENV: &str = "ZJHINTS_WRITE_IMAGES";

/// The size of one terminal cell in pixels, and of the margin around the grid.
const CELL_WIDTH: usize = 9;
const CELL_HEIGHT: usize = 18;
const MARGIN_X: usize = 14;
const MARGIN_Y: usize = 12;
/// Where the text sits below the top of its cell.
const BASELINE: usize = 13;
const FONT_SIZE: usize = 15;
const FONT_FAMILY: &str =
    "ui-monospace, SFMono-Regular, Menlo, Consolas, 'DejaVu Sans Mono', monospace";

/// The Powerline right-pointing arrow. It lives in a Nerd Font's private use
/// area, which a viewer is unlikely to have, so it is drawn as a triangle.
const POWERLINE_ARROW: char = '\u{e0b0}';

/// Marks the second column of a character two columns wide.
const COVERED: char = '\0';

/// The colors of the terminal the images are drawn in: Catppuccin Mocha, the
/// same palette the images use for the `zellij` theme (see `palette`).
const TERMINAL_BACKGROUND: Rgb = (0x1e, 0x1e, 0x2e);
const TERMINAL_FOREGROUND: Rgb = (0xcd, 0xd6, 0xf4);
const ANSI_COLORS: [Rgb; 16] = [
    (0x45, 0x47, 0x5a),
    (0xf3, 0x8b, 0xa8),
    (0xa6, 0xe3, 0xa1),
    (0xf9, 0xe2, 0xaf),
    (0x89, 0xb4, 0xfa),
    (0xf5, 0xc2, 0xe7),
    (0x94, 0xe2, 0xd5),
    (0xba, 0xc2, 0xde),
    (0x58, 0x5b, 0x70),
    (0xf3, 0x8b, 0xa8),
    (0xa6, 0xe3, 0xa1),
    (0xf9, 0xe2, 0xaf),
    (0x89, 0xb4, 0xfa),
    (0xf5, 0xc2, 0xe7),
    (0x94, 0xe2, 0xd5),
    (0xa6, 0xad, 0xc8),
];

/// How much of the text color survives `dim`, in percent.
const DIM_PERCENT: u32 = 60;

/// The color of an xterm 256-color index.
fn xterm_color(index: u8) -> Rgb {
    match index {
        0..=15 => ANSI_COLORS[usize::from(index)],
        16..=231 => {
            let cube = index - 16;
            let level = |step: u8| if step == 0 { 0 } else { 55 + 40 * step };
            (level(cube / 36), level(cube / 6 % 6), level(cube % 6))
        }
        232..=255 => {
            let gray = 8 + 10 * (index - 232);
            (gray, gray, gray)
        }
    }
}

fn rgb_of(color: PaletteColor) -> Rgb {
    match color {
        PaletteColor::Rgb(rgb) => rgb,
        PaletteColor::EightBit(index) => xterm_color(index),
    }
}

/// The text attributes in force at some point of an ANSI string.
#[derive(Clone, Copy, Default, PartialEq)]
struct Pen {
    foreground: Option<Rgb>,
    background: Option<Rgb>,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    reverse: bool,
}

impl Pen {
    fn colored(foreground: Rgb) -> Self {
        Pen {
            foreground: Some(foreground),
            ..Pen::default()
        }
    }

    /// Take in an SGR escape sequence, such as `\x1b[1;38;5;75m`.
    fn apply(&mut self, sequence: &str) {
        let parameters = sequence.trim_start_matches("\x1b[").trim_end_matches('m');
        // An empty parameter is a zero, so `\x1b[m` resets.
        let mut codes = parameters
            .split(';')
            .map(|code| code.parse::<u8>().unwrap_or(0));
        while let Some(code) = codes.next() {
            match code {
                0 => *self = Pen::default(),
                1 => self.bold = true,
                2 => self.dim = true,
                3 => self.italic = true,
                4 => self.underline = true,
                7 => self.reverse = true,
                9 => self.strike = true,
                22 => {
                    self.bold = false;
                    self.dim = false;
                }
                23 => self.italic = false,
                24 => self.underline = false,
                27 => self.reverse = false,
                29 => self.strike = false,
                30..=37 => self.foreground = Some(ANSI_COLORS[usize::from(code - 30)]),
                38 => self.foreground = extended_color(&mut codes),
                39 => self.foreground = None,
                40..=47 => self.background = Some(ANSI_COLORS[usize::from(code - 40)]),
                48 => self.background = extended_color(&mut codes),
                49 => self.background = None,
                90..=97 => self.foreground = Some(ANSI_COLORS[usize::from(code - 90) + 8]),
                100..=107 => self.background = Some(ANSI_COLORS[usize::from(code - 100) + 8]),
                // Blink, hidden and the rest have no place in a still image.
                _ => {}
            }
        }
    }

    /// What a cell with this pen looks like once reverse video and dimming
    /// are applied.
    fn look(&self) -> Look {
        let (mut foreground, mut background) = (
            self.foreground.unwrap_or(TERMINAL_FOREGROUND),
            self.background,
        );
        if self.reverse {
            let swapped = background.unwrap_or(TERMINAL_BACKGROUND);
            background = Some(foreground);
            foreground = swapped;
        }
        if self.dim {
            foreground = mix(foreground, background.unwrap_or(TERMINAL_BACKGROUND));
        }
        Look {
            foreground,
            background,
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
            strike: self.strike,
        }
    }
}

/// The color after `38;` or `48;`: `5;<index>` or `2;<red>;<green>;<blue>`.
fn extended_color(codes: &mut impl Iterator<Item = u8>) -> Option<Rgb> {
    match codes.next()? {
        5 => Some(xterm_color(codes.next()?)),
        2 => Some((codes.next()?, codes.next()?, codes.next()?)),
        _ => None,
    }
}

/// `foreground` faded toward `background`.
fn mix(foreground: Rgb, background: Rgb) -> Rgb {
    let blend = |front: u8, back: u8| {
        ((u32::from(front) * DIM_PERCENT + u32::from(back) * (100 - DIM_PERCENT)) / 100) as u8
    };
    (
        blend(foreground.0, background.0),
        blend(foreground.1, background.1),
        blend(foreground.2, background.2),
    )
}

/// How a cell is drawn.
#[derive(Clone, Copy, PartialEq)]
struct Look {
    foreground: Rgb,
    /// `None` leaves the terminal's own background showing.
    background: Option<Rgb>,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
}

#[derive(Clone, Copy)]
struct Cell {
    character: char,
    look: Look,
}

/// A terminal screen of fixed size, and the SVG it comes out as.
struct Screen {
    rows: Vec<Vec<Cell>>,
}

impl Screen {
    fn new(rows: usize, columns: usize) -> Self {
        let blank = Cell {
            character: ' ',
            look: Pen::default().look(),
        };
        Screen {
            rows: vec![vec![blank; columns]; rows],
        }
    }

    fn columns(&self) -> usize {
        self.rows.first().map_or(0, Vec::len)
    }

    /// Put `character` at a spot of the grid, if it fits, and say how many
    /// columns it took.
    fn put(&mut self, row: usize, column: usize, character: char, look: Look) -> usize {
        let width = char_width(character, false);
        let Some(cells) = self.rows.get_mut(row) else {
            return width;
        };
        if width > 0 && column + width <= cells.len() {
            cells[column] = Cell { character, look };
            if width == 2 {
                cells[column + 1] = Cell {
                    character: COVERED,
                    look,
                };
            }
        }
        width
    }

    /// Print an ANSI string from a spot of the grid, as a terminal would.
    fn print(&mut self, row: usize, column: usize, ansi: &str) {
        let mut pen = Pen::default();
        let mut column = column;
        let mut parser = AnsiParser::new(ansi);
        while let Some(segment) = parser.next_segment() {
            match segment {
                AnsiSegment::EscapeSequence(sequence) => pen.apply(&sequence),
                AnsiSegment::VisibleChar(character) => {
                    column += self.put(row, column, character, pen.look());
                }
            }
        }
    }

    /// Print `text` from a spot of the grid, each character in the pen
    /// `pen_of` gives for its index.
    fn print_with(&mut self, row: usize, column: usize, text: &str, pen_of: impl Fn(usize) -> Pen) {
        let mut column = column;
        for (index, character) in text.chars().enumerate() {
            column += self.put(row, column, character, pen_of(index).look());
        }
    }

    /// Paint `columns` of a row with a background, as a selected row is.
    fn fill(&mut self, row: usize, columns: std::ops::Range<usize>, background: Rgb) {
        let Some(cells) = self.rows.get_mut(row) else {
            return;
        };
        for cell in cells.iter_mut().take(columns.end).skip(columns.start) {
            cell.look.background = Some(background);
        }
    }

    /// The screen as an SVG, titled `title` for the viewers that read one.
    fn to_svg(&self, title: &str) -> String {
        let width = self.columns() * CELL_WIDTH + 2 * MARGIN_X;
        let height = self.rows.len() * CELL_HEIGHT + 2 * MARGIN_Y;
        let mut svg = String::new();
        let _ = writeln!(
            svg,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\" role=\"img\">"
        );
        let _ = writeln!(svg, "<title>{}</title>", escape(title));
        let _ = writeln!(
            svg,
            "<rect width=\"{width}\" height=\"{height}\" rx=\"8\" fill=\"{}\"/>",
            hex(TERMINAL_BACKGROUND)
        );
        let _ = writeln!(svg, "<g transform=\"translate({MARGIN_X} {MARGIN_Y})\">");
        let _ = writeln!(svg, "<g shape-rendering=\"crispEdges\">");
        for (row, cells) in self.rows.iter().enumerate() {
            self.write_backgrounds(&mut svg, row, cells);
        }
        let _ = writeln!(svg, "</g>");
        let _ = writeln!(
            svg,
            "<g font-family=\"{FONT_FAMILY}\" font-size=\"{FONT_SIZE}\" xml:space=\"preserve\">"
        );
        for (row, cells) in self.rows.iter().enumerate() {
            write_arrows(&mut svg, row, cells);
            write_text(&mut svg, row, cells);
        }
        let _ = writeln!(svg, "</g>");
        let _ = writeln!(svg, "</g>");
        let _ = writeln!(svg, "</svg>");
        svg
    }

    /// One rectangle for each stretch of a row with the same background.
    fn write_backgrounds(&self, svg: &mut String, row: usize, cells: &[Cell]) {
        let mut column = 0;
        while column < cells.len() {
            let Some(background) = cells[column].look.background else {
                column += 1;
                continue;
            };
            let start = column;
            while column < cells.len() && cells[column].look.background == Some(background) {
                column += 1;
            }
            let _ = writeln!(
                svg,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{CELL_HEIGHT}\" fill=\"{}\"/>",
                start * CELL_WIDTH,
                row * CELL_HEIGHT,
                (column - start) * CELL_WIDTH,
                hex(background)
            );
        }
    }
}

/// A triangle in place of each Powerline arrow of a row.
fn write_arrows(svg: &mut String, row: usize, cells: &[Cell]) {
    for (column, cell) in cells.iter().enumerate() {
        if cell.character != POWERLINE_ARROW {
            continue;
        }
        let (left, top) = (column * CELL_WIDTH, row * CELL_HEIGHT);
        let _ = writeln!(
            svg,
            "<path d=\"M{left} {top}L{} {}L{left} {}Z\" fill=\"{}\"/>",
            left + CELL_WIDTH,
            top + CELL_HEIGHT / 2,
            top + CELL_HEIGHT,
            hex(cell.look.foreground)
        );
    }
}

/// The text of a row: one `tspan` for each stretch in the same style, with an
/// x for every character so each lands on its own column.
fn write_text(svg: &mut String, row: usize, cells: &[Cell]) {
    let mut runs: Vec<(Look, Vec<(usize, char)>)> = Vec::new();
    for (column, cell) in cells.iter().enumerate() {
        let Look {
            underline, strike, ..
        } = cell.look;
        let invisible = cell.character == COVERED
            || cell.character == POWERLINE_ARROW
            || (cell.character == ' ' && !underline && !strike);
        if invisible {
            continue;
        }
        // The background is drawn on its own, so it does not split a run.
        let look = Look {
            background: None,
            ..cell.look
        };
        match runs.last_mut() {
            Some((last, characters)) if *last == look => characters.push((column, cell.character)),
            _ => runs.push((look, vec![(column, cell.character)])),
        }
    }
    if runs.is_empty() {
        return;
    }
    let _ = write!(svg, "<text y=\"{}\">", row * CELL_HEIGHT + BASELINE);
    for (look, characters) in runs {
        let positions: Vec<String> = characters
            .iter()
            .map(|(column, _)| (column * CELL_WIDTH).to_string())
            .collect();
        let _ = write!(
            svg,
            "<tspan x=\"{}\" fill=\"{}\"",
            positions.join(" "),
            hex(look.foreground)
        );
        if look.bold {
            let _ = write!(svg, " font-weight=\"bold\"");
        }
        if look.italic {
            let _ = write!(svg, " font-style=\"italic\"");
        }
        let decoration = match (look.underline, look.strike) {
            (true, true) => Some("underline line-through"),
            (true, false) => Some("underline"),
            (false, true) => Some("line-through"),
            (false, false) => None,
        };
        if let Some(decoration) = decoration {
            let _ = write!(svg, " text-decoration=\"{decoration}\"");
        }
        let text: String = characters.iter().map(|(_, character)| character).collect();
        let _ = write!(svg, ">{}</tspan>", escape(&text));
    }
    let _ = writeln!(svg, "</text>");
}

fn hex((red, green, blue): Rgb) -> String {
    format!("#{red:02x}{green:02x}{blue:02x}")
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The Zellij theme the images draw in. The `zellij` theme follows the active
/// Zellij theme, so an image of it needs one fixed.
fn palette() -> Styling {
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
fn default_keybinds() -> KeybindsVec {
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

fn configuration_of(options: &[(&str, &str)]) -> BTreeMap<String, String> {
    options
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// The bar in `mode`, `columns` wide, with `options` as its configuration,
/// and the click map of what it drew.
fn bar(options: &[(&str, &str)], mode: InputMode, columns: usize) -> (String, ClickMap) {
    let mut state = State::default();
    state.apply_configuration(configuration_of(options));
    state.pane_width = Some(columns);
    state.mode_info = ModeInfo {
        mode,
        base_mode: Some(InputMode::Normal),
        keybinds: default_keybinds(),
        style: zellij_tile::prelude::Style {
            colors: palette(),
            ..Default::default()
        },
        ..Default::default()
    };
    state.draw_bar()
}

/// How wide the bars are drawn, in columns.
const BAR_COLUMNS: usize = 120;

/// A screen holding one bar per entry of `options`, each under its caption.
fn bars(captioned: &[(&str, &[(&str, &str)])], mode: InputMode) -> Screen {
    let mut screen = Screen::new(captioned.len() * 3 - 1, BAR_COLUMNS);
    for (index, (caption, options)) in captioned.iter().enumerate() {
        let row = index * 3;
        screen.print_with(row, 0, caption, |_| Pen {
            dim: true,
            ..Pen::default()
        });
        screen.print(row + 1, 0, &bar(options, mode, BAR_COLUMNS).0);
    }
    screen
}

/// The settings screen, `rows` by `columns`, the way `SettingsScreen::render`
/// lays it out, with `edit` already typed into the alias and `select`
/// selected. The colors are the palette's, standing in for what Zellij's
/// `Text` draws with.
fn settings_screen(
    options: &[(&str, &str)],
    select: &str,
    edit: (&str, &str),
    rows: usize,
    columns: usize,
) -> Screen {
    let styling = palette();
    let colors = styling.text_unselected;
    let emphasis = |index: usize| {
        Pen::colored(rgb_of(match index {
            0 => colors.emphasis_0,
            1 => colors.emphasis_1,
            2 => colors.emphasis_2,
            _ => colors.emphasis_3,
        }))
    };
    let selected = Pen {
        foreground: Some(rgb_of(styling.text_selected.base)),
        background: Some(rgb_of(styling.text_selected.background)),
        ..Pen::default()
    };
    let dim = Pen {
        dim: true,
        ..Pen::default()
    };

    let mut editor = Editor::new(
        vec![PluginAliasEntry {
            name: "zjhints".to_string(),
            plugin: PluginEntry {
                location: "file:~/.local/share/zellij/plugins/zjhints.wasm".to_string(),
                cwd: None,
                configuration: configuration_of(options).into_iter().collect(),
            },
        }],
        None,
    )
    .expect("an alias loading zjhints");
    editor.submit(
        &Field::Value {
            key: edit.0.to_string(),
        },
        edit.1,
    );
    let (section, row) = editor.locate(select).expect("a key the screen lists");
    let index = editor
        .sections()
        .iter()
        .position(|candidate| *candidate == section)
        .expect("a section of the sidebar");
    editor.select_section(index);
    editor.select(row);
    editor.set_focus(Focus::List);

    let sections = editor.sections();
    let titles: Vec<&str> = sections.iter().map(|section| section.title()).collect();
    let layout = Layout::new(rows, columns, &titles);
    let sidebar = layout.sidebar.expect("room for the sidebar");
    let mut screen = Screen::new(rows, columns);

    let title = view::title(&editor, FormatView::Rendered);
    let unsaved = title.find('●').map(|byte| title[..byte].chars().count());
    screen.print_with(0, 0, &title, |index| match unsaved {
        Some(from) if index >= from => emphasis(1),
        _ => emphasis(2),
    });

    for (index, section) in sections.iter().enumerate() {
        let y = layout.body_y + index;
        let line = view::sidebar_line(&editor, *section, sidebar);
        let pen = if *section == editor.section() {
            match editor.focus {
                Focus::Sidebar => selected,
                Focus::List => emphasis(2),
            }
        } else {
            Pen::default()
        };
        draw_line(&mut screen, 0, y, &line, pen, emphasis(1));
    }
    for y in layout.body_y..layout.body_y + layout.body_height {
        screen.print_with(y, sidebar, "│", |_| dim);
    }

    let width = layout.list_width.saturating_sub(1);
    let row_lines = view::Rows::new(&editor, FormatView::Rendered);
    for (index, list_row) in editor.rows().iter().enumerate() {
        let y = layout.list_y + index;
        if y >= layout.list_y + layout.list_height {
            break;
        }
        let line = row_lines.line(list_row, width);
        let is_selected = index == editor.selected && editor.focus == Focus::List;
        let pen = if is_selected {
            selected
        } else {
            Pen::default()
        };
        if is_selected {
            screen.fill(
                y,
                layout.list_x..layout.list_x + width,
                selected.background.unwrap(),
            );
        }
        draw_line(&mut screen, layout.list_x, y, &line, pen, emphasis(1));
        if let Some((column, ansi)) = line.styled.as_ref().filter(|_| !is_selected) {
            screen.print(y, layout.list_x + column, ansi);
        }
        if let Some((column, color)) = line.swatch {
            screen.print(y, layout.list_x + column, &view::swatch_ansi(color));
        }
    }

    let footer = rows - FOOTER_LINES;
    screen.print_with(footer, 0, &view::describe(&editor), |_| dim);
    let mut column = 0;
    for (position, (key, action)) in
        view::help_keys(editor.focus, layout.is_collapsed(), FormatView::Rendered)
            .iter()
            .enumerate()
    {
        if position > 0 {
            column += 2;
        }
        screen.print_with(footer + 2, column, key, |_| emphasis(3));
        column += key.chars().count();
        screen.print_with(footer + 2, column, &format!(" {action}"), |_| {
            Pen::default()
        });
        column += action.chars().count() + 1;
    }
    screen
}

/// A settings line in `pen`, its unset part dimmed and its edited marker in
/// the `marker` pen's color.
fn draw_line(screen: &mut Screen, x: usize, y: usize, line: &Line, pen: Pen, marker: Pen) {
    screen.print_with(y, x, &line.text, |index| {
        if line
            .edited
            .as_ref()
            .is_some_and(|range| range.contains(&index))
        {
            Pen {
                foreground: marker.foreground,
                ..pen
            }
        } else if line
            .dim
            .as_ref()
            .is_some_and(|range| range.contains(&index))
        {
            Pen { dim: true, ..pen }
        } else {
            pen
        }
    });
}

/// The bar with the hover preview of its last hint or marker that has one open
/// below it, as the popup draws it.
fn hover_preview() -> Screen {
    let options = [("show_mode", "true"), ("mouse", "true")];
    let (line, map) = bar(&options, InputMode::Pane, BAR_COLUMNS);
    // The last one is the drop indicator's, which lists the most.
    let preview = (0..BAR_COLUMNS)
        .rev()
        .find_map(|column| mouse::hover_preview(&map, &Mouse::Hover(0, column)))
        .expect("a hint that opens a menu");
    let configuration = mouse::preview_configuration("1-1", &preview.entries, false);
    let popup = PreviewPopup::from_configuration(&configuration).expect("a preview popup");
    let (width, height) = mouse::preview_size(&preview.entries, false);

    let mut screen = Screen::new(1 + height, BAR_COLUMNS);
    screen.print(0, 0, &line);
    // A popup hangs from the hint it previews, shifted left when that would
    // put it off the screen.
    let left = preview.columns.start.min(BAR_COLUMNS.saturating_sub(width));
    for (offset, popup_line) in popup.lines(height, width).iter().enumerate() {
        screen.print(1 + offset, left, popup_line);
    }
    screen
}

/// The alias the settings screens show: the colors and formats of a theme
/// written out as a user would, so the rows have values and swatches.
const SETTINGS_OPTIONS: [(&str, &str); 11] = [
    ("theme", "catppuccin-mocha"),
    ("color_bg", "#181825"),
    ("color_fg", "#cdd6f4"),
    ("color_surface", "#313244"),
    ("color_muted", "#6c7086"),
    ("color_accent", "#cba6f7"),
    ("color_red", "#f38ba8"),
    ("key_format", "#[fg=$bg,bg=$accent,bold] {key} "),
    ("desc_format", "#[fg=$fg,bg=$surface] {desc} "),
    ("hint_spacer", "#[fg=$muted] │ "),
    ("key_format_quit", "#[fg=$bg,bg=$red,bold] {key} "),
];

/// Every image as the name of its file and its SVG.
fn images() -> Vec<(&'static str, String)> {
    let show_mode = [("show_mode", "true")];
    let themes: Vec<(&str, Vec<(&str, &str)>)> = [
        "zellij",
        "catppuccin-mocha",
        "catppuccin-latte",
        "dracula",
        "nord",
        "gruvbox-dark",
        "tokyo-night",
        "powerline",
    ]
    .iter()
    .map(|theme| (*theme, vec![("show_mode", "true"), ("theme", *theme)]))
    .collect();
    let captioned: Vec<(&str, &[(&str, &str)])> = themes
        .iter()
        .map(|(theme, options)| (*theme, options.as_slice()))
        .collect();
    vec![
        (
            "bar-normal.svg",
            bars(&[("normal mode", &show_mode)], InputMode::Normal)
                .to_svg("The zjhints bar in normal mode, with the mode shown"),
        ),
        (
            "bar-locked.svg",
            bars(&[("locked mode", &show_mode)], InputMode::Locked)
                .to_svg("The zjhints bar in locked mode, with the mode shown"),
        ),
        (
            "themes.svg",
            bars(&captioned, InputMode::Normal).to_svg("The zjhints bar in each built-in theme"),
        ),
        (
            "settings-styling.svg",
            settings_screen(
                &SETTINGS_OPTIONS,
                "hint_spacer",
                ("desc_format", "#[fg=$fg,bg=$surface,italic] {desc} "),
                19,
                100,
            )
            .to_svg("The settings screen on the Styling group, with formats shown rendered"),
        ),
        (
            "settings-colors.svg",
            settings_screen(
                &SETTINGS_OPTIONS,
                "color_accent",
                ("color_accent", "#f5c2e7"),
                19,
                100,
            )
            .to_svg("The settings screen on the Colors group, with a swatch beside each color"),
        ),
        (
            "mouse-preview.svg",
            hover_preview().to_svg("The hover preview of a hint, listing what clicking it offers"),
        ),
    ]
}

/// Where the images live.
fn image_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/img")
}

/// The SVGs in `directory`, by file name.
fn svgs_in(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".svg"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The look of the first cell after printing `ansi` on a fresh screen.
    fn look_of(ansi: &str) -> Look {
        let mut screen = Screen::new(1, 4);
        screen.print(0, 0, ansi);
        screen.rows[0][0].look
    }

    #[test]
    fn colors_come_out_of_every_form_of_sgr() {
        assert_eq!(look_of("\x1b[38;5;196mx").foreground, (255, 0, 0));
        assert_eq!(look_of("\x1b[38;5;244mx").foreground, (128, 128, 128));
        assert_eq!(look_of("\x1b[38;2;1;2;3mx").foreground, (1, 2, 3));
        assert_eq!(look_of("\x1b[31mx").foreground, ANSI_COLORS[1]);
        assert_eq!(look_of("\x1b[92mx").foreground, ANSI_COLORS[10]);
        assert_eq!(look_of("\x1b[48;5;21mx").background, Some((0, 0, 255)));
        assert_eq!(look_of("\x1b[44mx").background, Some(ANSI_COLORS[4]));
    }

    #[test]
    fn a_reset_clears_everything_set_before_it() {
        let look = look_of("\x1b[1;3;4;38;2;1;2;3;48;2;4;5;6m\x1b[0mx");
        assert_eq!(look.foreground, TERMINAL_FOREGROUND);
        assert_eq!(look.background, None);
        assert!(!look.bold && !look.italic && !look.underline);
        assert!(!look_of("\x1b[1m\x1b[mx").bold);
    }

    #[test]
    fn reverse_swaps_the_colors_and_dim_fades_the_text() {
        let reversed = look_of("\x1b[7;38;2;10;20;30mx");
        assert_eq!(reversed.background, Some((10, 20, 30)));
        assert_eq!(reversed.foreground, TERMINAL_BACKGROUND);
        let dimmed = look_of("\x1b[2mx");
        assert!(dimmed.foreground.0 < TERMINAL_FOREGROUND.0);
        assert!(dimmed.foreground.0 > TERMINAL_BACKGROUND.0);
    }

    #[test]
    fn a_background_is_one_rectangle_across_changes_of_text_color() {
        let mut screen = Screen::new(1, 4);
        screen.print(0, 0, "\x1b[48;2;1;2;3ma\x1b[31mb\x1b[0m");
        let svg = screen.to_svg("t");
        assert_eq!(svg.matches("<rect x=").count(), 1);
        assert!(svg.contains("<rect x=\"0\" y=\"0\" width=\"18\""));
        assert_eq!(svg.matches("<tspan").count(), 2);
    }

    #[test]
    fn each_character_is_placed_on_its_own_column() {
        let mut screen = Screen::new(1, 6);
        screen.print(0, 1, "a b");
        let svg = screen.to_svg("t");
        assert!(svg.contains("<tspan x=\"9 27\""), "{svg}");
        assert!(svg.contains(">ab</tspan>"));
    }

    #[test]
    fn a_wide_character_covers_two_columns() {
        let mut screen = Screen::new(1, 4);
        screen.print(0, 0, "\x1b[44m世x");
        assert_eq!(screen.rows[0][1].character, COVERED);
        let svg = screen.to_svg("t");
        assert!(svg.contains("<tspan x=\"0 18\""));
        assert!(svg.contains("width=\"27\""));
    }

    #[test]
    fn the_powerline_arrow_is_drawn_as_a_triangle() {
        let mut screen = Screen::new(1, 2);
        screen.print(0, 0, "\u{e0b0}");
        let svg = screen.to_svg("t");
        assert!(svg.contains("<path d=\"M0 0L9 9L0 18Z\""));
        assert!(!svg.contains("<tspan"));
    }

    #[test]
    fn markup_in_the_text_is_escaped() {
        let mut screen = Screen::new(1, 3);
        screen.print(0, 0, "<&>");
        assert!(screen.to_svg("a & b").contains("&lt;&amp;&gt;"));
        assert!(screen.to_svg("a & b").contains("<title>a &amp; b</title>"));
    }

    #[test]
    fn drawing_the_images_twice_gives_the_same_bytes() {
        assert_eq!(images(), images());
    }

    #[test]
    fn the_images_in_the_docs_are_up_to_date() {
        let directory = image_directory();
        let images = images();
        if std::env::var_os(WRITE_ENV).is_some() {
            fs::create_dir_all(&directory).expect("create docs/img");
            for (name, svg) in &images {
                fs::write(directory.join(name), svg).expect("write an image");
            }
            for name in svgs_in(&directory) {
                if !images.iter().any(|(image, _)| *image == name) {
                    fs::remove_file(directory.join(name)).expect("remove an orphaned image");
                }
            }
            return;
        }
        let mut stale: Vec<String> = images
            .iter()
            .filter(|(name, svg)| {
                fs::read_to_string(directory.join(name)).ok().as_ref() != Some(svg)
            })
            .map(|(name, _)| name.to_string())
            .collect();
        stale.extend(
            svgs_in(&directory)
                .into_iter()
                .filter(|name| !images.iter().any(|(image, _)| image == name))
                .map(|name| format!("{name} (no longer generated)")),
        );
        assert!(
            stale.is_empty(),
            "docs/img is out of date: {}. Run `make images` and commit the result.",
            stale.join(", ")
        );
    }
}
