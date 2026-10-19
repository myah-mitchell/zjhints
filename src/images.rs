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
//! `make images` rewrites the SVGs in `docs/img` that this module draws, and
//! removes the ones it drew before but no longer does. It knows its own by
//! `GENERATED_MARK`, so an SVG made any other way is left alone. Without
//! `ZJHINTS_WRITE_IMAGES` set, the same test compares what it draws with the
//! files, so `cargo test` in CI fails when a change to the bar leaves an image
//! out of date.

use crate::mouse::{self, PreviewPopup};
use crate::settings::editor::{Editor, Field, Focus};
use crate::settings::view::{self, Control, FormatView, HelpState, Layout, Marker, Rgb, RowView};
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

/// The comment every SVG drawn here carries, which is how `make images` tells
/// its own files from ones made by hand.
const GENERATED_MARK: &str =
    "<!-- Drawn by src/images.rs: edit the code and run `make images`. -->";

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
/// The round caps the pills format theme draws at each end of a block.
const PILL_LEFT: char = '\u{e0b6}';
const PILL_RIGHT: char = '\u{e0b4}';

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

    /// The screen as an SVG, titled `title` for the viewers that read one.
    fn to_svg(&self, title: &str) -> String {
        let width = self.columns() * CELL_WIDTH + 2 * MARGIN_X;
        let height = self.rows.len() * CELL_HEIGHT + 2 * MARGIN_Y;
        let mut svg = String::new();
        let _ = writeln!(
            svg,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\" role=\"img\">"
        );
        let _ = writeln!(svg, "{GENERATED_MARK}");
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

/// A shape in place of each Powerline arrow and pill cap of a row, since few
/// fonts a reader has carry those glyphs.
fn write_arrows(svg: &mut String, row: usize, cells: &[Cell]) {
    for (column, cell) in cells.iter().enumerate() {
        let (left, top) = (column * CELL_WIDTH, row * CELL_HEIGHT);
        let (right, middle, bottom) = (left + CELL_WIDTH, top + CELL_HEIGHT / 2, top + CELL_HEIGHT);
        let (radius_x, radius_y) = (CELL_WIDTH, CELL_HEIGHT / 2);
        let path = match cell.character {
            POWERLINE_ARROW => format!("M{left} {top}L{right} {middle}L{left} {bottom}Z"),
            PILL_RIGHT => format!("M{left} {top}A{radius_x} {radius_y} 0 0 1 {left} {bottom}Z"),
            PILL_LEFT => format!("M{right} {top}A{radius_x} {radius_y} 0 0 0 {right} {bottom}Z"),
            _ => continue,
        };
        let _ = writeln!(
            svg,
            "<path d=\"{path}\" fill=\"{}\"/>",
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
            || matches!(cell.character, POWERLINE_ARROW | PILL_LEFT | PILL_RIGHT)
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
    state.zellij_palette = Some(palette());
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

/// The pens Zellij's widgets draw with, from the palette, standing in for
/// what the server paints `Text` and the widgets in.
struct WidgetPens {
    plain: Pen,
    dim: Pen,
    /// The text of the selected menu item, and of a focused dropdown's value.
    selected: Pen,
    /// A focused field's label and brackets.
    focused_label: Pen,
    focused_bracket: Pen,
    /// A toggle that is on.
    on: Pen,
    /// The menu's border.
    frame: Pen,
    emphasis: [Pen; 4],
}

impl WidgetPens {
    fn new() -> Self {
        let styling = palette();
        let colors = styling.text_unselected;
        let bold = |pen: Pen| Pen { bold: true, ..pen };
        let plain = Pen::colored(rgb_of(colors.base));
        WidgetPens {
            plain,
            dim: Pen { dim: true, ..plain },
            selected: bold(Pen {
                foreground: Some(rgb_of(styling.text_selected.base)),
                background: Some(rgb_of(styling.text_selected.background)),
                ..Pen::default()
            }),
            focused_label: bold(Pen::colored(rgb_of(colors.emphasis_0))),
            focused_bracket: bold(Pen::colored(rgb_of(styling.frame_selected.base))),
            on: bold(Pen::colored(rgb_of(colors.emphasis_2))),
            frame: Pen::colored(rgb_of(
                styling
                    .frame_unselected
                    .map_or(styling.list_unselected.base, |frame| frame.base),
            )),
            emphasis: [
                colors.emphasis_0,
                colors.emphasis_1,
                colors.emphasis_2,
                colors.emphasis_3,
            ]
            .map(|color| Pen::colored(rgb_of(color))),
        }
    }
}

/// `text` cut or padded to exactly `width` columns.
fn fit(text: &str, width: usize) -> String {
    let cut = view::truncate(text, width);
    let pad = width.saturating_sub(view::columns(&cut));
    format!("{cut}{}", " ".repeat(pad))
}

/// A button as Zellij draws one, `[ label ]`, its key in the key color.
/// The columns it took.
fn print_button(
    screen: &mut Screen,
    y: usize,
    x: usize,
    key: &str,
    action: &str,
    pens: &WidgetPens,
) -> usize {
    let label = format!("[ {key} {action} ]");
    let key_columns = 2..2 + key.chars().count();
    screen.print_with(y, x, &label, |index| {
        if key_columns.contains(&index) {
            pens.emphasis[3]
        } else {
            pens.plain
        }
    });
    view::columns(&label)
}

/// Buttons in a row from `x`, leaving out any that would pass `end`, as the
/// screen places them.
fn print_buttons(
    screen: &mut Screen,
    y: usize,
    mut x: usize,
    end: usize,
    buttons: &[(&str, &str)],
    pens: &WidgetPens,
) {
    for (key, action) in buttons {
        let width = view::columns(&format!("{key} {action}")) + 4;
        if x + width > end {
            continue;
        }
        x += print_button(screen, y, x, key, action, pens) + 1;
    }
}

/// One row of the list as the widgets draw it, with its swatch and marker.
#[allow(clippy::too_many_arguments)]
fn print_row(
    screen: &mut Screen,
    y: usize,
    x: usize,
    row: &RowView,
    (label_width, value_width): (usize, usize),
    width: usize,
    focused: bool,
    pens: &WidgetPens,
) {
    let element_width = label_width + value_width;
    let label = fit(
        &view::truncate(&row.label, label_width.saturating_sub(2)),
        label_width,
    );
    let (label_pen, bracket_pen, value_pen) = if focused {
        let bold = Pen {
            bold: true,
            ..pens.plain
        };
        (pens.focused_label, pens.focused_bracket, bold)
    } else {
        (pens.plain, pens.plain, pens.plain)
    };
    let field_x = x + label_width;
    match &row.control {
        Control::Heading(title) => {
            screen.print_with(y, x, &view::truncate(title, width), |_| pens.emphasis[2]);
            return;
        }
        Control::Button(text) => {
            let pen = if focused { pens.selected } else { pens.plain };
            screen.print_with(y, x, &format!("[ {text} ]"), |_| pen);
            return;
        }
        Control::Toggle(on) => {
            screen.print_with(y, x, &label, |_| label_pen);
            let inner = format!(" {} ", fit(if *on { "on" } else { "off" }, value_width - 4));
            let inner_pen = match (*on, focused) {
                (true, _) => pens.on,
                (false, true) => value_pen,
                (false, false) => pens.plain,
            };
            screen.print_with(y, field_x, "[", |_| bracket_pen);
            screen.print_with(y, field_x + 1, &inner, |_| inner_pen);
            screen.print_with(y, field_x + value_width - 1, "]", |_| bracket_pen);
        }
        Control::Dropdown(options, chosen) => {
            screen.print_with(y, x, &label, |_| label_pen);
            let value = options.get(*chosen).map_or("", String::as_str);
            let value_pen = if focused { pens.selected } else { pens.plain };
            let inner = format!(" {} ▾ ", fit(value, value_width - 6));
            screen.print_with(y, field_x, "[", |_| bracket_pen);
            screen.print_with(y, field_x + 1, &inner, |index| {
                if focused && index == inner.chars().count() - 2 {
                    Pen {
                        foreground: pens.focused_bracket.foreground,
                        ..value_pen
                    }
                } else {
                    value_pen
                }
            });
            screen.print_with(y, field_x + value_width - 1, "]", |_| bracket_pen);
        }
        Control::Text { text, placeholder } => {
            screen.print_with(y, x, &label, |_| label_pen);
            let text_pen = if *placeholder {
                Pen {
                    dim: true,
                    italic: true,
                    bold: false,
                    ..value_pen
                }
            } else {
                value_pen
            };
            let room = value_width - 4;
            screen.print_with(y, field_x, "[ ", |index| {
                if index == 0 {
                    bracket_pen
                } else {
                    value_pen
                }
            });
            screen.print_with(y, field_x + 2, &fit(text, room), |_| text_pen);
            screen.print_with(y, field_x + value_width - 2, " ]", |index| {
                if index == 1 {
                    bracket_pen
                } else {
                    value_pen
                }
            });
            if let Some(ansi) = &row.styled {
                screen.print(y, field_x + 2, &view::take_visible(ansi, room));
            }
        }
    }
    let (swatch_x, marker_x) = view::trailer_columns(row, x + element_width, x + width);
    if let Some(color) = row.swatch {
        screen.print(y, swatch_x, &view::swatch_ansi(color));
    }
    let marker_pen = match row.marker {
        Marker::Unsaved => pens.emphasis[1],
        _ => pens.dim,
    };
    screen.print_with(y, marker_x, row.marker.text(), |_| marker_pen);
}

/// The settings screen, `rows` by `columns`, the way `SettingsScreen::render`
/// lays it out, with `edit` made and left unsaved and `select` selected.
fn settings_screen(
    options: &[(&str, &str)],
    select: &str,
    edit: (&str, &str),
    rows: usize,
    columns: usize,
) -> Screen {
    let pens = WidgetPens::new();
    let alias = vec![PluginAliasEntry {
        name: "zjhints".to_string(),
        plugin: PluginEntry {
            location: "file:~/.local/share/zellij/plugins/zjhints.wasm".to_string(),
            cwd: None,
            configuration: configuration_of(options).into_iter().collect(),
        },
    }];
    let mut editor = Editor::new(alias.clone(), alias, None).expect("an alias loading zjhints");
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
        .expect("a section of the menu");
    editor.select_section(index);
    editor.select(row);

    let items = view::menu_items(&editor);
    let layout = Layout::new(rows, columns, &items, None).expect("room for the screen");
    let mut screen = Screen::new(rows, columns);

    // The header.
    let file = format!("{}~/.config/zellij/config.kdl", view::FILE_PREFIX);
    screen.print_with(0, layout.x, &file, |_| pens.plain);
    let unsaved = view::unsaved_text(editor.unsaved_count());
    let count = format!("{unsaved} · ");
    let unsaved_columns = unsaved.chars().count();
    screen.print_with(2, layout.x, &count, |index| {
        if index < unsaved_columns && editor.unsaved_count() > 0 {
            pens.emphasis[1]
        } else {
            pens.plain
        }
    });
    let x = layout.x + view::columns(&count);
    let buttons = view::header_buttons(FormatView::Rendered).map(|(key, action, _)| (key, action));
    print_buttons(&mut screen, 2, x, layout.x + layout.width, &buttons, &pens);

    // The menu, as Zellij's side menu draws it.
    for y in layout.body_y..layout.body_y + layout.body_height {
        let item = items.get(y - layout.body_y);
        let text = fit(
            &item.map_or(String::new(), |item| format!(" {item}")),
            layout.menu_width - 1,
        );
        let is_selected = y - layout.body_y == editor.section_index();
        let pen = if is_selected {
            pens.selected
        } else {
            pens.plain
        };
        let marker_from = item
            .filter(|item| item.ends_with(" ●"))
            .map(|item| item.chars().count());
        screen.print_with(y, layout.x, &text, |index| match marker_from {
            Some(column) if index == column => Pen {
                foreground: pens.emphasis[1].foreground,
                ..pen
            },
            _ => pen,
        });
        screen.print_with(y, layout.x + layout.menu_width - 1, "│", |_| pens.frame);
    }

    // The rows.
    let width = layout.content_width - 1;
    let rows_view = view::Rows::new(&editor, FormatView::Rendered);
    let views: Vec<RowView> = editor
        .rows()
        .iter()
        .map(|row| rows_view.view(row))
        .collect();
    let widths = view::field_widths(&views, width);
    for (index, row) in views.iter().enumerate().take(layout.list_height) {
        let focused = index == editor.selected && editor.focus == Focus::List;
        print_row(
            &mut screen,
            layout.body_y + index,
            layout.content_x,
            row,
            widths,
            width,
            focused,
            &pens,
        );
    }

    // The footer.
    if let Some(description) = view::describe(&editor) {
        let description = view::truncate(&description, layout.width);
        screen.print_with(layout.footer_y, layout.x, &description, |_| {
            pens.emphasis[0]
        });
    }
    let state = HelpState::Browsing {
        focus: editor.focus,
        text_row: views.get(editor.selected).is_some_and(RowView::is_text),
        searching: false,
    };
    let end = layout.x + layout.width;
    let hints = view::help_hints(state);
    print_buttons(
        &mut screen,
        layout.footer_y + 2,
        layout.x,
        end,
        &hints,
        &pens,
    );
    screen
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
    ("color_theme", "catppuccin-mocha"),
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
    // Each kind of theme shown on its own, the others held still.
    let rows =
        |option: &'static str, names: &[&'static str], rest: &[(&'static str, &'static str)]| {
            names
                .iter()
                .map(|name| {
                    let mut options = vec![("show_mode", "true"), (option, *name)];
                    options.extend_from_slice(rest);
                    (*name, options)
                })
                .collect::<Vec<(&str, Vec<(&str, &str)>)>>()
        };
    let color_rows = rows(
        "color_theme",
        &crate::theme::COLOR_CHOICES,
        &[("format_theme", "flat")],
    );
    let format_rows = rows(
        "format_theme",
        &crate::theme::FORMAT_CHOICES,
        &[("color_theme", "catppuccin-mocha")],
    );
    // The nerd-font symbols are left out: a reader's font rarely has them,
    // and unlike the Powerline shapes they are too many to draw by hand.
    let alias_rows = rows(
        "alias_theme",
        &["zellij", "ascii", "unicode"],
        &[
            ("color_theme", "catppuccin-mocha"),
            ("format_theme", "pills"),
        ],
    );
    fn captioned<'a>(
        rows: &'a [(&'a str, Vec<(&'a str, &'a str)>)],
    ) -> Vec<(&'a str, &'a [(&'a str, &'a str)])> {
        rows.iter()
            .map(|(name, options)| (*name, options.as_slice()))
            .collect()
    }
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
            "color-themes.svg",
            bars(&captioned(&color_rows), InputMode::Normal)
                .to_svg("The zjhints bar in each color theme, drawn with the flat format theme"),
        ),
        (
            "format-themes.svg",
            bars(&captioned(&format_rows), InputMode::Normal)
                .to_svg("The zjhints bar in each format theme, in catppuccin-mocha colors"),
        ),
        (
            "alias-themes.svg",
            bars(&captioned(&alias_rows), InputMode::Normal)
                .to_svg("The zjhints bar in the zellij, ascii and unicode alias themes, drawn as catppuccin-mocha pills"),
        ),
        (
            "settings-styling.svg",
            settings_screen(
                &SETTINGS_OPTIONS,
                "hint_spacer",
                ("desc_format", "#[fg=$fg,bg=$surface,italic] {desc} "),
                22,
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
                22,
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

/// The SVGs in `directory` that this module drew, by file name.
fn generated_svgs_in(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".svg"))
                .filter(|name| {
                    fs::read_to_string(directory.join(name)).is_ok_and(|svg| is_generated(&svg))
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Whether `svg` was drawn here. The mark sits in the first lines, so a
/// hand-made SVG that quotes it further down is not mistaken for one.
fn is_generated(svg: &str) -> bool {
    svg.lines().take(3).any(|line| line == GENERATED_MARK)
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
    fn the_pill_caps_are_drawn_as_half_circles() {
        let mut screen = Screen::new(1, 2);
        screen.print(0, 0, "\u{e0b6}\u{e0b4}");
        let svg = screen.to_svg("t");
        assert!(svg.contains("<path d=\"M9 0A9 9 0 0 0 9 18Z\""));
        assert!(svg.contains("<path d=\"M9 0A9 9 0 0 1 9 18Z\""));
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
    fn only_svgs_drawn_here_are_taken_for_generated() {
        let svg = Screen::new(1, 1).to_svg("t");
        assert!(is_generated(&svg));
        assert!(!is_generated(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">\n<title>by hand</title>\n</svg>\n"
        ));
        let quoting = format!("<svg>\n<title>t</title>\n<g/>\n<g/>\n{GENERATED_MARK}\n</svg>\n");
        assert!(!is_generated(&quoting));
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
            for name in generated_svgs_in(&directory) {
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
            generated_svgs_in(&directory)
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
