//! What the settings screen shows and where: the layout for a pane size, and
//! each row as the widget that edits it, its marker and its swatch. Pure
//! functions, so the screen module only has to draw what these return, and
//! the images of the screen in the docs draw the same.

use super::editor::{Editor, Focus, Row, Section, OTHER_HEADING};
use super::schema::{Kind, Schema};
use crate::format;
use std::collections::BTreeMap;
use zellij_tile::prelude::{BareKey, ModeInfo, Styling};

/// Rows above the body: the config file, a blank row, the unsaved count with
/// the buttons, and a blank row.
pub const HEADER_ROWS: usize = 4;
/// Rows below the body: a blank row, the description or a notice, a blank
/// row, and the key help.
pub const FOOTER_ROWS: usize = 4;
/// The smallest pane the screen draws in.
pub const MIN_ROWS: usize = 12;
pub const MIN_COLS: usize = 50;
/// What precedes the config file's path in the header.
pub const FILE_PREFIX: &str = "File: ";
/// Columns the menu takes beyond its longest item.
const MENU_PADDING: usize = 4;
/// The fewest columns the menu takes.
const MIN_MENU_WIDTH: usize = 16;
/// The most columns the rows take beside the menu. A wider pane centers the
/// screen.
const CONTENT_WIDTH: usize = 80;
/// The most columns a row's label takes, its two spaces of padding included.
/// A longer key is cut, and the description under the rows names it whole.
pub const MAX_LABEL_WIDTH: usize = 32;
/// The fewest and most columns a value's field takes.
pub const MIN_VALUE_WIDTH: usize = 10;
const MAX_VALUE_WIDTH: usize = 40;
/// The fewest columns a field for typed text takes when there is room.
pub const TEXT_FIELD_WIDTH: usize = 24;
/// Columns a color swatch takes before the marker: the swatch and a gap.
pub const SWATCH_WIDTH: usize = 4;
/// Rows the preview takes under the list: a blank row, its caption and the
/// bar.
pub const PREVIEW_ROWS: usize = 3;

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
    /// The column the screen starts at, centered in a wide pane.
    pub x: usize,
    /// Columns the screen takes.
    pub width: usize,
    /// Columns of the menu down the left, or of the search box in its place.
    pub menu_width: usize,
    /// The row the menu and the list start on.
    pub body_y: usize,
    /// Rows the menu has.
    pub body_height: usize,
    pub content_x: usize,
    pub content_width: usize,
    /// Rows the list has.
    pub list_height: usize,
    /// The row the preview's caption is on, the bar on the row after it,
    /// when the list has a preview and there is room for it.
    pub preview_y: Option<usize>,
    /// The first row under the body. The description or notice is the row
    /// after it, and the key help two below that.
    pub footer_y: usize,
}

impl Layout {
    /// The layout for a pane of `rows` by `cols`, whose menu lists `items`,
    /// or `None` when the pane is too small to draw in. With `preview_rows`,
    /// the list of that many rows has a preview right under it.
    pub fn new(
        rows: usize,
        cols: usize,
        items: &[String],
        preview_rows: Option<usize>,
    ) -> Option<Self> {
        if rows < MIN_ROWS || cols < MIN_COLS {
            return None;
        }
        let menu_width = (items.iter().map(|item| columns(item)).max().unwrap_or(0) + MENU_PADDING)
            .min(cols / 3)
            .max(MIN_MENU_WIDTH);
        let width = (menu_width + 2 + CONTENT_WIDTH).min(cols);
        let x = (cols - width) / 2;
        let body_height = rows - HEADER_ROWS - FOOTER_ROWS;
        let mut layout = Layout {
            x,
            width,
            menu_width,
            body_y: HEADER_ROWS,
            body_height,
            content_x: x + menu_width + 2,
            content_width: width - menu_width - 2,
            list_height: body_height,
            preview_y: None,
            footer_y: rows - FOOTER_ROWS,
        };
        if let Some(count) = preview_rows {
            // Too short a pane keeps the list over the preview.
            if body_height > PREVIEW_ROWS {
                layout.list_height = count.clamp(1, body_height - PREVIEW_ROWS);
                layout.preview_y = Some(layout.body_y + layout.list_height + 1);
            }
        }
        Some(layout)
    }
}

/// The widget a row is edited with, and what it shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    /// A switch, for a boolean.
    Toggle(bool),
    /// A list to pick from, and the index of the value in effect.
    Dropdown(Vec<String>, usize),
    /// A field to type in. `placeholder` when the value is not set, so
    /// `text` is what stands in for it, drawn dimmed.
    Text { text: String, placeholder: bool },
    /// A button, for adding a key.
    Button(String),
    /// The title of the section the search results under it come from.
    Heading(String),
}

/// What a row says about where its value comes from, after the field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Marker {
    /// Set, and saved as it is.
    None,
    /// Differs from the config file.
    Unsaved,
    /// Not set, so the built-in default applies.
    Default,
    /// Not set, so the selected theme's value applies.
    Theme,
}

impl Marker {
    pub fn text(self) -> &'static str {
        match self {
            Marker::None => "",
            Marker::Unsaved => "● unsaved",
            Marker::Default => "○ default",
            Marker::Theme => "○ theme",
        }
    }
}

/// One row of the list as the screen draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct RowView {
    /// The key, as the label left of the field.
    pub label: String,
    pub control: Control,
    pub marker: Marker,
    /// A color the value names, drawn between the field and the marker.
    pub swatch: Option<Rgb>,
    /// A rendered format string to draw over the field's text, as ANSI.
    /// The field holds the same text plain.
    pub styled: Option<String>,
}

impl RowView {
    /// A row with no marker, swatch or styling.
    pub fn plain(label: impl Into<String>, control: Control) -> Self {
        RowView {
            label: label.into(),
            control,
            marker: Marker::None,
            swatch: None,
            styled: None,
        }
    }

    /// Whether the row is typed into, which Enter starts rather than Space.
    pub fn is_text(&self) -> bool {
        matches!(self.control, Control::Text { .. })
    }

    /// Columns the swatch and marker take after the field.
    fn trailer_width(&self) -> usize {
        let swatch = if self.swatch.is_some() {
            SWATCH_WIDTH
        } else {
            0
        };
        swatch + columns(self.marker.text())
    }
}

/// What the rows of one render are drawn against: the editor, how format
/// strings are shown, and the configuration the bar would read, with the
/// selected theme laid under the user's own values. Made once per render,
/// since laying the theme copies the configuration.
pub struct Rows<'a> {
    editor: &'a Editor,
    view: FormatView,
    /// What samples and swatches resolve `$name` aliases against. Only for
    /// showing: what is saved is `Editor::working`, the user's own values.
    layered: BTreeMap<String, String>,
}

impl<'a> Rows<'a> {
    /// `zellij_palette` is the running Zellij theme, which the default
    /// `color_theme` takes its colors from.
    pub fn new(editor: &'a Editor, view: FormatView, zellij_palette: Option<&Styling>) -> Self {
        Rows {
            editor,
            view,
            layered: (editor.schema().layer)(editor.working(), zellij_palette),
        }
    }

    /// How `row` is drawn.
    pub fn view(&self, row: &Row) -> RowView {
        let editor = self.editor;
        let (key, kind, default) = match row {
            Row::Alias => {
                let (names, chosen) = editor.choices_of(row).unwrap_or_default();
                let names = names
                    .iter()
                    .map(|name| crate::ansi::show_controls(name).into_owned())
                    .collect();
                return RowView::plain("alias", Control::Dropdown(names, chosen));
            }
            Row::Add(pattern) => {
                let what = if pattern.is_empty() { "a key" } else { pattern };
                return RowView::plain("", Control::Button(format!("+ add {what}")));
            }
            Row::Heading(title) => return RowView::plain("", Control::Heading(title.to_string())),
            Row::Option(option) => (option.name, option.kind, option.default),
            Row::Key(key) => (key.as_str(), editor.schema().kind_of(key), None),
        };
        let config = &self.layered;
        let set = editor.value(key);
        let themed = config
            .get(key)
            .map(String::as_str)
            .filter(|_| set.is_none());
        let shown = set.or(themed).or(default);
        let marker = if editor.is_edited(key) {
            Marker::Unsaved
        } else if set.is_some() {
            Marker::None
        } else if themed.is_some() {
            Marker::Theme
        } else {
            Marker::Default
        };
        let mut view = RowView {
            label: crate::ansi::show_controls(key).into_owned(),
            control: Control::Text {
                text: String::new(),
                placeholder: false,
            },
            marker,
            swatch: shown.and_then(|value| swatch_color(value, config)),
            styled: None,
        };
        view.control = match kind {
            Kind::Bool => Control::Toggle(shown == Some("true")),
            Kind::Choice(_) => {
                let (values, chosen) = editor.choices_of(row).unwrap_or_default();
                Control::Dropdown(values, chosen)
            }
            Kind::Integer | Kind::Fraction | Kind::Text | Kind::Format => {
                let rendered = (self.view == FormatView::Rendered && kind == Kind::Format)
                    .then(|| {
                        shown.and_then(|value| render_sample(editor.schema(), key, value, config))
                    })
                    .flatten();
                let text = match rendered {
                    Some((plain, ansi)) => {
                        view.styled = Some(ansi);
                        plain
                    }
                    None => shown.map_or("unset".to_string(), |value| show_value(value, kind)),
                };
                Control::Text {
                    text,
                    placeholder: set.is_none(),
                }
            }
        };
        view
    }
}

/// The columns of the label and of the field for rows `views` in a list
/// `width` columns wide: as wide as the longest label and value need, within
/// limits, and narrower when the swatches and markers would not fit.
pub fn field_widths(views: &[RowView], width: usize) -> (usize, usize) {
    let fields: Vec<&RowView> = views
        .iter()
        .filter(|view| !matches!(view.control, Control::Heading(_) | Control::Button(_)))
        .collect();
    let label_width = (fields
        .iter()
        .map(|view| columns(&view.label))
        .max()
        .unwrap_or(0)
        + 2)
    .min(MAX_LABEL_WIDTH);
    let widest_value = fields
        .iter()
        .map(|view| match &view.control {
            Control::Dropdown(values, _) => values.iter().map(|value| columns(value)).max(),
            Control::Text { text, .. } => Some(columns(text).max(TEXT_FIELD_WIDTH - 4)),
            _ => None,
        })
        .max()
        .flatten()
        .unwrap_or(0);
    let widest_trailer = fields
        .iter()
        .map(|view| view.trailer_width())
        .max()
        .unwrap_or(0);
    let value_width = (widest_value + 4)
        .clamp(MIN_VALUE_WIDTH, MAX_VALUE_WIDTH)
        .min(width.saturating_sub(label_width + widest_trailer + 2))
        .max(MIN_VALUE_WIDTH);
    (label_width, value_width)
}

/// Where a row's swatch and marker go, from the column the field ends at:
/// the swatch two columns after it, the marker after the swatch, or the
/// marker pulled back to end at the list's edge when it would run past it.
pub fn trailer_columns(view: &RowView, field_end: usize, list_end: usize) -> (usize, usize) {
    let swatch = field_end + 2;
    let marker = swatch
        + if view.swatch.is_some() {
            SWATCH_WIDTH
        } else {
            0
        };
    let marker_width = columns(view.marker.text());
    let marker = if marker + marker_width <= list_end {
        marker
    } else {
        list_end.saturating_sub(marker_width)
    };
    (swatch, marker)
}

/// A value as shown in its field. Text is always quoted, so its spaces show
/// and it reads apart from a number or a choice; anything else is quoted
/// only when it would otherwise be invisible or ambiguous.
pub fn show_value(value: &str, kind: Kind) -> String {
    // Escaped, so an ESC in a value draws as text instead of moving the
    // cursor or rewriting other rows.
    let shown = crate::ansi::show_controls(value);
    if kind.is_text() || value.is_empty() || value.trim() != value {
        format!("\"{shown}\"")
    } else {
        shown.into_owned()
    }
}

/// A format string as the bar draws it for `key`, with sample values for
/// the placeholders the bar fills in for that key: the plain text and the
/// ANSI. `None` when it draws nothing visible, which the raw text shows
/// better.
pub fn render_sample(
    schema: &Schema,
    key: &str,
    template: &str,
    config: &BTreeMap<String, String>,
) -> Option<(String, String)> {
    let samples = (schema.sample_values)(key, config);
    let samples: Vec<(&str, &str)> = samples
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let parts = format::render_template(template, &samples, config, 0.0);
    let plain: String = parts.iter().map(|part| &**part).collect();
    // A control character, such as a newline or an escape, would break the
    // line apart; the raw text shows it escaped instead.
    if plain.trim().is_empty() || plain.chars().any(char::is_control) {
        return None;
    }
    Some((plain, ansi_term::ANSIStrings(&parts).to_string()))
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
    format!("\u{1b}[38;2;{red};{green};{blue}m██\u{1b}[0m")
}

/// The first `count` visible characters of an ANSI string, styles kept.
pub fn take_visible(ansi: &str, count: usize) -> String {
    crate::ansi::truncate_ansi_string(ansi, "", count, false)
}

/// The menu's items: each section's title, with ` ●` after one holding
/// unsaved edits.
pub fn menu_items(editor: &Editor) -> Vec<String> {
    editor
        .sections()
        .into_iter()
        .map(|section| {
            if editor.section_is_edited(section) {
                format!("{} ●", section.title())
            } else {
                section.title().to_string()
            }
        })
        .collect()
}

/// "N unsaved changes", as the header starts.
pub fn unsaved_text(count: usize) -> String {
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} unsaved change{plural}")
}

/// What the screen does when a header button is clicked or its key pressed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Save,
    RevertAll,
    ToggleFormats,
    Close,
}

/// The buttons after the unsaved count, in order: the key, what it does,
/// and the command it runs.
pub fn header_buttons(view: FormatView) -> [(&'static str, &'static str, Command); 4] {
    let formats = match view {
        FormatView::Rendered => "raw formats",
        FormatView::Raw => "rendered formats",
    };
    [
        ("<Ctrl a>", "save", Command::Save),
        ("<Ctrl r>", "revert all", Command::RevertAll),
        ("<f>", formats, Command::ToggleFormats),
        ("<Esc>", "close", Command::Close),
    ]
}

/// The label of the button that shows or hides the overrides in the
/// preview.
pub fn overrides_button(shown: bool) -> (&'static str, &'static str) {
    if shown {
        ("<o>", "hide overrides")
    } else {
        ("<o>", "show overrides")
    }
}

/// The line under the rows about the selected one, while the list has focus.
pub fn describe(editor: &Editor) -> Option<String> {
    if editor.focus != Focus::List {
        return None;
    }
    let text = match editor.selected_row()? {
        Row::Option(option) => format!(
            "{} (default: {})",
            option.description,
            option.default.unwrap_or("unset")
        ),
        Row::Key(key) => match editor.schema().family_of(&key) {
            Some(family) => format!(
                "{}: {} ({}). Del removes it.",
                crate::ansi::show_controls(&key),
                family.description,
                family.pattern
            ),
            None => format!(
                "{}: not an option {} reads. Del removes it.",
                crate::ansi::show_controls(&key),
                editor.schema().plugin
            ),
        },
        Row::Add("") => format!(
            "Add a key no other group lists. It is listed under {OTHER_HEADING} unless a group fits it."
        ),
        Row::Add(pattern) => {
            let family = editor.schema().families.iter().find(|family| family.pattern == pattern);
            let what = family.map_or("", |family| family.description);
            format!("Add a {pattern} key. {what}.")
        }
        Row::Alias => "The plugins {} alias being edited.".to_string(),
        Row::Heading(_) => return None,
    };
    Some(text)
}

/// "N matches", under the search box.
pub fn match_text(count: usize) -> String {
    match count {
        1 => "1 match".to_string(),
        count => format!("{count} matches"),
    }
}

/// What the keys do right now, for the help line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HelpState {
    /// Typing into a field.
    Editing,
    /// A dropdown's list is open.
    Choosing,
    /// The part of the screen with focus, the selected row's view, and
    /// whether a search is showing.
    Browsing {
        focus: Focus,
        text_row: bool,
        searching: bool,
    },
}

/// The keys the help line lists.
pub fn help_hints(state: HelpState) -> Vec<(&'static str, &'static str)> {
    match state {
        HelpState::Editing => vec![("<Enter>", "apply"), ("<Esc>", "cancel")],
        HelpState::Choosing => vec![
            ("<↓↑>", "choose"),
            ("<Enter>", "select"),
            ("<Esc>", "cancel"),
        ],
        HelpState::Browsing {
            focus: Focus::Sidebar,
            ..
        } => vec![
            ("<↓↑>", "category"),
            ("<Tab>", "settings"),
            ("</>", "search"),
            ("<Esc>", "close"),
        ],
        HelpState::Browsing {
            focus: Focus::Search,
            ..
        } => vec![("<↓>", "results"), ("<Esc>", "end search")],
        HelpState::Browsing {
            focus: Focus::List,
            text_row,
            searching,
        } => {
            let change = if text_row {
                ("<Enter>", "edit")
            } else {
                ("<Space>", "change")
            };
            let (back, close) = if searching {
                (("<←>", "search"), ("<Esc>", "end search"))
            } else {
                (("<←>", "categories"), ("<Esc>", "close"))
            };
            vec![
                ("<↓↑>", "move"),
                change,
                ("<r>", "revert"),
                ("<Del>", "unset"),
                back,
                ("</>", "search"),
                close,
            ]
        }
    }
}

/// The key a help line button presses when clicked, from the key it names.
/// `<↓↑>` moves down, as clicking it again keeps doing.
pub fn hint_press(key: &str) -> Option<BareKey> {
    let bare = match key {
        "<↓↑>" | "<↓>" => BareKey::Down,
        "<Enter>" => BareKey::Enter,
        "<Esc>" => BareKey::Esc,
        "<Tab>" => BareKey::Tab,
        "<Space>" => BareKey::Char(' '),
        "<Del>" => BareKey::Delete,
        "<←>" => BareKey::Left,
        "</>" => BareKey::Char('/'),
        "<r>" => BareKey::Char('r'),
        _ => return None,
    };
    Some(bare)
}

/// The keys of `configuration` that take the place of a value the selected
/// themes set, in key order.
pub fn theme_overrides(
    schema: &Schema,
    configuration: &BTreeMap<String, String>,
    zellij_palette: Option<&Styling>,
) -> Vec<String> {
    let is_theme = |key: &str| crate::theme::OPTIONS.contains(&key);
    let themes: BTreeMap<String, String> = configuration
        .iter()
        .filter(|(key, _)| is_theme(key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let themed = (schema.layer)(&themes, zellij_palette);
    configuration
        .keys()
        .filter(|key| !is_theme(key) && themed.contains_key(*key))
        .cloned()
        .collect()
}

/// The bar as `configuration` draws it `width` columns wide, in the mode,
/// keybindings and Zellij theme of `mode_info`, and with the user's
/// overrides of the themes left out unless `overrides_shown`.
pub fn theme_preview(
    schema: &Schema,
    configuration: &BTreeMap<String, String>,
    mode_info: &ModeInfo,
    overrides_shown: bool,
    width: usize,
) -> String {
    let palette = mode_info.style.colors;
    let mut shown = configuration.clone();
    if !overrides_shown {
        for key in theme_overrides(schema, configuration, Some(&palette)) {
            shown.remove(&key);
        }
    }
    (schema.draw_bar)(shown, mode_info, width)
}

/// The line over the preview, saying whether it draws the user's overrides
/// of the themes and how many there are.
pub fn preview_caption(overrides: usize, shown: bool) -> String {
    let noun = if overrides == 1 {
        "override"
    } else {
        "overrides"
    };
    match (overrides, shown) {
        (0, _) => "Preview · no overrides".to_string(),
        (_, true) => format!("Preview · with your {overrides} {noun}"),
        (_, false) => format!("Preview · without your {overrides} {noun}"),
    }
}

/// Whether the list of `section` has a preview of the bar under it.
pub fn is_previewed(schema: &Schema, section: Section) -> bool {
    schema
        .preview_group
        .is_some_and(|group| section == Section::Group(group))
}

/// The display columns `text` takes in the pane. Zellij lays out a plugin's
/// pane with East Asian Ambiguous characters one column wide, so they are
/// counted as one here, whatever `ambiguous_width` says for the bar.
pub fn columns(text: &str) -> usize {
    text.chars()
        .map(|ch| crate::ansi::char_width(ch, false))
        .sum()
}

/// `text` cut to `width` columns, ending in `…` when it was cut. A wide
/// character that would cross the edge is left out whole.
pub fn truncate(text: &str, width: usize) -> String {
    if columns(text) <= width {
        return text.to_string();
    }
    let room = width.saturating_sub(1);
    let mut cut = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let width = crate::ansi::char_width(ch, false);
        if used + width > room {
            break;
        }
        cut.push(ch);
        used += width;
    }
    if width > 0 {
        cut.push('…');
    }
    cut
}
