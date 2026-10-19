//! What the settings screen shows and where: the layout for a pane size, and
//! each row as the widget that edits it, its marker and its swatch. Pure
//! functions, so the screen module only has to draw what these return, and
//! the images of the screen in the docs draw the same.

use super::editor::{Editor, Focus, Row, Section, OTHER_HEADING};
use super::options::{self, Kind, FAMILIES};
use crate::format;
use std::collections::BTreeMap;
use std::ops::Range;
use zellij_tile::prelude::{BareKey, KeyWithModifier, ModeInfo, Styling};

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
const MAX_LABEL_WIDTH: usize = 32;
/// The fewest and most columns a value's field takes.
const MIN_VALUE_WIDTH: usize = 10;
const MAX_VALUE_WIDTH: usize = 40;
/// The fewest columns a field for typed text takes when there is room.
const TEXT_FIELD_WIDTH: usize = 24;
/// Columns a color swatch takes before the marker: the swatch and a gap.
pub const SWATCH_WIDTH: usize = 4;
/// The group whose list a preview of the bar follows.
pub const PREVIEW_GROUP: &str = "Theme";
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
    fn plain(label: impl Into<String>, control: Control) -> Self {
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
            layered: crate::theme::layer(editor.working(), zellij_palette),
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
                    .map(|name| crate::show_controls(name).into_owned())
                    .collect();
                return RowView::plain("alias", Control::Dropdown(names, chosen));
            }
            Row::Add(prefix) => {
                return RowView::plain("", Control::Button(format!("+ add {}", add_what(prefix))))
            }
            Row::Heading(title) => return RowView::plain("", Control::Heading(title.to_string())),
            Row::Option(option) => (option.name, option.kind, option.default),
            Row::Key(key) => (key.as_str(), options::kind_of(key), None),
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
            label: crate::show_controls(key).into_owned(),
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
                    .then(|| shown.and_then(|value| render_sample(key, value, config)))
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

/// A value as shown in its field. Text is always quoted, so its spaces show
/// and it reads apart from a number or a choice; anything else is quoted
/// only when it would otherwise be invisible or ambiguous.
pub fn show_value(value: &str, kind: Kind) -> String {
    // Escaped, so an ESC in a value draws as text instead of moving the
    // cursor or rewriting other rows.
    let shown = crate::show_controls(value);
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
    // line apart; the raw text shows it escaped instead.
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
    format!("\u{1b}[38;2;{red};{green};{blue}m██\u{1b}[0m")
}

/// The first `count` visible characters of an ANSI string, styles kept.
pub fn take_visible(ansi: &str, count: usize) -> String {
    crate::truncate_ansi_string(ansi, "", count, false)
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
        Row::Key(key) => match options::family_of(&key) {
            Some(family) => format!(
                "{}: {} ({}). Del removes it.",
                crate::show_controls(&key),
                family.description,
                family.pattern
            ),
            None => format!(
                "{}: not an option zjhints reads. Del removes it.",
                crate::show_controls(&key)
            ),
        },
        Row::Add("") => format!(
            "Add a key no other group lists. It is listed under {OTHER_HEADING} unless a group fits it."
        ),
        Row::Add(prefix) => {
            let family = FAMILIES.iter().find(|family| family.prefix() == prefix);
            let what = family.map_or("", |family| family.description);
            format!("Add a key starting {prefix}. {what}.")
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

/// The help line: `prefix`, then as many of `hints` as fit in `cols`, each
/// as "key - action". The ranges are the characters of the keys, which are
/// drawn in a color of their own.
pub fn hint_line(prefix: &str, hints: &[(&str, &str)], cols: usize) -> (String, Vec<Range<usize>>) {
    let mut line = prefix.to_string();
    let mut key_ranges = Vec::new();
    for (index, (key, action)) in hints.iter().enumerate() {
        let separator = if index == 0 { "" } else { ", " };
        let entry = format!("{separator}{key} - {action}");
        if columns(&line) + columns(&entry) > cols {
            break;
        }
        let key_start = line.chars().count() + separator.chars().count();
        key_ranges.push(key_start..key_start + key.chars().count());
        line.push_str(&entry);
    }
    (truncate(&line, cols), key_ranges)
}

/// The keys of `configuration` that take the place of a value the selected
/// themes set, in key order.
pub fn theme_overrides(
    configuration: &BTreeMap<String, String>,
    zellij_palette: Option<&Styling>,
) -> Vec<String> {
    let is_theme = |key: &str| crate::theme::OPTIONS.contains(&key);
    let themes: BTreeMap<String, String> = configuration
        .iter()
        .filter(|(key, _)| is_theme(key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let themed = crate::theme::layer(&themes, zellij_palette);
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
    configuration: &BTreeMap<String, String>,
    mode_info: &ModeInfo,
    overrides_shown: bool,
    width: usize,
) -> String {
    let palette = mode_info.style.colors;
    let mut shown = configuration.clone();
    if !overrides_shown {
        for key in theme_overrides(configuration, Some(&palette)) {
            shown.remove(&key);
        }
    }
    // Options that hide the whole bar would leave nothing to preview.
    for key in ["hidden", "hide_in_base_mode", "hide_when_nested"] {
        shown.remove(key);
    }
    let mut state = crate::State {
        zellij_palette: Some(palette),
        pane_width: Some(width),
        ..Default::default()
    };
    state.apply_configuration(shown);
    state.mode_info = mode_info.clone();
    state.draw_bar().0
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
pub fn is_previewed(section: Section) -> bool {
    section == Section::Group(PREVIEW_GROUP)
}

/// The display columns `text` takes in the pane. Zellij lays out a plugin's
/// pane with East Asian Ambiguous characters one column wide, so they are
/// counted as one here, whatever `ambiguous_width` says for the bar.
pub fn columns(text: &str) -> usize {
    text.chars().map(|ch| crate::char_width(ch, false)).sum()
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
        let width = crate::char_width(ch, false);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::editor::Field;
    use zellij_tile::prelude::actions::Action;
    use zellij_tile::prelude::{InputMode, PaletteColor, PluginAliasEntry, PluginEntry};

    fn alias(configuration: &[(&str, &str)]) -> Vec<PluginAliasEntry> {
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
        }]
    }

    fn editor(configuration: &[(&str, &str)]) -> Editor {
        Editor::new(alias(configuration), alias(configuration), None).unwrap()
    }

    /// `row` as drawn on its own, laying the theme for it alone.
    fn row_view(editor: &Editor, row: &Row, view: FormatView) -> RowView {
        Rows::new(editor, view, None).view(row)
    }

    fn option_row(name: &str) -> Row {
        Row::Option(options::find(name).unwrap())
    }

    fn key_row(key: &str) -> Row {
        Row::Key(key.to_string())
    }

    /// The text of the field of `row`, formats shown raw.
    fn raw(editor: &Editor, row: &Row) -> String {
        match row_view(editor, row, FormatView::Raw).control {
            Control::Text { text, .. } => text,
            other => panic!("expected a text field, got {other:?}"),
        }
    }

    fn text(text: &str, placeholder: bool) -> Control {
        Control::Text {
            text: text.to_string(),
            placeholder,
        }
    }

    #[test]
    fn each_kind_of_option_gets_its_own_widget() {
        let editor = editor(&[("show_mode", "true"), ("direction_keys", "arrows")]);
        let view = |name| row_view(&editor, &option_row(name), FormatView::Raw);
        assert_eq!(view("show_mode").control, Control::Toggle(true));
        assert_eq!(view("auto_width").control, Control::Toggle(true));
        assert_eq!(
            view("direction_keys").control,
            Control::Dropdown(
                vec![
                    "both".to_string(),
                    "arrows".to_string(),
                    "letters".to_string()
                ],
                1
            )
        );
        assert_eq!(view("max_length").control, text("0", true));
        assert_eq!(view("max_length").label, "max_length");
    }

    #[test]
    fn markers_say_whether_a_value_is_unsaved_default_or_set() {
        let mut editor = editor(&[("max_length", "40")]);
        let marker =
            |editor: &Editor, name| row_view(editor, &option_row(name), FormatView::Raw).marker;
        assert_eq!(marker(&editor, "max_length"), Marker::None);
        assert_eq!(marker(&editor, "auto_width"), Marker::Default);
        editor.submit(
            &Field::Value {
                key: "max_length".to_string(),
            },
            "60",
        );
        assert_eq!(marker(&editor, "max_length"), Marker::Unsaved);
        // Unsetting a saved value is unsaved too.
        editor.submit(
            &Field::Value {
                key: "max_length".to_string(),
            },
            "",
        );
        assert_eq!(marker(&editor, "max_length"), Marker::Unsaved);
        assert_eq!(Marker::Unsaved.text(), "● unsaved");
        assert_eq!(Marker::Default.text(), "○ default");
    }

    #[test]
    fn an_unset_field_shows_its_default_as_a_placeholder() {
        let editor = editor(&[("max_length", "40")]);
        assert_eq!(
            row_view(&editor, &option_row("overflow_str"), FormatView::Raw).control,
            text("\"...\"", true)
        );
        assert_eq!(
            row_view(&editor, &option_row("max_length"), FormatView::Raw).control,
            text("40", false)
        );
    }

    #[test]
    fn text_is_always_quoted_and_nothing_else_is() {
        let editor = editor(&[
            ("pipe_name", "bar"),
            ("label_quit", "bye"),
            ("dim_strength", "0.25"),
            ("key_format", "{key}"),
        ]);
        assert_eq!(raw(&editor, &option_row("pipe_name")), "\"bar\"");
        assert_eq!(raw(&editor, &key_row("label_quit")), "\"bye\"");
        assert_eq!(raw(&editor, &option_row("key_format")), "\"{key}\"");
        assert_eq!(raw(&editor, &option_row("dim_strength")), "0.25");
        assert_eq!(show_value("", Kind::Integer), "\"\"");
        assert_eq!(show_value(" 1", Kind::Integer), "\" 1\"");
    }

    #[test]
    fn control_characters_in_keys_and_values_show_as_escapes() {
        let osc = "\u{1b}]52;c;aGk=\u{7}";
        let editor = editor(&[
            ("label_quit", osc),
            ("key_format", osc),
            ("max_length", "\u{1b}[2J"),
            ("x\u{1b}[1A", "1"),
        ]);
        let shown = "\"\\u{1b}]52;c;aGk=\\u{7}\"";
        for view in [FormatView::Raw, FormatView::Rendered] {
            let row = |row: &Row| row_view(&editor, row, view);
            assert_eq!(row(&key_row("label_quit")).control, text(shown, false));
            let format = row(&option_row("key_format"));
            assert_eq!(format.control, text(shown, false));
            assert_eq!(format.styled, None);
            assert_eq!(
                row(&option_row("max_length")).control,
                text("\\u{1b}[2J", false)
            );
            assert_eq!(row(&key_row("x\u{1b}[1A")).label, "x\\u{1b}[1A");
        }
    }

    #[test]
    fn add_rows_are_buttons_naming_the_families_they_add() {
        let editor = editor(&[]);
        let button = |prefix| row_view(&editor, &Row::Add(prefix), FormatView::Raw).control;
        assert_eq!(
            button("color_"),
            Control::Button("+ add color_<name>".to_string())
        );
        assert_eq!(
            button("label_"),
            Control::Button("+ add label_<id> or label_<mode>_<id>".to_string())
        );
        assert_eq!(button(""), Control::Button("+ add a key".to_string()));
        assert_eq!(
            row_view(&editor, &Row::Heading("Colors"), FormatView::Raw).control,
            Control::Heading("Colors".to_string())
        );
    }

    #[test]
    fn the_alias_row_is_a_dropdown_of_the_aliases() {
        let editor = editor(&[]);
        assert_eq!(
            row_view(&editor, &Row::Alias, FormatView::Raw).control,
            Control::Dropdown(vec!["zjhints".to_string()], 0)
        );
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
        let rendered = |row: &Row| row_view(&editor, row, FormatView::Rendered);

        let view = rendered(&option_row("key_format"));
        assert_eq!(view.control, text(" Ctrl p ", false));
        let ansi = view.styled.unwrap();
        assert!(ansi.contains("38;2;255;0;0"));
        assert!(ansi.contains(" Ctrl p "));

        let shown = |row: &Row| match rendered(row).control {
            Control::Text { text, .. } => text,
            other => panic!("{other:?}"),
        };
        assert_eq!(shown(&option_row("desc_format")), "[Pane]");
        assert_eq!(shown(&option_row("mode_format")), "<NORMAL>");
        assert_eq!(shown(&key_row("mode_format_enter_search")), "ENTERSEARCH");
        assert_eq!(shown(&option_row("hint_spacer")), "|");
        // Labels are text, not format strings, whatever they hold.
        assert_eq!(shown(&key_row("label_quit")), "\"{key}\"");

        let view = row_view(&editor, &option_row("key_format"), FormatView::Raw);
        assert_eq!(view.control, text("\"#[fg=#ff0000,bold] {key} \"", false));
        assert_eq!(view.styled, None);
    }

    #[test]
    fn a_rendered_key_uses_the_modifier_aliases_being_edited() {
        let editor = editor(&[("mod_alias_ctrl", "^"), ("key_format", "{key}")]);
        let view = row_view(&editor, &option_row("key_format"), FormatView::Rendered);
        assert_eq!(view.control, text("^p", false));
    }

    #[test]
    fn a_format_that_draws_nothing_visible_shows_raw() {
        let editor = editor(&[("hint_spacer", "  "), ("desc_format", "#[fg=red]")]);
        let view = |name| row_view(&editor, &option_row(name), FormatView::Rendered);
        assert_eq!(view("hint_spacer").control, text("\"  \"", false));
        assert_eq!(view("desc_format").control, text("\"#[fg=red]\"", false));
        assert_eq!(view("desc_format").styled, None);
    }

    #[test]
    fn a_rendered_value_is_cut_by_visible_characters() {
        let ansi = "\u{1b}[31mabcdefghij\u{1b}[0m";
        let cut = take_visible(ansi, 5);
        assert!(cut.contains("abcde") && !cut.contains('f'));
        assert!(cut.ends_with("\u{1b}[0m"));
    }

    #[test]
    fn wide_characters_are_cut_by_their_columns() {
        assert_eq!(truncate("漢字漢字", 8), "漢字漢字");
        assert_eq!(truncate("漢字漢字", 7), "漢字漢…");
        assert_eq!(truncate("漢字漢字", 6), "漢字…");
        assert_eq!(truncate("abc", 0), "");
        assert_eq!(columns("漢字"), 4);
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
            "\u{1b}[38;2;1;2;3m██\u{1b}[0m".to_string()
        );

        let editor = editor(&[("color_blue", "#0000ff"), ("color_accent", "$blue")]);
        let swatch = |key| row_view(&editor, &key_row(key), FormatView::Raw).swatch;
        assert_eq!(swatch("color_blue"), Some((0, 0, 255)));
        assert_eq!(swatch("color_accent"), Some((0, 0, 255)));
    }

    #[test]
    fn values_are_drawn_against_the_selected_theme() {
        let editor = editor(&[
            ("color_theme", "tokyo-night"),
            ("key_format_quit", "#[fg=$bg,bg=$red,bold] {key} "),
            ("color_accent", "$red"),
        ]);
        let themed = crate::theme::layer(editor.working(), None);
        let red = swatch_color(&themed["color_red"], &themed).expect("a hex red");

        // A `$red` value the theme defines gets its swatch.
        let view = row_view(&editor, &key_row("color_accent"), FormatView::Raw);
        assert_eq!(view.swatch, Some(red));

        // A format using the theme's aliases renders in its colors.
        let view = row_view(&editor, &key_row("key_format_quit"), FormatView::Rendered);
        let (red_r, red_g, red_b) = red;
        let ansi = view.styled.expect("a rendered format");
        assert!(ansi.contains(&format!("48;2;{red_r};{red_g};{red_b}")));

        // What is saved stays the user's own.
        assert!(!editor.working().contains_key("color_red"));
    }

    #[test]
    fn values_follow_the_zellij_theme_when_no_color_theme_is_chosen() {
        let editor = editor(&[("key_format", "#[bg=$accent] {key} ")]);
        let mut styling = Styling::default();
        styling.ribbon_selected.background = PaletteColor::Rgb((1, 2, 3));
        let row = option_row("key_format");
        let view = Rows::new(&editor, FormatView::Rendered, Some(&styling)).view(&row);
        let ansi = view.styled.expect("a rendered format");
        assert!(ansi.contains("48;2;1;2;3"), "{ansi:?}");
        // Without the palette there is no `$accent` to draw.
        let view = row_view(&editor, &row, FormatView::Rendered);
        let ansi = view.styled.expect("a rendered format");
        assert!(!ansi.contains("48;2;1;2;3"));
    }

    #[test]
    fn an_unset_option_a_theme_sets_shows_the_themes_value() {
        let editor = editor(&[("color_theme", "nord"), ("format_theme", "flat")]);
        let themed = crate::theme::layer(editor.working(), None);
        let view = row_view(&editor, &option_row("key_format"), FormatView::Raw);
        let shown = show_value(&themed["key_format"], Kind::Format);
        assert_eq!(view.control, text(&shown, true));
        assert_eq!(view.marker, Marker::Theme);

        let view = row_view(&editor, &option_row("key_format"), FormatView::Rendered);
        assert_eq!(view.control, text(" Ctrl p ", true));
        assert!(view.styled.is_some());

        // Without a theme the built-in default shows, as before.
        let view = row_view(&editor, &option_row("max_length"), FormatView::Raw);
        assert_eq!(view.marker, Marker::Default);
    }

    #[test]
    fn fields_are_as_wide_as_the_longest_label_and_value_need() {
        let editor = editor(&[]);
        let rows = Rows::new(&editor, FormatView::Raw, None);
        let views: Vec<RowView> = editor
            .rows_of(Section::Group("Fitting"))
            .iter()
            .map(|row| rows.view(row))
            .collect();
        // "hint_precedence" is the longest label in Fitting.
        let (label, value) = field_widths(&views, 80);
        assert_eq!(label, "hint_precedence".len() + 2);
        assert_eq!(value, TEXT_FIELD_WIDTH);

        // A narrow list leaves room for the markers, down to a minimum.
        let (label_narrow, value_narrow) = field_widths(&views, 40);
        assert_eq!(label_narrow, label);
        assert_eq!(value_narrow, 40 - label - "○ default".chars().count() - 2);
        assert_eq!(field_widths(&views, 10).1, MIN_VALUE_WIDTH);

        // A long key is cut at the label limit.
        let long = RowView::plain("x".repeat(60), text("", false));
        assert_eq!(field_widths(&[long], 120).0, MAX_LABEL_WIDTH);
    }

    #[test]
    fn the_swatch_comes_before_the_marker_which_stays_inside_the_list() {
        let mut view = RowView::plain("color_blue", text("\"#0000ff\"", false));
        view.marker = Marker::Unsaved;
        assert_eq!(trailer_columns(&view, 30, 80), (32, 32));
        view.swatch = Some((0, 0, 255));
        assert_eq!(trailer_columns(&view, 30, 80), (32, 36));
        // Too little room pulls the marker back over the gap.
        assert_eq!(trailer_columns(&view, 30, 40), (32, 31));
    }

    #[test]
    fn the_screen_is_centered_with_the_menu_beside_the_rows() {
        let items: Vec<String> = editor(&[])
            .sections()
            .iter()
            .map(|s| s.title().to_string())
            .collect();
        // "Keys and ordering" is the longest item.
        let layout = Layout::new(30, 200, &items, None).unwrap();
        assert_eq!(layout.menu_width, 17 + 4);
        assert_eq!(layout.width, 21 + 2 + 80);
        assert_eq!(layout.x, (200 - 103) / 2);
        assert_eq!(layout.content_x, layout.x + 23);
        assert_eq!(layout.content_width, 80);
        assert_eq!((layout.body_y, layout.body_height), (4, 22));
        assert_eq!(layout.footer_y, 26);

        let layout = Layout::new(30, 60, &items, None).unwrap();
        assert_eq!((layout.x, layout.menu_width), (0, 20));
        assert_eq!(layout.content_width, 38);

        assert_eq!(Layout::new(MIN_ROWS - 1, 80, &items, None), None);
        assert_eq!(Layout::new(30, MIN_COLS - 1, &items, None), None);
    }

    #[test]
    fn a_previewed_list_has_the_preview_right_under_its_rows() {
        let items = vec!["Theme".to_string()];
        let layout = Layout::new(30, 120, &items, Some(3)).unwrap();
        assert_eq!(layout.list_height, 3);
        assert_eq!(layout.preview_y, Some(layout.body_y + 4));

        // A long list leaves room for the preview at the bottom.
        let layout = Layout::new(30, 120, &items, Some(100)).unwrap();
        assert_eq!(
            layout.list_height,
            30 - HEADER_ROWS - FOOTER_ROWS - PREVIEW_ROWS
        );
    }

    #[test]
    fn the_menu_marks_sections_with_unsaved_edits() {
        let mut editor = editor(&[]);
        editor.submit(
            &Field::Value {
                key: "color_blue".to_string(),
            },
            "#0000ff",
        );
        let items = menu_items(&editor);
        assert!(items.contains(&"Colors ●".to_string()));
        assert!(items.contains(&"Fitting".to_string()));
    }

    #[test]
    fn the_header_counts_the_unsaved_changes_and_names_the_buttons() {
        assert_eq!(unsaved_text(0), "0 unsaved changes");
        assert_eq!(unsaved_text(1), "1 unsaved change");
        let buttons = header_buttons(FormatView::Rendered);
        assert_eq!(buttons[0], ("<Ctrl a>", "save", Command::Save));
        assert_eq!(buttons[2].1, "raw formats");
        assert_eq!(header_buttons(FormatView::Raw)[2].1, "rendered formats");
        assert_eq!(overrides_button(true).1, "hide overrides");
    }

    #[test]
    fn the_help_line_lists_the_keys_for_what_has_focus() {
        let browsing = |focus, text_row, searching| {
            help_hints(HelpState::Browsing {
                focus,
                text_row,
                searching,
            })
        };
        let list = browsing(Focus::List, false, false);
        assert!(list.contains(&("<Space>", "change")));
        assert!(list.contains(&("<r>", "revert")));
        assert!(list.contains(&("<Esc>", "close")));
        assert!(browsing(Focus::List, true, false).contains(&("<Enter>", "edit")));
        assert!(browsing(Focus::List, false, true).contains(&("<Esc>", "end search")));
        assert!(browsing(Focus::Sidebar, false, false).contains(&("<Tab>", "settings")));
        assert_eq!(
            browsing(Focus::Search, false, true),
            [("<↓>", "results"), ("<Esc>", "end search")]
        );
        assert_eq!(help_hints(HelpState::Editing)[0], ("<Enter>", "apply"));

        let (line, keys) = hint_line("Help: ", &[("<a>", "one"), ("<b>", "two")], 80);
        assert_eq!(line, "Help: <a> - one, <b> - two");
        assert_eq!(keys, [6..9, 17..20]);
        // Hints that do not fit are left out whole.
        let (line, keys) = hint_line("Help: ", &[("<a>", "one"), ("<b>", "two")], 20);
        assert_eq!(line, "Help: <a> - one");
        assert_eq!(keys.len(), 1);
    }

    #[test]
    fn the_description_explains_the_selected_row() {
        let mut editor = editor(&[("color_blue", "#0000ff")]);
        editor.submit(&Field::NewKey, "key_order");
        let text = describe(&editor).unwrap();
        assert!(text.starts_with("How keys within a hint are ordered"));
        assert!(text.ends_with("(default: qwerty)"));
        editor.submit(&Field::NewKey, "color_blue");
        assert!(describe(&editor).unwrap().contains("color_<name>"));
        editor.focus = Focus::Sidebar;
        assert_eq!(describe(&editor), None);
        assert_eq!(match_text(1), "1 match");
        assert_eq!(match_text(0), "0 matches");
    }

    fn mode_info(mode: InputMode) -> ModeInfo {
        ModeInfo {
            mode,
            base_mode: Some(InputMode::Normal),
            keybinds: vec![(
                InputMode::Normal,
                vec![(KeyWithModifier::new(BareKey::Char('q')), vec![Action::Quit])],
            )],
            ..Default::default()
        }
    }

    fn config_of(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn overrides_are_the_keys_a_theme_sets_that_the_user_sets_too() {
        let configuration = config_of(&[
            ("color_theme", "nord"),
            ("format_theme", "flat"),
            ("color_accent", "#ebcb8b"),
            ("desc_format", "{desc}"),
            ("color_mine", "#123456"),
            ("max_length", "80"),
        ]);
        assert_eq!(
            theme_overrides(&configuration, None),
            vec!["color_accent", "desc_format"]
        );

        // With every theme left at zellij and no palette, nothing is set to
        // override.
        let plain = config_of(&[("key_format", "{key}"), ("color_accent", "#ff0000")]);
        assert!(theme_overrides(&plain, None).is_empty());
    }

    #[test]
    fn the_preview_draws_the_bar_with_or_without_the_overrides() {
        let configuration = config_of(&[
            ("format_theme", "plain"),
            ("key_format", "<{key}>"),
            ("hidden", "true"),
        ]);
        let mode_info = mode_info(InputMode::Normal);
        let with = theme_preview(&configuration, &mode_info, true, 80);
        let without = theme_preview(&configuration, &mode_info, false, 80);
        // Drawn although `hidden` is set.
        assert!(with.contains("<q>"), "{with:?}");
        assert!(!without.contains("<q>"), "{without:?}");
        assert!(without.contains('q'), "{without:?}");
    }

    #[test]
    fn the_caption_says_whether_the_overrides_are_drawn() {
        assert_eq!(preview_caption(0, true), "Preview · no overrides");
        assert_eq!(preview_caption(1, true), "Preview · with your 1 override");
        assert_eq!(
            preview_caption(3, false),
            "Preview · without your 3 overrides"
        );
        assert!(is_previewed(Section::Group("Theme")));
        assert!(!is_previewed(Section::Group("Colors")));
    }
}
