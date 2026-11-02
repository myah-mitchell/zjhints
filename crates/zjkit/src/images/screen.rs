//! A grid of terminal cells and the SVG it is written out as.

use super::pen::{Look, Pen, TERMINAL_BACKGROUND};
use crate::ansi::{char_width, AnsiParser, AnsiSegment};
use crate::settings::view::Rgb;
use std::fmt::Write;

/// The width of one terminal cell in pixels.
const CELL_WIDTH: usize = 9;
/// The height of one terminal cell in pixels.
const CELL_HEIGHT: usize = 18;
/// The margin to the left and right of the grid, in pixels.
const MARGIN_X: usize = 14;
/// The margin above and below the grid, in pixels.
const MARGIN_Y: usize = 12;
/// Where the text sits below the top of its cell.
const BASELINE: usize = 13;

/// The Powerline right-pointing arrow. It lives in a Nerd Font's private use
/// area, which a viewer is unlikely to have, so it is drawn as a triangle.
pub const POWERLINE_ARROW: char = '\u{e0b0}';
/// The round caps the pills format theme draws at each end of a block.
pub const PILL_LEFT: char = '\u{e0b6}';
pub const PILL_RIGHT: char = '\u{e0b4}';

/// The mouse pointer, drawn as an arrow with its tip at the top left of its
/// cell. A private use character, so no text the plugin draws is taken for it.
pub const MOUSE_POINTER: char = '\u{f8fe}';

/// Marks the second column of a character two columns wide.
pub(super) const COVERED: char = '\0';

/// One cell of the grid: a character and how it is drawn.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct Cell {
    pub(super) character: char,
    pub(super) look: Look,
}

/// A terminal screen of fixed size, and the SVG it comes out as.
#[derive(Clone)]
pub struct Screen {
    pub(super) rows: Vec<Vec<Cell>>,
}

impl Screen {
    /// A blank screen of `rows` by `columns` cells.
    pub fn new(rows: usize, columns: usize) -> Self {
        let blank = Cell {
            character: ' ',
            look: Pen::default().look(),
        };
        Screen {
            rows: vec![vec![blank; columns]; rows],
        }
    }

    /// How many columns wide the screen is.
    pub fn columns(&self) -> usize {
        self.rows.first().map_or(0, Vec::len)
    }

    /// Put `character` at a spot of the grid, if it fits, and say how many
    /// columns it took.
    pub fn put(&mut self, row: usize, column: usize, character: char, look: Look) -> usize {
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
    pub fn print(&mut self, row: usize, column: usize, ansi: &str) {
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
    pub fn paste(&mut self, row: usize, column: usize, other: &Screen) {
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
    pub fn print_with(
        &mut self,
        row: usize,
        column: usize,
        text: &str,
        pen_of: impl Fn(usize) -> Pen,
    ) {
        let mut column = column;
        for (index, character) in text.chars().enumerate() {
            column += self.put(row, column, character, pen_of(index).look());
        }
    }

    /// The screen as an SVG, titled `title` for the viewers that read one.
    pub fn to_svg(&self, title: &str) -> String {
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
pub(super) fn svg_start(rows: usize, columns: usize, title: &str) -> String {
    let width = columns * CELL_WIDTH + 2 * MARGIN_X;
    let height = rows * CELL_HEIGHT + 2 * MARGIN_Y;
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
    svg
}

/// The text group's opening tag, which sets the font for every row in it.
pub(super) const TEXT_GROUP: &str = concat!(
    "<g font-family=\"ui-monospace, SFMono-Regular, Menlo, Consolas, 'DejaVu Sans Mono', monospace\"",
    " font-size=\"15\" xml:space=\"preserve\">"
);

/// `svg` closed: the text group, the margin's group and the SVG itself.
pub(super) fn svg_end(mut svg: String) -> String {
    let _ = writeln!(svg, "</g>");
    let _ = writeln!(svg, "</g>");
    let _ = writeln!(svg, "</svg>");
    svg
}

/// One rectangle for each stretch of a row with the same background.
pub(super) fn write_backgrounds(svg: &mut String, row: usize, cells: &[Cell]) {
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
/// A shape in place of each Powerline arrow and pill cap of a row, since few
/// fonts a reader has carry those glyphs.
pub(super) fn write_arrows(svg: &mut String, row: usize, cells: &[Cell]) {
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
pub(super) fn write_text(svg: &mut String, row: usize, cells: &[Cell]) {
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

pub(super) fn hex((red, green, blue): Rgb) -> String {
    format!("#{red:02x}{green:02x}{blue:02x}")
}

pub(super) fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::images::pen::{ANSI_COLORS, TERMINAL_FOREGROUND};

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
}
