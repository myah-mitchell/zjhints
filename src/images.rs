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

mod session;

use crate::mouse;
use crate::popup::{self, PreviewPopup};
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

/// The Powerline right-pointing arrow. It lives in a Nerd Font's private use
/// area, which a viewer is unlikely to have, so it is drawn as a triangle.
const POWERLINE_ARROW: char = '\u{e0b0}';
/// The round caps the pills format theme draws at each end of a block.
const PILL_LEFT: char = '\u{e0b6}';
const PILL_RIGHT: char = '\u{e0b4}';

/// The mouse pointer, drawn as an arrow with its tip at the top left of its
/// cell. A private use character, so no text the plugin draws is taken for it.
const MOUSE_POINTER: char = '\u{f8fe}';

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

#[derive(Clone, Copy, PartialEq)]
struct Cell {
    character: char,
    look: Look,
}

/// A terminal screen of fixed size, and the SVG it comes out as.
#[derive(Clone)]
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

    /// Lay `other` over this screen with its top left corner at a spot of the
    /// grid, clipped to this screen's edges.
    fn paste(&mut self, row: usize, column: usize, other: &Screen) {
        for (offset, cells) in other.rows.iter().enumerate() {
            let Some(target) = self.rows.get_mut(row + offset) else {
                break;
            };
            for (index, cell) in cells.iter().enumerate() {
                if let Some(spot) = target.get_mut(column + index) {
                    *spot = *cell;
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
        let mut svg = svg_start(self.rows.len(), self.columns(), title);
        let _ = writeln!(svg, "<g shape-rendering=\"crispEdges\">");
        for (row, cells) in self.rows.iter().enumerate() {
            write_backgrounds(&mut svg, row, cells);
        }
        let _ = writeln!(svg, "</g>");
        let _ = writeln!(svg, "{TEXT_GROUP}");
        for (row, cells) in self.rows.iter().enumerate() {
            write_arrows(&mut svg, row, cells);
            write_text(&mut svg, row, cells);
        }
        svg_end(svg)
    }
}

/// The opening of an SVG for a grid of `rows` by `columns`: the terminal's
/// background, and the margin the grid is drawn inside.
fn svg_start(rows: usize, columns: usize, title: &str) -> String {
    let width = columns * CELL_WIDTH + 2 * MARGIN_X;
    let height = rows * CELL_HEIGHT + 2 * MARGIN_Y;
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
    svg
}

/// The text group's opening tag, which sets the font for every row in it.
const TEXT_GROUP: &str = concat!(
    "<g font-family=\"ui-monospace, SFMono-Regular, Menlo, Consolas, 'DejaVu Sans Mono', monospace\"",
    " font-size=\"15\" xml:space=\"preserve\">"
);

/// `svg` closed: the text group, the margin's group and the SVG itself.
fn svg_end(mut svg: String) -> String {
    let _ = writeln!(svg, "</g>");
    let _ = writeln!(svg, "</g>");
    let _ = writeln!(svg, "</svg>");
    svg
}

/// One rectangle for each stretch of a row with the same background.
fn write_backgrounds(svg: &mut String, row: usize, cells: &[Cell]) {
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

/// One screen of an animation, and how long it stays up.
struct Frame {
    screen: Screen,
    milliseconds: u32,
}

impl Frame {
    fn new(screen: Screen, milliseconds: u32) -> Self {
        Frame {
            screen,
            milliseconds,
        }
    }
}

/// Screens shown one after another on a loop, and the SVG they come out as.
///
/// The SVG plays with CSS alone, which a browser runs even in an `<img>`,
/// where scripts do not. Each row is written once for every different way it
/// looks across the frames, and shown only during the frames that look that
/// way, so the rows a scene leaves alone cost nothing per frame. A reader who
/// has asked for less motion sees the frame `still` and nothing moves.
struct Animation {
    frames: Vec<Frame>,
    still: usize,
}

/// One way a row looks, and the frames it looks that way in.
struct RowLook<'a> {
    row: usize,
    cells: &'a [Cell],
    frames: Vec<usize>,
}

impl Animation {
    /// `frames` on a loop, standing still on the frame `still` for readers
    /// who prefer less motion. Every frame is the same size.
    fn new(frames: Vec<Frame>, still: usize) -> Self {
        assert!(still < frames.len(), "the still frame is one of the frames");
        let size = |frame: &Frame| (frame.screen.rows.len(), frame.screen.columns());
        assert!(
            frames.iter().all(|frame| size(frame) == size(&frames[0])),
            "every frame is the same size"
        );
        Animation { frames, still }
    }

    fn milliseconds(&self) -> u32 {
        self.frames.iter().map(|frame| frame.milliseconds).sum()
    }

    /// Every way each row looks, row by row, in the order they first appear.
    fn row_looks(&self) -> Vec<RowLook<'_>> {
        let mut looks: Vec<RowLook> = Vec::new();
        let rows = self.frames[0].screen.rows.len();
        for row in 0..rows {
            let first = looks.len();
            for (index, frame) in self.frames.iter().enumerate() {
                let cells = frame.screen.rows[row].as_slice();
                match looks[first..].iter_mut().find(|look| look.cells == cells) {
                    Some(look) => look.frames.push(index),
                    None => looks.push(RowLook {
                        row,
                        cells,
                        frames: vec![index],
                    }),
                }
            }
        }
        // A blank row draws nothing, so it needs no frames of its own.
        looks.retain(|look| {
            look.cells
                .iter()
                .any(|cell| cell.character != ' ' || cell.look.background.is_some())
        });
        looks
    }

    /// The CSS keyframes that show something only during `frames`: visible
    /// from the start of each run of them to the start of the next frame.
    fn keyframes(&self, name: &str, frames: &[usize]) -> String {
        let total = u64::from(self.milliseconds());
        let mut css = format!("@keyframes {name}{{");
        let mut start = 0u64;
        let mut was_visible = None;
        for (index, frame) in self.frames.iter().enumerate() {
            let visible = frames.contains(&index);
            if was_visible != Some(visible) {
                // Thousandths of a percent, worked out in integers so the
                // output is the same everywhere.
                let at = start * 100_000 / total;
                let state = if visible { "visible" } else { "hidden" };
                let _ = write!(css, "{}.{:03}%{{visibility:{state}}}", at / 1000, at % 1000);
                was_visible = Some(visible);
            }
            start += u64::from(frame.milliseconds);
        }
        css.push('}');
        css
    }

    /// The animation as an SVG, titled `title` for the viewers that read one.
    fn to_svg(&self, title: &str) -> String {
        let first = &self.frames[0].screen;
        let mut svg = svg_start(first.rows.len(), first.columns(), title);
        let looks = self.row_looks();

        // One class per set of frames, shared by every row shown in them.
        let mut classes: Vec<&[usize]> = Vec::new();
        let mut indices = Vec::new();
        for look in &looks {
            let frames = look.frames.as_slice();
            let index = match classes.iter().position(|known| *known == frames) {
                Some(index) => index,
                None => {
                    classes.push(frames);
                    classes.len() - 1
                }
            };
            indices.push(index);
        }

        let total = self.milliseconds();
        let _ = writeln!(svg, "<style>");
        for (class, frames) in classes.iter().enumerate() {
            let _ = writeln!(svg, "{}", self.keyframes(&format!("f{class}"), frames));
            let hidden = if frames.contains(&self.still) {
                ""
            } else {
                "visibility:hidden;"
            };
            let _ = writeln!(
                svg,
                ".f{class}{{{hidden}animation:f{class} {total}ms step-end infinite}}"
            );
        }
        let _ = writeln!(
            svg,
            "@media (prefers-reduced-motion:reduce){{g[class]{{animation:none}}}}"
        );
        let _ = writeln!(svg, "</style>");

        let _ = writeln!(svg, "<g shape-rendering=\"crispEdges\">");
        for (look, class) in looks.iter().zip(&indices) {
            let mut rects = String::new();
            write_backgrounds(&mut rects, look.row, look.cells);
            if !rects.is_empty() {
                let _ = write!(svg, "<g class=\"f{class}\">\n{rects}</g>\n");
            }
        }
        let _ = writeln!(svg, "</g>");
        let _ = writeln!(svg, "{TEXT_GROUP}");
        for (look, class) in looks.iter().zip(&indices) {
            let mut text = String::new();
            write_arrows(&mut text, look.row, look.cells);
            write_text(&mut text, look.row, look.cells);
            if !text.is_empty() {
                let _ = write!(svg, "<g class=\"f{class}\">\n{text}</g>\n");
            }
        }
        svg_end(svg)
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
            MOUSE_POINTER => {
                let point = |x: usize, y: usize| format!("{} {}", left + x, top + y);
                let _ = writeln!(
                    svg,
                    "<path d=\"M{}L{}L{}L{}L{}L{}L{}Z\" fill=\"{}\" stroke=\"{}\"/>",
                    point(0, 0),
                    point(0, 15),
                    point(4, 11),
                    point(7, 17),
                    point(9, 16),
                    point(6, 10),
                    point(11, 10),
                    hex(cell.look.foreground),
                    hex(TERMINAL_BACKGROUND)
                );
                continue;
            }
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
            || matches!(
                cell.character,
                POWERLINE_ARROW | PILL_LEFT | PILL_RIGHT | MOUSE_POINTER
            )
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
    bar_in_session(options, mode, columns, false)
}

/// `bar`, in a session that is `ascended` out of, so not receiving input.
fn bar_in_session(
    options: &[(&str, &str)],
    mode: InputMode,
    columns: usize,
    ascended: bool,
) -> (String, ClickMap) {
    bar_state(options, mode, columns, ascended).draw_bar()
}

/// The plugin's state for `bar_in_session`, before it draws.
fn bar_state(options: &[(&str, &str)], mode: InputMode, columns: usize, ascended: bool) -> State {
    let mut state = State::default();
    state.zellij_palette = Some(palette());
    state.apply_configuration(configuration_of(options));
    state.pane_width = Some(columns);
    state.mode_info = ModeInfo {
        session_ascended: ascended.then_some(true),
        ..mode_info(mode)
    };
    state
}

/// What Zellij tells a plugin about `mode`: the default keybindings, in the
/// Zellij theme the images are drawn in.
fn mode_info(mode: InputMode) -> ModeInfo {
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

/// The themes the topic images are drawn in, unless a shot overrides them.
const BASE_THEME: [(&str, &str); 3] = [
    ("color_theme", "catppuccin-mocha"),
    ("format_theme", "pills"),
    ("alias_theme", "unicode"),
];

/// The themes that leave the bar looking like Zellij's own.
const ZELLIJ_THEME: [(&str, &str); 3] = [
    ("color_theme", "zellij"),
    ("format_theme", "zellij"),
    ("alias_theme", "zellij"),
];

/// One bar of an image: what its caption says, and how it is drawn.
struct Shot<'a> {
    caption: String,
    options: Vec<(&'a str, &'a str)>,
    mode: InputMode,
    columns: usize,
    ascended: bool,
}

impl<'a> Shot<'a> {
    /// The bar in `mode` at full width, with `options` over `BASE_THEME`.
    fn new(caption: impl Into<String>, options: &[(&'a str, &'a str)], mode: InputMode) -> Self {
        let mut all = BASE_THEME.to_vec();
        all.extend_from_slice(options);
        Shot {
            caption: caption.into(),
            options: all,
            mode,
            columns: BAR_COLUMNS,
            ascended: false,
        }
    }

    /// Each value of `option` in turn, captioned with it as a config line.
    fn each(
        option: &'a str,
        values: &[&'a str],
        rest: &[(&'a str, &'a str)],
        mode: InputMode,
    ) -> Vec<Self> {
        values
            .iter()
            .map(|value| {
                let mut options = rest.to_vec();
                options.push((option, value));
                Shot::new(format!("{option} {}", kdl_value(value)), &options, mode)
            })
            .collect()
    }

    /// `options` added to the configuration, leaving the caption as it is.
    fn also(mut self, options: &[(&'a str, &'a str)]) -> Self {
        self.options.extend_from_slice(options);
        self
    }

    fn columns(mut self, columns: usize) -> Self {
        self.columns = columns;
        self
    }

    fn ascended(mut self) -> Self {
        self.ascended = true;
        self
    }
}

/// `value` as a KDL config line writes it: booleans bare, the rest quoted.
fn kdl_value(value: &str) -> String {
    match value {
        "true" | "false" => value.to_string(),
        _ => format!("{value:?}"),
    }
}

/// A screen holding each shot under its caption.
fn shots(shots: &[Shot]) -> Screen {
    let mut screen = Screen::new(shots.len() * 3 - 1, BAR_COLUMNS);
    for (index, shot) in shots.iter().enumerate() {
        let row = index * 3;
        screen.print_with(row, 0, &shot.caption, |_| Pen {
            dim: true,
            ..Pen::default()
        });
        let line = bar_in_session(&shot.options, shot.mode, shot.columns, shot.ascended).0;
        screen.print(row + 1, 0, &line);
    }
    screen
}

/// The bar of `shot` narrowing from its width to `narrowest` columns and
/// widening back, `step` columns a frame, under a caption giving the width
/// and with the edge of its pane marked. It stands still at `still` columns.
fn narrowing(shot: &Shot, narrowest: usize, step: usize, still: usize) -> Animation {
    let widest = shot.columns;
    let mut widths: Vec<usize> = (narrowest..=widest).rev().step_by(step).collect();
    let mut back: Vec<usize> = widths.iter().rev().skip(1).copied().collect();
    back.pop();
    widths.extend(back);
    let edge = Pen {
        dim: true,
        ..Pen::default()
    };
    let frames = widths
        .iter()
        .map(|&columns| {
            let mut screen = Screen::new(2, widest + 1);
            screen.print_with(0, 0, &format!("{columns} columns"), |_| edge);
            let line = bar_in_session(&shot.options, shot.mode, columns, shot.ascended).0;
            screen.print(1, 0, &line);
            screen.print_with(1, columns, "│", |_| edge);
            // The ends are held, so each width can be read before it moves.
            let hold = columns == widest || columns == narrowest;
            Frame::new(screen, if hold { 2000 } else { 400 })
        })
        .collect();
    let still = widths
        .iter()
        .position(|&columns| columns == still)
        .expect("the still width is one of the steps");
    Animation::new(frames, still)
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
/// lays it out, with `edits` made and left unsaved and `select` selected.
fn settings_screen(
    options: &[(&str, &str)],
    select: &str,
    edits: &[(&str, &str)],
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
    for (key, value) in edits {
        editor.submit(
            &Field::Value {
                key: key.to_string(),
            },
            value,
        );
    }
    let (section, row) = editor.locate(select).expect("a key the screen lists");
    let index = editor
        .sections()
        .iter()
        .position(|candidate| *candidate == section)
        .expect("a section of the menu");
    editor.select_section(index);
    editor.select(row);

    let items = view::menu_items(&editor);
    let preview_rows = view::is_previewed(editor.section()).then_some(editor.rows().len());
    let layout = Layout::new(rows, columns, &items, preview_rows).expect("room for the screen");
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
    let zellij_palette = palette();
    let rows_view = view::Rows::new(&editor, FormatView::Rendered, Some(&zellij_palette));
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

    // The preview, under a Theme list: a dim caption, then the bar.
    if let Some(y) = layout.preview_y {
        let mode_info = mode_info(InputMode::Normal);
        let overrides = view::theme_overrides(editor.working(), Some(&mode_info.style.colors));
        let caption = view::truncate(&view::preview_caption(overrides.len(), true), width);
        let dim = Pen {
            dim: true,
            ..pens.plain
        };
        screen.print_with(y, layout.content_x, &caption, |_| dim);
        if !overrides.is_empty() {
            let x = layout.content_x + view::columns(&caption) + 2;
            let button = view::overrides_button(true);
            print_buttons(&mut screen, y, x, layout.content_x + width, &[button], &pens);
        }
        let bar = view::theme_preview(editor.working(), &mode_info, true, width);
        screen.print(y + 1, layout.content_x, &bar);
    }

    // The footer.
    if let Some(description) = view::describe(&editor) {
        let description = view::truncate(&description, layout.width);
        screen.print_with(layout.footer_y + 1, layout.x, &description, |_| {
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
        layout.footer_y + 3,
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
    let configuration = popup::configuration("1-1", &preview.entries, false);
    let popup = PreviewPopup::from_configuration(&configuration).expect("a preview popup");
    let (width, height) = popup::size(&preview.entries, false);

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
            "session-demo.svg",
            session::demo().to_svg("A Zellij session with the zjhints bar at the bottom, played through its features: the bar following each mode, dropping hints as the terminal narrows, previewing the hints that did not fit, the settings screen restyling it live, and the hints of a nested session"),
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
                &[("desc_format", "#[fg=$fg,bg=$surface,italic] {desc} ")],
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
                &[("color_accent", "#f5c2e7")],
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
    .into_iter()
    .chain(documentation_images())
    .collect()
}

/// The colors the format examples in the docs name, as a user would set them.
const EXAMPLE_COLORS: [(&str, &str); 6] = [
    ("color_bg", "#181825"),
    ("color_fg", "#cdd6f4"),
    ("color_surface", "#313244"),
    ("color_gray", "#6c7086"),
    ("color_red", "#f38ba8"),
    ("color_accent", "#cba6f7"),
];

/// The images of the topic pages, each showing the config examples beside
/// it as the bar draws them.
fn documentation_images() -> Vec<(&'static str, String)> {
    use InputMode::{Locked, Normal, Pane, Tab};
    let with_colors = |options: &[(&'static str, &'static str)]| {
        let mut all = EXAMPLE_COLORS.to_vec();
        all.extend_from_slice(options);
        all
    };
    // Each image names in its doc whichever of these shared settings it uses
    // to bring the hints it is about into view.
    let fitted = [("hint_order", "*, quit")];
    let precedence = [("hint_order", "pane, *, quit")];
    let formats = with_colors(&[
        ("key_format", "#[fg=$bg,bg=$accent,bold] {key} "),
        ("desc_format", "#[fg=$fg,bg=$surface] {desc} "),
    ]);
    let mut quit = formats.clone();
    quit.extend([
        ("key_format_quit", "#[fg=$bg,bg=$red,bold] {key} "),
        ("desc_format_quit", "#[fg=$red,bg=$surface,bold] {desc} "),
    ]);
    let aliases = [
        ("key_alias_enter", "⏎"),
        ("key_alias_esc", "⎋"),
        ("key_alias_left", "←"),
        ("key_alias_down", "↓"),
        ("key_alias_up", "↑"),
        ("key_alias_right", "→"),
        ("mod_alias_ctrl", "^"),
        ("mod_alias_alt", "⌥"),
    ];
    let focus_first = [("hint_order", "focus, mode_normal, *")];
    let zellij_keys = [("alias_theme", "zellij")];
    let labels = [
        ("label_split_right", "split →"),
        ("label_split_down", "split ↓"),
        ("label_focus", "◆"),
        ("label_mode_normal", "exit"),
    ];
    // Named by label too, since a relabeled hint is matched by its new label.
    let splits_first = [(
        "hint_order",
        "split_right, split →, split_down, split ↓, focus, ◆, mode_normal, exit, *",
    )];
    let layouts_first = [("hint_order", "prev_layout, next_layout, swap layout, *")];
    let swap = [
        ("label_next_layout", "swap layout"),
        ("label_prev_layout", "swap layout"),
    ];
    let tabs_first = [("hint_order", "go_to_tab, focus, *")];
    let curated = [("discover_hints", "false")];
    let modes_first = [(
        "hint_order",
        "mode_tab, mode_resize, mode_move, mode_scroll, *",
    )];
    let one_format = [
        ("show_mode", "true"),
        ("mode_format", "#[fg=#181825,bg=#cba6f7,bold] {desc} "),
        ("mode_label_normal", "NORM"),
    ];
    let show_mode = [("show_mode", "true")];
    vec![
        (
            "fitting-narrowing.svg",
            narrowing(&Shot::new("", &fitted, Normal), 36, 6, 60)
                .to_svg("The normal mode bar narrowing from 120 to 36 columns and widening back, losing hints from the middle while quit stays last"),
        ),
        (
            "fitting-precedence.svg",
            shots(
                &Shot::each("hint_precedence", &["tl", "lt"], &precedence, Normal)
                    .into_iter()
                    .map(|shot| shot.columns(14))
                    .collect::<Vec<_>>(),
            )
            .to_svg("The normal mode bar at 14 columns, keeping quit with hint_precedence tl and pane with lt"),
        ),
        (
            "styling-formats.svg",
            shots(&[
                Shot::new("the Zellij theme's colors", &ZELLIJ_THEME, Normal),
                Shot::new("key_format and desc_format", &formats, Normal),
            ])
            .to_svg("The normal mode bar in the Zellij theme's colors, then with key_format and desc_format set"),
        ),
        (
            "styling-one-hint.svg",
            shots(&[
                Shot::new("key_format and desc_format", &formats, Normal),
                Shot::new("plus key_format_quit and desc_format_quit", &quit, Normal),
            ])
            .to_svg("The normal mode bar with one format for every hint, then with quit drawn in red"),
        ),
        (
            "styling-spacer.svg",
            shots(&Shot::each(
                "hint_spacer",
                &["", "  ", "#[fg=$gray] │ "],
                &formats,
                Normal,
            ))
            .to_svg("The normal mode bar with no hint spacer, a wider gap, and a gray divider between hints"),
        ),
        (
            "styling-aliases.svg",
            shots(&[
                Shot::new("Zellij's key names", &focus_first, Pane).also(&zellij_keys),
                Shot::new("key_alias_<name> and mod_alias_<name>", &focus_first, Pane)
                    .also(&zellij_keys)
                    .also(&aliases),
            ])
            .to_svg("The pane mode bar with Zellij's key names, then with Alt, Ctrl, Esc, Enter and the arrows replaced by symbols"),
        ),
        (
            "labels-changing.svg",
            shots(&[
                Shot::new("the default labels", &splits_first, Pane),
                Shot::new("label_<id>", &splits_first, Pane).also(&labels),
            ])
            .to_svg("The pane mode bar with its default labels, then with split right, split down, focus and normal relabeled"),
        ),
        (
            "labels-combining.svg",
            shots(&[
                Shot::new("the default labels", &layouts_first, Normal),
                Shot::new("label_next_layout and label_prev_layout \"swap layout\"", &layouts_first, Normal).also(&swap),
            ])
            .to_svg("The normal mode bar with prev layout and next layout as two hints, then merged into one swap layout hint"),
        ),
        (
            "ordering-direction-keys.svg",
            shots(&Shot::each(
                "direction_keys",
                &["both", "arrows", "letters"],
                &focus_first,
                Pane,
            ))
            .to_svg("The pane mode bar with direction_keys both, arrows and letters, changing the keys of focus"),
        ),
        (
            "ordering-key-order.svg",
            shots(&Shot::each(
                "key_order",
                &["qwerty", "none"],
                &tabs_first,
                Tab,
            ))
            .to_svg("The tab mode bar with key_order qwerty and none, changing the order of the tab and focus keys"),
        ),
        (
            "ordering-hint-order.svg",
            shots(&Shot::each(
                "hint_order",
                &["*", "quit, *", "*, pane"],
                &curated,
                Normal,
            ))
            .to_svg("The normal mode bar in its own order, with quit moved first, and with pane moved last"),
        ),
        (
            "hints-discovery.svg",
            shots(&Shot::each(
                "discover_hints",
                &["false", "true"],
                &[],
                InputMode::Tmux,
            ))
            .to_svg("The tmux mode bar with only its way out, then with every discovered binding after it"),
        ),
        (
            "hints-shared.svg",
            shots(&Shot::each(
                "hide_shared_hints",
                &["true", "false"],
                &modes_first,
                Pane,
            ))
            .to_svg("The pane mode bar without the mode switches it shares with normal mode, then listing them"),
        ),
        (
            "nested-dimming.svg",
            shots(&[
                Shot::new("receiving input", &formats, Normal),
                Shot::new("not receiving input, dim_strength \"0.5\"", &formats, Normal).ascended(),
            ])
            .to_svg("The bar as drawn while the session receives input, then dimmed while it does not"),
        ),
        (
            "nested-show-mode.svg",
            shots(&[
                Shot::new("show_mode true", &show_mode, Normal),
                Shot::new("show_mode true, in locked mode", &show_mode, Locked),
                Shot::new("one mode_format, mode_label_normal \"NORM\"", &one_format, Normal),
            ])
            .to_svg("The bar with the mode shown before the hints, in normal and locked mode, then drawn from one mode_format with a shorter label"),
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

    /// A one-row screen showing `text`.
    fn showing(text: &str) -> Screen {
        let mut screen = Screen::new(2, 4);
        screen.print(0, 0, text);
        screen
    }

    #[test]
    fn a_row_that_stays_the_same_is_written_once() {
        let mut first = showing("ab");
        let mut second = showing("cd");
        first.print(1, 0, "zz");
        second.print(1, 0, "zz");
        let animation = Animation::new(vec![Frame::new(first, 100), Frame::new(second, 300)], 0);
        let svg = animation.to_svg("t");
        assert_eq!(svg.matches(">zz</tspan>").count(), 1, "{svg}");
        assert_eq!(svg.matches(">ab</tspan>").count(), 1);
        assert_eq!(svg.matches(">cd</tspan>").count(), 1);
    }

    #[test]
    fn a_row_shows_only_during_its_frames() {
        let frames = vec![
            Frame::new(showing("a"), 1000),
            Frame::new(showing("b"), 2000),
            Frame::new(showing("a"), 1000),
        ];
        let svg = Animation::new(frames, 1).to_svg("t");
        // "a" is shown in the first and last frames, as one class.
        assert!(
            svg.contains("@keyframes f0{0.000%{visibility:visible}25.000%{visibility:hidden}75.000%{visibility:visible}}"),
            "{svg}"
        );
        assert!(svg.contains("@keyframes f1{0.000%{visibility:hidden}25.000%{visibility:visible}75.000%{visibility:hidden}}"));
        assert!(svg.contains("animation:f0 4000ms step-end infinite"));
        assert_eq!(svg.matches(">a</tspan>").count(), 1);
    }

    #[test]
    fn without_motion_only_the_still_frame_shows() {
        let frames = vec![Frame::new(showing("a"), 100), Frame::new(showing("b"), 100)];
        let svg = Animation::new(frames, 1).to_svg("t");
        assert!(svg.contains(".f0{visibility:hidden;animation:"), "{svg}");
        assert!(svg.contains(".f1{animation:"));
        assert!(svg.contains("@media (prefers-reduced-motion:reduce){g[class]{animation:none}}"));
    }

    #[test]
    fn a_blank_row_draws_nothing() {
        let frames = vec![Frame::new(showing("a"), 100), Frame::new(showing("b"), 100)];
        let svg = Animation::new(frames, 0).to_svg("t");
        assert_eq!(svg.matches("<g class=").count(), 2, "{svg}");
        assert!(is_generated(&svg));
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
