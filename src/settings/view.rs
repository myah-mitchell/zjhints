//! What the settings screen shows and where: the layout for a pane size, and
//! each line as text with the parts to draw differently. Pure functions, so
//! the screen module only has to draw what these return.

use super::editor::{Editor, Focus, Row, Section, OTHER_HEADING};
use super::options::{self, Kind, FAMILIES};
use crate::format;
use std::collections::BTreeMap;
use std::ops::Range;
use zellij_tile::prelude::{BareKey, KeyWithModifier};

/// Lines above the body: the title and a blank line.
pub const HEADER_LINES: usize = 2;
/// Lines below the body: the selected row's description, a status line and
/// the key help.
pub const FOOTER_LINES: usize = 3;
/// Columns a row's name is padded to, wide enough for the longest option
/// name. A longer key, such as a label's, pushes its value along.
pub const NAME_COLUMNS: usize = 21;
/// The fewest columns the list keeps beside the sidebar. A narrower pane
/// folds the sidebar into a line over the list.
const MIN_LIST_COLUMNS: usize = 40;

/// How format-string values are shown in the list.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum FormatView {
    /// As the bar draws them, with sample keys, descriptions and modes.
    #[default]
    Rendered,
    /// As written in the config.
    Raw,
}

impl FormatView {
    pub fn toggled(self) -> Self {
        match self {
            FormatView::Rendered => FormatView::Raw,
            FormatView::Raw => FormatView::Rendered,
        }
    }
}

/// An RGB color, for a swatch.
pub type Rgb = (u8, u8, u8);

/// Where everything goes in a pane of a given size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// Columns of the sidebar, its border excluded, or `None` when the pane
    /// is too narrow for one and a header line over the list stands in.
    pub sidebar: Option<usize>,
    /// The line the sidebar, or the header line, starts on.
    pub body_y: usize,
    /// Lines the sidebar has.
    pub body_height: usize,
    pub list_x: usize,
    pub list_y: usize,
    pub list_width: usize,
    pub list_height: usize,
}

/// What a click at some spot lands on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    Sidebar,
    List,
    /// The arrow at the start of the header line, which shows the previous
    /// section.
    Previous,
    /// The rest of the header line, which shows the next section.
    Next,
}

impl Layout {
    /// The layout for a pane of `rows` by `cols`, whose sidebar lists
    /// `titles`.
    pub fn new(rows: usize, cols: usize, titles: &[&str]) -> Self {
        let body_height = rows.saturating_sub(HEADER_LINES + FOOTER_LINES);
        // "▸ " before a title and " ●" after it.
        let sidebar = titles
            .iter()
            .map(|title| title.chars().count() + 4)
            .max()
            .unwrap_or(0);
        // The sidebar's border and a space after it.
        let list_x = sidebar + 2;
        if cols >= list_x + MIN_LIST_COLUMNS {
            Layout {
                sidebar: Some(sidebar),
                body_y: HEADER_LINES,
                body_height,
                list_x,
                list_y: HEADER_LINES,
                list_width: cols - list_x,
                list_height: body_height,
            }
        } else {
            Layout {
                sidebar: None,
                body_y: HEADER_LINES,
                body_height: body_height.min(1),
                list_x: 0,
                list_y: HEADER_LINES + 1,
                list_width: cols,
                list_height: body_height.saturating_sub(1),
            }
        }
    }

    pub fn is_collapsed(&self) -> bool {
        self.sidebar.is_none()
    }

    /// What a click on `line` and `column` lands on, if anything.
    pub fn place(&self, line: usize, column: usize) -> Option<Place> {
        let in_body = (self.body_y..self.body_y + self.body_height).contains(&line);
        let in_list = (self.list_y..self.list_y + self.list_height).contains(&line);
        match self.sidebar {
            Some(sidebar) if in_body && column < sidebar => Some(Place::Sidebar),
            Some(_) if in_list && column >= self.list_x => Some(Place::List),
            None if in_body && column < 2 => Some(Place::Previous),
            None if in_body => Some(Place::Next),
            None if in_list => Some(Place::List),
            _ => None,
        }
    }
}

/// One line as text, with the parts to draw differently. Ranges and columns
/// count characters from the line's start.
#[derive(Debug, Default, PartialEq)]
pub struct Line {
    /// The text as it fits, cut with `…` when it does not.
    pub text: String,
    /// The characters of a value that is not set, so the default shows.
    pub dim: Option<Range<usize>>,
    /// The characters of the marker on an edited value.
    pub edited: Option<Range<usize>>,
    /// A rendered format string to draw over its plain text: the column it
    /// starts at, and its ANSI cut to as much as `text` shows of it.
    pub styled: Option<(usize, String)>,
    /// A swatch of a color to draw over the two spaces at this column.
    pub swatch: Option<(usize, Rgb)>,
}

impl Line {
    fn plain(text: String, width: usize) -> Self {
        Line {
            text: truncate(&text, width),
            ..Line::default()
        }
    }
}

/// A row of the list as a line `width` columns wide.
pub fn row_line(editor: &Editor, row: &Row, width: usize, view: FormatView) -> Line {
    let (key, kind, default) = match row {
        Row::Alias => {
            return Line::plain(
                format!(
                    "  {:NAME_COLUMNS$} {}  (Enter for the next of {})",
                    "alias",
                    editor.alias_name(),
                    editor.alias_count()
                ),
                width,
            )
        }
        Row::Add(prefix) => return Line::plain(format!("  + add {}", add_what(prefix)), width),
        Row::Option(option) => (option.name, option.kind, option.default),
        Row::Key(key) => (key.as_str(), options::kind_of(key), None),
    };
    let config = editor.working();
    let mut text = format!("  {key:NAME_COLUMNS$} ");
    let start = text.chars().count();
    let mut styled = None;
    let mut swatch = None;
    let mut dim = false;
    match editor.value(key) {
        Some(value) => {
            let rendered = (view == FormatView::Rendered && kind == Kind::Format)
                .then(|| render_sample(key, value, config))
                .flatten();
            match rendered {
                Some((plain, ansi)) => {
                    text.push_str(&plain);
                    styled = Some(ansi);
                }
                None => text.push_str(&show_value(value, kind)),
            }
            swatch = swatch_color(value, config);
        }
        None => {
            dim = true;
            match default {
                Some(default) => {
                    text.push_str(&show_value(default, kind));
                    swatch = swatch_color(default, config);
                }
                None => text.push_str("unset"),
            }
        }
    }
    let value_end = text.chars().count();
    let swatch = swatch.map(|color| {
        text.push_str("   ");
        (value_end + 1, color)
    });
    if dim && default.is_some() {
        text.push_str(" (default)");
    }
    let end = text.chars().count();
    let mut edited = None;
    if editor.is_edited(key) {
        text.push_str("  ●");
        edited = Some(end + 2..end + 3);
    }

    // What survives the cut: everything, or all but the last column, which
    // the `…` takes.
    let full = text.chars().count();
    let text = truncate(&text, width);
    let visible = text.chars().count();
    let shown = if full > width {
        width.saturating_sub(1)
    } else {
        full
    };
    Line {
        text,
        dim: dim.then(|| start.min(visible)..end.min(visible)),
        edited: edited.filter(|range| range.end <= visible),
        styled: styled.and_then(|ansi| {
            let room = value_end.min(shown).saturating_sub(start);
            (room > 0).then(|| (start, take_visible(&ansi, room)))
        }),
        swatch: swatch.filter(|(column, _)| column + 2 <= shown),
    }
}

/// What an add line adds: every family starting with `prefix`, or any key.
fn add_what(prefix: &str) -> String {
    if prefix.is_empty() {
        return "a key".to_string();
    }
    FAMILIES
        .iter()
        .filter(|family| family.prefix() == prefix)
        .map(|family| family.pattern)
        .collect::<Vec<_>>()
        .join(" or ")
}

/// A value as shown in the list. Text is always quoted, so its spaces show
/// and it reads apart from a number or a choice; anything else is quoted
/// only when it would otherwise be invisible or ambiguous.
pub fn show_value(value: &str, kind: Kind) -> String {
    if kind.is_text() || value.is_empty() || value.trim() != value {
        format!("\"{value}\"")
    } else {
        value.to_string()
    }
}

/// A format string as the bar draws it for `key`, with sample values for
/// the placeholders the bar fills in for that key: the plain text and the
/// ANSI. `None` when it draws nothing visible, which the raw text shows
/// better.
pub fn render_sample(
    key: &str,
    template: &str,
    config: &BTreeMap<String, String>,
) -> Option<(String, String)> {
    let samples = sample_values(key, config);
    let samples: Vec<(&str, &str)> = samples
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let parts = format::render_template(template, &samples, config, 0.0);
    let plain: String = parts.iter().map(|part| &**part).collect();
    // A control character, such as a newline or an escape, would break the
    // line apart; the raw text shows it quoted instead.
    if plain.trim().is_empty() || plain.chars().any(char::is_control) {
        return None;
    }
    Some((plain, ansi_term::ANSIStrings(&parts).to_string()))
}

/// The placeholders the bar fills in a format string under `key`, with
/// the sample value each gets in the list.
fn sample_values(key: &str, config: &BTreeMap<String, String>) -> Vec<(&'static str, String)> {
    if key == "key_format" || key.starts_with("key_format_") {
        // The keys as the bar composes them, so key and modifier aliases
        // show too.
        let sample = KeyWithModifier::new(BareKey::Char('p')).with_ctrl_modifier();
        vec![("key", crate::compose_key_text(&[sample], config))]
    } else if key == "desc_format" || key.starts_with("desc_format_") {
        vec![("desc", "Pane".to_string())]
    } else if key == "mode_format" {
        vec![("mode", "NORMAL".to_string())]
    } else if let Some(mode) = key.strip_prefix("mode_format_") {
        // The bar names a mode by its variant, which is the suffix without
        // underscores: enter_search is ENTERSEARCH.
        vec![("mode", mode.replace('_', "").to_uppercase())]
    } else {
        Vec::new()
    }
}

/// The color a value names, when it is a hex color or a `$name` alias for
/// one, so the list can show a swatch of it.
pub fn swatch_color(value: &str, config: &BTreeMap<String, String>) -> Option<Rgb> {
    let value = value.trim();
    let color = match value.strip_prefix('$') {
        Some(alias) => config.get(&format!("color_{alias}"))?.trim(),
        None => value,
    };
    let hex = color.strip_prefix('#')?;
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |digits: &str| u8::from_str_radix(digits, 16).ok();
    match hex.len() {
        // #rgb is #rrggbb with each digit doubled.
        3 => Some((
            channel(&hex[0..1])? * 17,
            channel(&hex[1..2])? * 17,
            channel(&hex[2..3])? * 17,
        )),
        6 => Some((
            channel(&hex[0..2])?,
            channel(&hex[2..4])?,
            channel(&hex[4..6])?,
        )),
        _ => None,
    }
}

/// A swatch of `color` two columns wide, as ANSI.
pub fn swatch_ansi((red, green, blue): Rgb) -> String {
    format!("\u{1b}[48;2;{red};{green};{blue}m  \u{1b}[0m")
}

/// The first `count` visible characters of an ANSI string, styles kept.
fn take_visible(ansi: &str, count: usize) -> String {
    crate::truncate_ansi_string(ansi, "", count, false)
}

/// A section of the sidebar as a line `width` columns wide: an arrow on the
/// one the list shows, and a marker when it holds unsaved edits.
pub fn sidebar_line(editor: &Editor, section: Section, width: usize) -> Line {
    let arrow = if section == editor.section() {
        "▸"
    } else {
        " "
    };
    let mut text = format!("{arrow} {}", section.title());
    let mut edited = None;
    if editor.section_is_edited(section) {
        let end = text.chars().count();
        text.push_str(" ●");
        edited = Some(end + 1..end + 2);
    }
    let text = truncate(&text, width);
    let visible = text.chars().count();
    Line {
        text,
        edited: edited.filter(|range| range.end <= visible),
        ..Line::default()
    }
}

/// The line standing in for the sidebar in a narrow pane: the section the
/// list shows, between arrows to the ones either side of it.
pub fn header_line(editor: &Editor, width: usize) -> Line {
    let sections = editor.sections();
    let section = editor.section();
    let mut text = format!("◀ {} ▶", section.title());
    let end = text.chars().count();
    let mut edited = None;
    if editor.section_is_edited(section) {
        text.push_str(" ●");
        edited = Some(end + 1..end + 2);
    }
    text.push_str(&format!(
        "  {} of {}",
        editor.section_index() + 1,
        sections.len()
    ));
    let text = truncate(&text, width);
    let visible = text.chars().count();
    Line {
        text,
        edited: edited.filter(|range| range.end <= visible),
        ..Line::default()
    }
}

/// The screen's first line: what is being edited, how format strings are
/// shown, and whether there is anything unsaved.
pub fn title(editor: &Editor, view: FormatView) -> String {
    let shown = match view {
        FormatView::Rendered => "rendered",
        FormatView::Raw => "raw",
    };
    let mut title = format!(
        "zjhints settings · alias \"{}\" · formats {shown}",
        editor.alias_name()
    );
    if editor.is_dirty() {
        title.push_str("  ● unsaved");
    }
    title
}

/// The footer line about what is selected on the focused side.
pub fn describe(editor: &Editor) -> String {
    if editor.focus == Focus::Sidebar {
        return match editor.section() {
            Section::Alias => "Which zjhints alias to edit. Enter shows it.".to_string(),
            Section::Group(group) => format!("The {group} options. Enter shows them."),
            Section::Other => {
                "Keys zjhints does not read as options. Enter shows them.".to_string()
            }
        };
    }
    match editor.selected_row() {
        Some(Row::Option(option)) => {
            let default = option.default.unwrap_or("unset");
            let how = match option.kind {
                Kind::Bool => "Enter toggles".to_string(),
                Kind::Choice(choices) => format!("Enter cycles {}", choices.join(" / ")),
                Kind::Integer | Kind::Fraction | Kind::Text | Kind::Format => {
                    "Enter edits".to_string()
                }
            };
            format!("{}. Default {default}. {how}.", option.description)
        }
        Some(Row::Key(key)) => match options::family_of(&key) {
            Some(family) => format!(
                "{} ({}). Del removes it.",
                family.description, family.pattern
            ),
            None => "Not an option zjhints reads. Del removes it.".to_string(),
        },
        Some(Row::Add("")) => format!(
            "Add a key no other group lists. It is listed under {OTHER_HEADING} unless a group fits it."
        ),
        Some(Row::Add(prefix)) => {
            let family = FAMILIES.iter().find(|family| family.prefix() == prefix);
            let what = family.map_or("", |family| family.description);
            format!("Add a key starting {prefix}. {what}.")
        }
        Some(Row::Alias) => "The plugins {} alias being edited.".to_string(),
        None => String::new(),
    }
}

/// The keys to list in the footer, for the side that has focus and how
/// format strings are shown.
pub fn help_keys(
    focus: Focus,
    collapsed: bool,
    view: FormatView,
) -> Vec<(&'static str, &'static str)> {
    let toggle = match view {
        FormatView::Rendered => ("r", "raw formats"),
        FormatView::Raw => ("r", "rendered formats"),
    };
    match (focus, collapsed) {
        (_, true) => vec![
            ("↑↓", "move"),
            ("←→", "group"),
            ("Enter", "change"),
            ("Del", "reset"),
            toggle,
            ("s", "save"),
            ("Esc", "close"),
        ],
        (Focus::Sidebar, false) => vec![
            ("↑↓", "group"),
            ("→", "options"),
            toggle,
            ("s", "save"),
            ("Esc", "close"),
        ],
        (Focus::List, false) => vec![
            ("↑↓", "move"),
            ("←", "groups"),
            ("Enter", "change"),
            ("Del", "reset"),
            toggle,
            ("s", "save"),
            ("Esc", "close"),
        ],
    }
}

/// `text` cut to `width` characters, ending in `…` when it was cut.
pub fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(width.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::editor::Field;
    use crate::settings::options::GROUPS;
    use zellij_tile::prelude::{PluginAliasEntry, PluginEntry};

    fn editor(configuration: &[(&str, &str)]) -> Editor {
        Editor::new(
            vec![PluginAliasEntry {
                name: "zjhints".to_string(),
                plugin: PluginEntry {
                    location: "file:~/zjhints.wasm".to_string(),
                    cwd: None,
                    configuration: configuration
                        .iter()
                        .map(|(key, value)| (key.to_string(), value.to_string()))
                        .collect(),
                },
            }],
            None,
        )
        .unwrap()
    }

    fn option_row(name: &str) -> Row {
        Row::Option(options::find(name).unwrap())
    }

    fn key_row(key: &str) -> Row {
        Row::Key(key.to_string())
    }

    /// The text of `row` in an 80-column list, formats shown raw.
    fn raw(editor: &Editor, row: &Row) -> String {
        row_line(editor, row, 80, FormatView::Raw).text
    }

    #[test]
    fn an_unset_option_shows_its_default_dimmed() {
        let line = row_line(&editor(&[]), &option_row("auto_width"), 80, FormatView::Raw);
        assert_eq!(line.text, format!("  {:22}true (default)", "auto_width"));
        assert_eq!(line.dim, Some(24..38));
        assert_eq!(line.edited, None);
        assert_eq!(line.swatch, None);
    }

    #[test]
    fn a_set_option_shows_its_value_plainly() {
        let line = row_line(
            &editor(&[("max_length", "40")]),
            &option_row("max_length"),
            80,
            FormatView::Raw,
        );
        assert_eq!(line.text, format!("  {:22}40", "max_length"));
        assert_eq!(line.dim, None);
    }

    #[test]
    fn text_is_always_quoted_and_nothing_else_is() {
        let editor = editor(&[
            ("pipe_name", "bar"),
            ("label_quit", "bye"),
            ("key_order", "dvorak"),
            ("dim_strength", "0.25"),
            ("key_format", "{key}"),
        ]);
        let line = row_line(&editor, &option_row("overflow_str"), 80, FormatView::Raw);
        assert_eq!(
            line.text,
            format!("  {:22}\"...\" (default)", "overflow_str")
        );
        assert_eq!(line.dim, Some(24..39));
        assert_eq!(
            raw(&editor, &option_row("pipe_name")),
            format!("  {:22}\"bar\"", "pipe_name")
        );
        assert_eq!(
            raw(&editor, &key_row("label_quit")),
            format!("  {:22}\"bye\"", "label_quit")
        );
        assert_eq!(
            raw(&editor, &option_row("key_format")),
            format!("  {:22}\"{{key}}\"", "key_format")
        );
        assert_eq!(
            raw(&editor, &option_row("key_order")),
            format!("  {:22}dvorak", "key_order")
        );
        assert_eq!(
            raw(&editor, &option_row("dim_strength")),
            format!("  {:22}0.25", "dim_strength")
        );
        assert_eq!(show_value("", Kind::Integer), "\"\"");
        assert_eq!(show_value(" 1", Kind::Integer), "\" 1\"");
    }

    #[test]
    fn an_edited_value_is_marked() {
        let mut editor = editor(&[]);
        editor.submit(
            &Field::Value {
                key: "hint_spacer".to_string(),
            },
            " | ",
        );
        let line = row_line(&editor, &option_row("hint_spacer"), 80, FormatView::Raw);
        assert_eq!(line.text, format!("  {:22}\" | \"  ●", "hint_spacer"));
        assert_eq!(line.edited, Some(31..32));
        assert!(title(&editor, FormatView::Raw).ends_with("● unsaved"));
    }

    #[test]
    fn a_narrow_pane_truncates_rows_and_drops_markers_cut_off() {
        let mut editor = editor(&[]);
        editor.submit(
            &Field::Value {
                key: "hint_spacer".to_string(),
            },
            "|",
        );
        for view in [FormatView::Raw, FormatView::Rendered] {
            let line = row_line(&editor, &option_row("hint_spacer"), 20, view);
            assert_eq!(line.text.chars().count(), 20);
            assert!(line.text.ends_with('…'));
            assert_eq!(line.edited, None);
            assert_eq!(line.styled, None);
        }
    }

    #[test]
    fn a_key_longer_than_the_name_column_keeps_a_space_before_its_value() {
        let key = "label_locked_switch_to_mode_normal";
        let line = row_line(
            &editor(&[(key, "Unlock")]),
            &key_row(key),
            80,
            FormatView::Raw,
        );
        assert_eq!(line.text, format!("  {key} \"Unlock\""));
    }

    #[test]
    fn add_lines_name_the_families_they_add() {
        let editor = editor(&[]);
        assert_eq!(raw(&editor, &Row::Add("color_")), "  + add color_<name>");
        assert_eq!(
            raw(&editor, &Row::Add("label_")),
            "  + add label_<id> or label_<mode>_<id>"
        );
        assert_eq!(raw(&editor, &Row::Add("")), "  + add a key");
    }

    #[test]
    fn formats_are_rendered_unless_raw_is_asked_for() {
        let editor = editor(&[
            ("key_format", "#[fg=#ff0000,bold] {key} "),
            ("desc_format", "[{desc}]"),
            ("mode_format", "<{mode}>"),
            ("mode_format_enter_search", "{mode}"),
            ("hint_spacer", "#[fg=red]|"),
            ("label_quit", "{key}"),
        ]);
        let rendered = |row: &Row| row_line(&editor, row, 80, FormatView::Rendered);

        let line = rendered(&option_row("key_format"));
        assert_eq!(line.text, format!("  {:22} Ctrl p ", "key_format"));
        let (column, ansi) = line.styled.unwrap();
        assert_eq!(column, 24);
        assert!(ansi.contains("38;2;255;0;0"));
        assert!(ansi.contains(" Ctrl p "));

        let text = |row: &Row| rendered(row).text[24..].to_string();
        assert_eq!(text(&option_row("desc_format")), "[Pane]");
        assert_eq!(text(&option_row("mode_format")), "<NORMAL>");
        assert_eq!(
            rendered(&key_row("mode_format_enter_search")).text,
            "  mode_format_enter_search ENTERSEARCH"
        );
        assert_eq!(text(&option_row("hint_spacer")), "|");
        // Labels are text, not format strings, whatever they hold.
        assert_eq!(text(&key_row("label_quit")), "\"{key}\"");

        let line = row_line(&editor, &option_row("key_format"), 80, FormatView::Raw);
        assert_eq!(
            line.text,
            format!("  {:22}\"#[fg=#ff0000,bold] {{key}} \"", "key_format")
        );
        assert_eq!(line.styled, None);
    }

    #[test]
    fn a_rendered_key_uses_the_modifier_aliases_being_edited() {
        let editor = editor(&[("mod_alias_ctrl", "^"), ("key_format", "{key}")]);
        let line = row_line(&editor, &option_row("key_format"), 80, FormatView::Rendered);
        assert_eq!(line.text, format!("  {:22}^p", "key_format"));
    }

    #[test]
    fn a_format_that_draws_nothing_visible_shows_raw() {
        let editor = editor(&[("hint_spacer", "  "), ("desc_format", "#[fg=red]")]);
        let text = |name| row_line(&editor, &option_row(name), 80, FormatView::Rendered).text;
        assert_eq!(text("hint_spacer"), format!("  {:22}\"  \"", "hint_spacer"));
        assert_eq!(
            text("desc_format"),
            format!("  {:22}\"#[fg=red]\"", "desc_format")
        );
    }

    #[test]
    fn a_rendered_value_is_cut_by_visible_characters() {
        let editor = editor(&[("key_format", "#[fg=#ff0000]abcdefghij")]);
        let line = row_line(&editor, &option_row("key_format"), 30, FormatView::Rendered);
        assert_eq!(line.text, format!("  {:22}abcde…", "key_format"));
        let (column, ansi) = line.styled.unwrap();
        assert_eq!(column, 24);
        assert!(ansi.contains("abcde"));
        assert!(!ansi.contains('f'));
        assert!(ansi.ends_with("\u{1b}[0m"));
    }

    #[test]
    fn hex_colors_and_aliases_for_them_get_a_swatch() {
        let config: BTreeMap<String, String> = [
            ("color_blue", "#0000ff"),
            ("color_red", "red"),
            ("color_short", "#0f0"),
        ]
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
        assert_eq!(swatch_color("#ff0000", &config), Some((255, 0, 0)));
        assert_eq!(swatch_color("#F00", &config), Some((255, 0, 0)));
        assert_eq!(swatch_color(" #00ff00 ", &config), Some((0, 255, 0)));
        assert_eq!(swatch_color("$blue", &config), Some((0, 0, 255)));
        assert_eq!(swatch_color("$short", &config), Some((0, 255, 0)));
        assert_eq!(swatch_color("$red", &config), None);
        assert_eq!(swatch_color("$missing", &config), None);
        assert_eq!(swatch_color("red", &config), None);
        assert_eq!(swatch_color("#12", &config), None);
        assert_eq!(swatch_color("#1234", &config), None);
        assert_eq!(swatch_color("#gggggg", &config), None);
        assert_eq!(swatch_color("#[fg=#ff0000]", &config), None);
        assert_eq!(
            swatch_ansi((1, 2, 3)),
            "\u{1b}[48;2;1;2;3m  \u{1b}[0m".to_string()
        );
    }

    #[test]
    fn a_swatch_sits_after_the_value_while_it_fits() {
        let editor = editor(&[("color_blue", "#0000ff"), ("color_accent", "$blue")]);
        let line = row_line(&editor, &key_row("color_blue"), 80, FormatView::Raw);
        assert_eq!(line.text, format!("  {:22}\"#0000ff\"   ", "color_blue"));
        assert_eq!(line.swatch, Some((34, (0, 0, 255))));
        let line = row_line(&editor, &key_row("color_accent"), 80, FormatView::Raw);
        assert_eq!(line.swatch, Some((32, (0, 0, 255))));
        let line = row_line(&editor, &key_row("color_blue"), 35, FormatView::Raw);
        assert_eq!(line.swatch, None);
    }

    #[test]
    fn a_wide_pane_has_a_sidebar_beside_the_list() {
        let layout = Layout::new(30, 120, &GROUPS);
        // "Keys and ordering" is the longest title.
        assert_eq!(layout.sidebar, Some(21));
        assert_eq!((layout.list_x, layout.list_width), (23, 97));
        assert_eq!((layout.list_y, layout.list_height), (2, 25));
        assert_eq!(layout.place(5, 3), Some(Place::Sidebar));
        assert_eq!(layout.place(5, 30), Some(Place::List));
        assert_eq!(layout.place(5, 21), None);
        assert_eq!(layout.place(0, 3), None);
        assert_eq!(layout.place(28, 30), None);
        assert!(!Layout::new(30, 63, &GROUPS).is_collapsed());
    }

    #[test]
    fn a_narrow_pane_folds_the_sidebar_into_a_header_line() {
        let layout = Layout::new(30, 62, &GROUPS);
        assert!(layout.is_collapsed());
        assert_eq!((layout.list_x, layout.list_width), (0, 62));
        assert_eq!((layout.list_y, layout.list_height), (3, 24));
        assert_eq!(layout.place(2, 0), Some(Place::Previous));
        assert_eq!(layout.place(2, 10), Some(Place::Next));
        assert_eq!(layout.place(3, 0), Some(Place::List));

        let mut editor = editor(&[("color_blue", "#0000ff")]);
        assert_eq!(header_line(&editor, 61).text, "◀ Fitting ▶  1 of 13");
        editor.cycle_section(-1);
        assert_eq!(header_line(&editor, 61).text, "◀ Other ▶  13 of 13");

        let keys = help_keys(Focus::List, true, FormatView::Rendered);
        assert!(keys.contains(&("←→", "group")));
    }

    #[test]
    fn the_sidebar_marks_the_section_shown_and_unsaved_edits() {
        let mut editor = editor(&[]);
        editor.submit(
            &Field::Value {
                key: "color_blue".to_string(),
            },
            "#0000ff",
        );
        assert_eq!(editor.section(), Section::Group("Colors"));
        let line = sidebar_line(&editor, Section::Group("Colors"), 20);
        assert_eq!(line.text, "▸ Colors ●");
        assert_eq!(line.edited, Some(9..10));
        let line = sidebar_line(&editor, Section::Group("Fitting"), 20);
        assert_eq!(line.text, "  Fitting");
        assert_eq!(line.edited, None);
    }

    #[test]
    fn the_footer_names_the_keys_for_the_focused_side_and_the_view() {
        let sidebar = help_keys(Focus::Sidebar, false, FormatView::Rendered);
        assert!(sidebar.contains(&("→", "options")));
        assert!(sidebar.contains(&("r", "raw formats")));
        let list = help_keys(Focus::List, false, FormatView::Raw);
        assert!(list.contains(&("←", "groups")));
        assert!(list.contains(&("r", "rendered formats")));
        assert!(title(&editor(&[]), FormatView::Raw).contains("formats raw"));
        assert!(title(&editor(&[]), FormatView::Rendered).contains("formats rendered"));
    }

    #[test]
    fn the_footer_explains_the_selected_row() {
        let mut editor = editor(&[("color_blue", "#0000ff")]);
        editor.submit(&Field::NewKey, "key_order");
        let text = describe(&editor);
        assert!(text.starts_with("How keys within a hint are ordered. Default qwerty."));
        assert!(text.contains("qwerty / dvorak"));
        editor.submit(&Field::NewKey, "color_blue");
        assert!(describe(&editor).contains("color_<name>"));
        editor.focus = Focus::Sidebar;
        assert_eq!(describe(&editor), "The Colors options. Enter shows them.");
    }
}
