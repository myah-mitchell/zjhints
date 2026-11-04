//! The colors of the terminal the images are drawn in, and the text
//! attributes an ANSI string sets on the way.

use crate::settings::view::Rgb;
use zellij_tile::prelude::PaletteColor;

/// The terminal's own background, which shows wherever a cell sets none.
/// The terminal is drawn in Catppuccin Mocha, the same palette the images use
/// for the `zellij` theme (see `environment::palette`).
pub const TERMINAL_BACKGROUND: Rgb = (0x1e, 0x1e, 0x2e);
/// The terminal's own text color, used wherever a cell sets none.
pub const TERMINAL_FOREGROUND: Rgb = (0xcd, 0xd6, 0xf4);
/// The terminal's 16 ANSI colors: the normal eight, then the bright eight.
pub const ANSI_COLORS: [Rgb; 16] = [
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
pub fn xterm_color(index: u8) -> Rgb {
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

/// The color of one of Zellij's palette entries.
pub fn rgb_of(color: PaletteColor) -> Rgb {
    match color {
        PaletteColor::Rgb(rgb) => rgb,
        PaletteColor::EightBit(index) => xterm_color(index),
    }
}

/// The text attributes in force at some point of an ANSI string.
#[derive(Clone, Copy, Default, PartialEq)]
pub struct Pen {
    /// `None` is the terminal's own text color.
    pub foreground: Option<Rgb>,
    pub background: Option<Rgb>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub reverse: bool,
}

impl Pen {
    /// A pen that sets only the text color.
    pub fn colored(foreground: Rgb) -> Self {
        Pen {
            foreground: Some(foreground),
            ..Pen::default()
        }
    }

    /// Take in an SGR escape sequence, such as `\x1b[1;38;5;75m`.
    pub fn apply(&mut self, sequence: &str) {
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
    pub fn look(&self) -> Look {
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
pub struct Look {
    pub foreground: Rgb,
    /// `None` leaves the terminal's own background showing.
    pub background: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
}
