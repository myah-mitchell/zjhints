//! The images in the README and the docs, drawn by the code that draws the
//! bar and the settings screen.
//!
//! The engine that turns terminal cells into SVG, and decides which files to
//! write, compare or remove, is `zjkit::images`. This module holds what is
//! zjhints' own: how to put the bar, the hover preview and the settings screen
//! on a screen, and the list of images the docs show. Each image is filled in
//! from the ANSI the bar prints, or from the lines `settings::view` lays out
//! for the settings screen.
//!
//! `make images` rewrites the SVGs in `docs/img` that this module draws, and
//! removes the ones it drew before but no longer does. Without
//! `ZJKIT_WRITE_IMAGES` set, the same test compares what it draws with the
//! files, so `cargo test` in CI fails when a change to the bar leaves an image
//! out of date.

mod session;

use crate::mouse;
use crate::settings::editor::{Editor, Field, Focus};
use crate::settings::view::{self, Control, FormatView, HelpState, Layout, Marker, RowView};
use crate::{ClickMap, State};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use zellij_tile::prelude::{InputMode, ModeInfo, Mouse, PluginAliasEntry, PluginEntry};
use zjkit::images::{mode_info, palette, rgb_of, Animation, Frame, Pen, Screen, MOUSE_POINTER};

/// The plugin the images are drawn for. The engine marks each SVG with it and
/// removes only the SVGs that carry it.
const PLUGIN: &str = "zjhints";

/// Where the settings screen in the images runs from, and its alias loads.
const LOCATION: &str = "file:~/.local/share/zellij/plugins/zjhints.wasm";

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
    state.bar.zellij_palette = Some(palette());
    state.apply_configuration(configuration_of(options));
    state.bar.pane_width = Some(columns);
    state.mode_info = ModeInfo {
        session_ascended: ascended.then_some(true),
        ..mode_info(mode)
    };
    state
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
    assert_eq!(
        (widest - narrowest) % step,
        0,
        "the steps land on the narrowest width"
    );
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
    /// Text in the error color, such as a key's warning that the bar
    /// ignores it.
    error: Pen,
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
            error: Pen::colored(rgb_of(styling.exit_code_error.base)),
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
        Marker::Ignored => pens.error,
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
            location: LOCATION.to_string(),
            cwd: None,
            configuration: configuration_of(options).into_iter().collect(),
        },
    }];
    let mut editor = Editor::new(
        &crate::settings::SCHEMA,
        LOCATION,
        alias.clone(),
        alias,
        None,
    )
    .expect("an alias loading zjhints");
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
    let preview_rows =
        view::is_previewed(editor.schema(), editor.section()).then_some(editor.rows().len());
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
        let overrides = view::theme_overrides(
            editor.schema(),
            editor.working(),
            Some(&mode_info.style.colors),
        );
        let caption = view::truncate(&view::preview_caption(overrides.len(), true), width);
        let dim = Pen {
            dim: true,
            ..pens.plain
        };
        screen.print_with(y, layout.content_x, &caption, |_| dim);
        if !overrides.is_empty() {
            let x = layout.content_x + view::columns(&caption) + 2;
            let button = view::overrides_button(true);
            print_buttons(
                &mut screen,
                y,
                x,
                layout.content_x + width,
                &[button],
                &pens,
            );
        }
        let bar = view::theme_preview(editor.schema(), editor.working(), &mode_info, true, width);
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

/// The hover preview popup of a hint, placed on a screen.
struct Hover {
    /// The columns of the hint or marker it previews.
    hint: std::ops::Range<usize>,
    /// The column of its left edge.
    left: usize,
    lines: Vec<String>,
    /// The list's row of the first entry, which it highlights.
    highlighted: Option<String>,
}

/// The hover preview of what is under `column` of the bar `map` was drawn
/// with, as the popup draws it on a screen `columns` wide, or `None` when
/// that spot shows none. The popup hangs from the hint it previews, shifted
/// left when that would put it off the screen.
fn hover_at(map: &ClickMap, column: usize, columns: usize) -> Option<Hover> {
    let preview = mouse::hover_preview(map, &Mouse::Hover(0, column))?;
    let labels = preview.labels();
    let (width, height) = zjkit::popup::size(&labels);
    Some(Hover {
        left: preview.columns.start.min(columns.saturating_sub(width)),
        hint: preview.columns,
        lines: menu_lines(&labels, height, width),
        highlighted: labels.first().map(|label| menu_row(label, width)),
    })
}

/// A hover popup of `labels` as it shows at `rows` by `cols`: the frame the
/// popup draws, and inside it the rows of Zellij's list widget, each a
/// space, the empty mark column and a space before its label.
fn menu_lines(labels: &[String], rows: usize, cols: usize) -> Vec<String> {
    let mut lines = zjkit::popup::frame_lines(rows, cols);
    for (row, label) in labels.iter().take(rows.saturating_sub(2)).enumerate() {
        lines[row + 1] = format!("│ {} │", menu_row(label, cols));
    }
    lines
}

/// The list widget's row of `label` in a hover popup `cols` wide, which
/// fills the popup inside its frame and padding.
fn menu_row(label: &str, cols: usize) -> String {
    format!("   {} ", fit(label, cols.saturating_sub(8)))
}

/// Every hover preview the bar `map` was drawn with opens, left to right,
/// on a screen `columns` wide.
fn hovers_of(map: &ClickMap, columns: usize) -> Vec<Hover> {
    let mut hovers: Vec<Hover> = Vec::new();
    for column in 0..columns {
        if let Some(hover) = hover_at(map, column, columns) {
            if hovers.last().is_none_or(|last| last.hint != hover.hint) {
                hovers.push(hover);
            }
        }
    }
    hovers
}

impl Hover {
    /// The middle column of the hint, where a pointer resting on it points.
    fn middle(&self) -> usize {
        (self.hint.start + self.hint.end) / 2
    }

    /// The popup with its bottom edge on the row above `bar_row`, as it opens
    /// over a bar at the bottom of the screen. The list highlights its first
    /// entry until the pointer moves onto another.
    fn draw_above(&self, screen: &mut Screen, bar_row: usize) {
        let top = bar_row - self.lines.len();
        for (offset, line) in self.lines.iter().enumerate() {
            screen.print(top + offset, self.left, line);
        }
        if let Some(row) = self.highlighted.as_ref().filter(|_| self.lines.len() > 2) {
            let selected = WidgetPens::new().selected;
            screen.print_with(top + 1, self.left + 2, row, |_| selected);
        }
    }
}

/// The mouse pointer with its tip at the top left of the cell at `row` and
/// `column`, so it touches the cell above from below.
fn draw_pointer(screen: &mut Screen, row: usize, column: usize) {
    let pointer = Pen::colored((0xff, 0xff, 0xff));
    screen.print_with(row, column, &MOUSE_POINTER.to_string(), |_| pointer);
}

/// The pointer moving along a bar at the bottom of the screen: swept across
/// the first hints it opens no popup, then it rests on a hint whose keys do
/// different things and on the drop indicator, and each one's popup opens
/// over the bar. It stands still on the drop indicator's popup.
fn hover_sweep() -> Animation {
    let options = [("show_mode", "true"), ("mouse", "true")];
    let (line, map) = bar(&options, InputMode::Pane, BAR_COLUMNS);
    let hovers = hovers_of(&map, BAR_COLUMNS);
    // The last hint before the drop indicator, so the sweep crosses others.
    let [.., hint, indicator] = hovers.as_slice() else {
        panic!("a hint and the drop indicator that open popups");
    };
    let bar_row = hovers
        .iter()
        .map(|hover| hover.lines.len())
        .max()
        .unwrap_or(0);
    let frame = |pointer: usize, hover: Option<&Hover>, milliseconds: u32| {
        let mut screen = Screen::new(bar_row + 2, BAR_COLUMNS);
        screen.print(bar_row, 0, &line);
        if let Some(hover) = hover {
            hover.draw_above(&mut screen, bar_row);
        }
        draw_pointer(&mut screen, bar_row + 1, pointer);
        Frame::new(screen, milliseconds)
    };
    // The pointer starts on the mode prefix, which shows nothing.
    let mut frames = vec![frame(2, None, 1200)];
    let sweep = hint.hint.start.saturating_sub(2) / 4;
    frames.extend((1..4).map(|step| frame(2 + sweep * step, None, 200)));
    for hover in [hint, indicator] {
        // The popup opens once the pointer has rested a moment.
        frames.push(frame(hover.middle(), None, 600));
        frames.push(frame(hover.middle(), Some(hover), 3000));
    }
    let still = frames.len() - 1;
    Animation::new(frames, still)
}

/// The settings screen on the Theme group as each theme option is picked,
/// over a bar started from the alias, which takes each change as it is made.
/// It stands still once `format_theme` is pills.
fn settings_live() -> Animation {
    let options = [("show_mode", "true")];
    let edits = [
        ("format_theme", "flat"),
        ("format_theme", "powerline"),
        ("format_theme", "pills"),
        ("color_theme", "dracula"),
        ("alias_theme", "unicode"),
    ];
    let (rows, columns) = (22, 100);
    let caption = Pen {
        dim: true,
        ..Pen::default()
    };
    let frames = (0..=edits.len())
        .map(|made| {
            let made = &edits[..made];
            let select = made.last().map_or("format_theme", |(key, _)| key);
            let mut screen = Screen::new(rows + 3, columns);
            screen.paste(
                0,
                0,
                &settings_screen(&options, select, made, rows, columns),
            );
            screen.print_with(rows + 1, 0, "A bar started from the alias", |_| caption);
            let running = [options.as_slice(), made].concat();
            screen.print(rows + 2, 0, &bar(&running, InputMode::Normal, columns).0);
            let last = made.len() == edits.len();
            Frame::new(screen, if last { 3000 } else { 1800 })
        })
        .collect();
    let pills = edits
        .iter()
        .position(|edit| *edit == ("format_theme", "pills"))
        .expect("an edit to pills");
    // Frame 0 has no edits, so the frame after an edit is one past it.
    Animation::new(frames, pills + 1)
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
            "session-modes.svg",
            session::modes().to_svg("A Zellij session with the zjtabs tab bar at the top, ending in the session's name, demo, and the zjhints bar at the bottom, both showing the mode as the session moves through them, and the zjhints bar the keys of each"),
        ),
        (
            "session-fitting.svg",
            session::fitting().to_svg("A Zellij session narrowing under the zjtabs tab bar, the session's name, demo, keeping to the tab bar's right end and the zjhints bar dropping whole hints to fit and offering the ones that did not fit from the drop indicator"),
        ),
        (
            "session-settings.svg",
            session::settings().to_svg("A Zellij session named demo, under the zjtabs tab bar that ends in that name, opening the zjhints settings screen and restyling the zjhints bar, each change previewed on it before saving"),
        ),
        (
            "session-mouse.svg",
            session::mouse().to_svg("A Zellij session named demo, under the zjtabs tab bar that ends in that name, with the pointer resting on zjhints hints, each one's menu opening over the bar and following the mode"),
        ),
        (
            "session-themes.svg",
            session::themes().to_svg("A Zellij session named demo, under the zjtabs tab bar that ends in that name, its zjhints bar drawn in one combination of color, format and alias themes after another"),
        ),
        (
            "session-nested.svg",
            session::nested().to_svg("A Zellij session named demo, its name at the end of the zjtabs tab bar, with a nested session in one pane, its zjhints bar showing the keys of whichever session has the keyboard, and its zjtabs tab bar fading while the nested session has it"),
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
            "settings-live.svg",
            settings_live().to_svg("The settings screen picking themes, with a bar under it restyled by each pick"),
        ),
        (
            "mouse-preview.svg",
            hover_sweep().to_svg("The pointer resting on hints of the bar, each one's hover preview opening over it"),
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

/// Where the images live: `docs/img` at the workspace root, two levels above
/// this crate.
fn image_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/img")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_the_images_twice_gives_the_same_bytes() {
        assert_eq!(images(), images());
    }

    #[test]
    fn the_images_in_the_docs_are_up_to_date() {
        zjkit::images::write_or_compare_images(PLUGIN, &image_directory(), &images());
    }
}
