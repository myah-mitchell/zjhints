//! Screens shown one after another, written out as one SVG that loops them.

use super::screen::{
    svg_end, svg_start, write_arrows, write_backgrounds, write_text, Cell, Screen, TEXT_GROUP,
};
use std::fmt::Write;

/// One screen of an animation, and how long it stays up.
pub struct Frame {
    screen: Screen,
    milliseconds: u32,
}

impl Frame {
    /// `screen`, shown for `milliseconds`.
    pub fn new(screen: Screen, milliseconds: u32) -> Self {
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
pub struct Animation {
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
    pub fn new(frames: Vec<Frame>, still: usize) -> Self {
        assert!(still < frames.len(), "the still frame is one of the frames");
        assert!(
            frames.iter().all(|frame| frame.milliseconds > 0),
            "every frame shows for some time"
        );
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
        // A blank row draws nothing, so it needs no frames of its own. A
        // space draws something only with a background or a line through it.
        looks.retain(|look| {
            look.cells.iter().any(|cell| {
                cell.character != ' '
                    || cell.look.background.is_some()
                    || cell.look.underline
                    || cell.look.strike
            })
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
    pub fn to_svg(&self, title: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
