//! Images of a plugin's terminal output, drawn by the code that draws it, for
//! the README and the docs.
//!
//! An image is a grid of terminal cells, filled in from the ANSI a plugin
//! prints, or from text placed on it a character at a time, and written out as
//! an SVG. Nothing here needs a terminal or a running Zellij, and the output is
//! the same on every machine: the cell size is fixed, the numbers are
//! integers, and each character is placed at its own column so the font a
//! viewer falls back to cannot shift anything. [`Screen`] is one still image
//! and [`Animation`] a loop of them, written as one SVG that plays with CSS
//! alone.
//!
//! This is the engine only. A plugin keeps its own drawings, the code that
//! builds its screens, in a test module of its own, and hands the finished
//! SVGs to [`write_or_compare_images`], along with the plugin's name and the
//! directory the images live in. The engine assumes neither.
//!
//! [`write_or_compare_images`] compares what a plugin draws with the files in
//! its directory, so a test fails when a change to the plugin leaves an image
//! out of date. With `ZJKIT_WRITE_IMAGES` set it rewrites the files instead,
//! and removes the ones the plugin drew before but no longer does. It knows a
//! plugin's own files by [`mark_of`], a comment on the second line of each SVG
//! that names the plugin. An SVG with no mark, or with another plugin's, is
//! never touched, so plugins can share a directory and an image made by hand
//! can live next to generated ones.
//!
//! The engine is behind the `images` feature, which only tests enable: it
//! pulls in Zellij's themes and default config to fix the look of the drawing,
//! none of which a plugin's wasm build needs.

mod animation;
mod environment;
mod pen;
mod screen;

pub use animation::{Animation, Frame};
pub use environment::{default_keybinds, mode_info, palette};
pub use pen::{
    rgb_of, xterm_color, Look, Pen, ANSI_COLORS, TERMINAL_BACKGROUND, TERMINAL_FOREGROUND,
};
pub use screen::{Screen, MOUSE_POINTER, PILL_LEFT, PILL_RIGHT, POWERLINE_ARROW};

use std::fs;
use std::path::Path;

/// Set to rewrite the images instead of comparing against them.
pub const WRITE_ENV: &str = "ZJKIT_WRITE_IMAGES";

/// The comment that marks an SVG as drawn for `plugin`, which is how a plugin
/// tells its own files from other plugins' and from ones made by hand.
///
/// # Panics
///
/// When `plugin` is empty or has a character other than a letter, a digit, `-`
/// or `_`: a name that could end the comment early or be mistaken for another
/// plugin's.
pub fn mark_of(plugin: &str) -> String {
    assert!(
        !plugin.is_empty()
            && plugin
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character)),
        "a plugin name is letters, digits, hyphens and underscores: {plugin:?}"
    );
    format!(
        "<!-- Drawn for {plugin} by zjkit's image engine: edit the code and run `make images`. -->"
    )
}

/// `svg` with the mark for `plugin` as its second line, under the opening
/// `<svg>` tag that `Screen::to_svg` and `Animation::to_svg` start with.
fn marked(plugin: &str, svg: &str) -> String {
    let (opening, rest) = svg
        .split_once('\n')
        .expect("an SVG drawn by the engine has more than one line");
    format!("{opening}\n{}\n{rest}", mark_of(plugin))
}

/// Whether `svg` carries the mark for `plugin`. The mark sits in the first
/// lines, so a hand-made SVG that quotes it further down is not mistaken for a
/// generated one.
fn is_drawn_for(plugin: &str, svg: &str) -> bool {
    let mark = mark_of(plugin);
    svg.lines().take(3).any(|line| line == mark)
}

/// The SVGs in `directory` drawn for `plugin`, by file name.
fn svgs_drawn_for(plugin: &str, directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".svg"))
                .filter(|name| {
                    fs::read_to_string(directory.join(name))
                        .is_ok_and(|svg| is_drawn_for(plugin, &svg))
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Check `images`, each a file name in `directory` and the SVG that
/// `Screen::to_svg` or `Animation::to_svg` drew for it, against the files
/// there, or rewrite the files when `ZJKIT_WRITE_IMAGES` is set. The mark for
/// `plugin` is added to each SVG here, so a drawing does not carry it.
///
/// Rewriting also removes the SVGs in `directory` that carry the mark for
/// `plugin` but are no longer drawn. Every other file is left alone.
///
/// # Panics
///
/// When a file is stale, missing, or no longer drawn, with a message that
/// names them and says to regenerate; and on a file system error.
pub fn write_or_compare_images(plugin: &str, directory: &Path, images: &[(&str, String)]) {
    let write = std::env::var_os(WRITE_ENV).is_some();
    if let Err(stale) = synchronize(plugin, directory, images, write) {
        panic!(
            "{} is out of date: {stale}. Run `make images` and commit the result.",
            directory.display()
        );
    }
}

/// The work of [`write_or_compare_images`], with the choice of writing passed
/// in. `Err` lists what differs, and only when comparing.
fn synchronize(
    plugin: &str,
    directory: &Path,
    images: &[(&str, String)],
    write: bool,
) -> Result<(), String> {
    let images: Vec<(&str, String)> = images
        .iter()
        .map(|(name, svg)| (*name, marked(plugin, svg)))
        .collect();
    let orphans: Vec<String> = svgs_drawn_for(plugin, directory)
        .into_iter()
        .filter(|name| !images.iter().any(|(image, _)| image == name))
        .collect();
    if write {
        fs::create_dir_all(directory).expect("create the image directory");
        for (name, svg) in &images {
            fs::write(directory.join(name), svg).expect("write an image");
        }
        for name in orphans {
            fs::remove_file(directory.join(name)).expect("remove an orphaned image");
        }
        return Ok(());
    }
    let mut stale: Vec<String> = images
        .iter()
        .filter(|(name, svg)| fs::read_to_string(directory.join(name)).ok().as_ref() != Some(svg))
        .map(|(name, _)| name.to_string())
        .collect();
    stale.extend(
        orphans
            .into_iter()
            .map(|name| format!("{name} (no longer generated)")),
    );
    if stale.is_empty() {
        Ok(())
    } else {
        Err(stale.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// An empty directory of this test's own, removed when it goes out of
    /// scope, so tests running side by side do not share files.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("zjkit-images-{}-{test}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create a scratch directory");
            Scratch(path)
        }

        fn names(&self) -> Vec<String> {
            let mut names: Vec<String> = fs::read_dir(&self.0)
                .expect("read the scratch directory")
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// An SVG as `Screen::to_svg` starts one, drawing `text`.
    fn drawing(text: &str) -> String {
        let mut screen = Screen::new(1, 4);
        screen.print(0, 0, text);
        screen.to_svg("t")
    }

    #[test]
    fn the_mark_names_the_plugin() {
        assert!(mark_of("zjtabs").contains("Drawn for zjtabs "));
        assert_ne!(mark_of("zjtabs"), mark_of("zjhints"));
    }

    #[test]
    #[should_panic(expected = "a plugin name is letters")]
    fn a_plugin_name_that_could_end_the_comment_is_refused() {
        mark_of("zj--> <script>");
    }

    #[test]
    fn an_svg_is_marked_on_its_second_line() {
        let svg = marked("zjtabs", &drawing("a"));
        let lines: Vec<&str> = svg.lines().collect();
        assert!(lines[0].starts_with("<svg "));
        assert_eq!(lines[1], mark_of("zjtabs"));
        assert!(lines[2].starts_with("<title>"));
    }

    #[test]
    fn only_svgs_drawn_for_the_plugin_are_taken_for_its_own() {
        let svg = marked("zjtabs", &drawing("a"));
        assert!(is_drawn_for("zjtabs", &svg));
        assert!(!is_drawn_for("zjhints", &svg));
        // A name that starts with another plugin's name is not that plugin.
        assert!(!is_drawn_for("zjtab", &svg));
        assert!(!is_drawn_for(
            "zjtabs",
            "<svg xmlns=\"http://www.w3.org/2000/svg\">\n<title>by hand</title>\n</svg>\n"
        ));
        let quoting = format!(
            "<svg>\n<title>t</title>\n<g/>\n<g/>\n{}\n</svg>\n",
            mark_of("zjtabs")
        );
        assert!(!is_drawn_for("zjtabs", &quoting));
    }

    #[test]
    fn rewriting_removes_only_this_plugins_orphans() {
        let scratch = Scratch::new("rewriting");
        let directory = scratch.0.as_path();
        // Both plugins draw into one directory, as they might in a shared
        // docs/img, and one image of each is no longer drawn.
        synchronize(
            "zjhints",
            directory,
            &[("kept.svg", drawing("a")), ("old-hints.svg", drawing("b"))],
            true,
        )
        .unwrap();
        synchronize("zjtabs", directory, &[("old-tabs.svg", drawing("c"))], true).unwrap();
        let by_hand = "<svg>\n<title>by hand</title>\n</svg>\n";
        fs::write(directory.join("by-hand.svg"), by_hand).unwrap();
        fs::write(directory.join("notes.txt"), mark_of("zjhints")).unwrap();

        synchronize("zjhints", directory, &[("kept.svg", drawing("a"))], true).unwrap();
        assert_eq!(
            scratch.names(),
            ["by-hand.svg", "kept.svg", "notes.txt", "old-tabs.svg"]
        );
        assert_eq!(
            fs::read_to_string(directory.join("by-hand.svg")).unwrap(),
            by_hand
        );

        synchronize("zjtabs", directory, &[("new-tabs.svg", drawing("d"))], true).unwrap();
        assert_eq!(
            scratch.names(),
            ["by-hand.svg", "kept.svg", "new-tabs.svg", "notes.txt"]
        );
    }

    #[test]
    fn comparing_names_what_is_stale_missing_or_orphaned() {
        let scratch = Scratch::new("comparing");
        let directory = scratch.0.as_path();
        synchronize(
            "zjhints",
            directory,
            &[
                ("same.svg", drawing("a")),
                ("changed.svg", drawing("b")),
                ("old.svg", drawing("c")),
            ],
            true,
        )
        .unwrap();
        synchronize("zjtabs", directory, &[("theirs.svg", drawing("d"))], true).unwrap();

        let drawn = [
            ("same.svg", drawing("a")),
            ("changed.svg", drawing("x")),
            ("new.svg", drawing("e")),
        ];
        let stale = synchronize("zjhints", directory, &drawn, false).unwrap_err();
        assert_eq!(stale, "changed.svg, new.svg, old.svg (no longer generated)");
        // Comparing writes and removes nothing.
        assert_eq!(
            scratch.names(),
            ["changed.svg", "old.svg", "same.svg", "theirs.svg"]
        );

        synchronize("zjhints", directory, &drawn, true).unwrap();
        assert_eq!(synchronize("zjhints", directory, &drawn, false), Ok(()));
    }
}
