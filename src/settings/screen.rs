//! The settings screen as a running plugin: reading the config, turning keys
//! and clicks into edits, previewing them on the bars, saving, and drawing.

use super::editor::{Editor, Field, Focus, Outcome, Section};
use super::live;
use super::view::{self, FormatView, Layout, Line, Place, FOOTER_LINES, NAME_COLUMNS};
use std::collections::BTreeMap;
use zellij_tile::prelude::*;
use zellij_utils::input::config_blocks::plugin_aliases_kdl;

/// A question waiting for an answer before anything else happens.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Question {
    /// Closing with unsaved edits.
    DiscardAndClose,
    /// The file changed on disk since it was read, so saving would lose that.
    OverwriteOrReload,
}

/// What the host is to report back on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Waiting {
    Save,
    Reload,
}

#[derive(Default)]
pub struct SettingsScreen {
    /// The alias the `alias` option pins, if any.
    pinned_alias: Option<String>,
    /// What is being edited, or why nothing can be.
    editor: Option<Result<Editor, String>>,
    /// The value being typed, and what it is for.
    input: Option<(Field, TextInput)>,
    question: Option<Question>,
    /// The last thing worth telling the user, and whether it is an error.
    status: Option<(String, bool)>,
    /// The alias's configuration as the bars were last told it.
    previewed: BTreeMap<String, String>,
    /// A save or reload was asked for and its result has not come back yet.
    waiting: Option<Waiting>,
    /// The session's aliases hold edits the file does not, because a save
    /// put them there and did not reach the file.
    runtime_ahead: bool,
    /// How format strings are shown in the list.
    view: FormatView,
    /// Where everything went at the last render, for the clicks after it.
    layout: Option<Layout>,
    /// The list, and the sidebar.
    scroll: Option<ScrollView>,
    sidebar_scroll: Option<ScrollView>,
    /// The section and row the list was last scrolled to show, so the view
    /// follows the selection when it moves and the scroll wheel otherwise.
    shown_selected: Option<(Section, usize)>,
    /// The section the sidebar was last scrolled to show.
    shown_section: Option<Section>,
    /// What the pointer was last over, so the scroll wheel scrolls that.
    pointer: Option<Place>,
}

impl SettingsScreen {
    pub fn load(&mut self, configuration: &BTreeMap<String, String>) {
        self.pinned_alias = configuration.get("alias").cloned();
        // The screen runs from the same location as the bars, so Zellij
        // caches one grant for both, and a grant replaces the cached one
        // rather than adding to it. Asking for everything a bar may use as
        // well keeps opening the screen from taking the bars' permissions
        // away. `ReadApplicationState` is also what the failed-write event
        // needs.
        let mut permissions = vec![
            PermissionType::Reconfigure,
            PermissionType::ReadApplicationState,
            PermissionType::MessageAndLaunchOtherPlugins,
        ];
        permissions.extend(crate::MOUSE_PERMISSIONS);
        request_permission(&permissions);
        subscribe(&[
            EventType::Key,
            EventType::Mouse,
            EventType::PermissionRequestResult,
            EventType::ConfigWasWrittenToDisk,
            EventType::FailedToWriteConfigToDisk,
            EventType::ConfigFileChangedSinceRead,
        ]);
    }

    pub fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(PermissionStatus::Granted) => {
                self.read();
                true
            }
            Event::PermissionRequestResult(PermissionStatus::Denied) => {
                self.editor = Some(Err(
                    "The settings screen needs permission to change the configuration.".to_string(),
                ));
                true
            }
            Event::Key(key) => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            Event::ConfigWasWrittenToDisk if self.waiting.is_some() => {
                let done = match self.waiting.take() {
                    Some(Waiting::Reload) => "Reloaded the config file",
                    _ => "Saved",
                };
                self.runtime_ahead = false;
                self.reread();
                if let Some(Ok(editor)) = &self.editor {
                    self.previewed = editor.working().clone();
                }
                self.status = Some((done.to_string(), false));
                true
            }
            Event::FailedToWriteConfigToDisk(path) if self.waiting.is_some() => {
                let verb = match self.waiting.take() {
                    Some(Waiting::Reload) => "read",
                    _ => "write",
                };
                let path = path.unwrap_or_else(|| "the config file".to_string());
                self.status = Some((format!("Could not {verb} {path}"), true));
                true
            }
            Event::ConfigFileChangedSinceRead if self.waiting == Some(Waiting::Save) => {
                self.waiting = None;
                self.question = Some(Question::OverwriteOrReload);
                true
            }
            Event::ConfigFileChangedSinceRead => {
                let dirty = matches!(&self.editor, Some(Ok(editor)) if editor.is_dirty());
                if !dirty {
                    self.reread();
                }
                true
            }
            _ => false,
        }
    }

    /// Read the aliases fresh and start editing.
    fn read(&mut self) {
        let aliases = read_config().blocks.plugin_aliases;
        let editor = Editor::new(aliases, self.pinned_alias.as_deref());
        if let Ok(editor) = &editor {
            self.previewed = editor.working().clone();
        }
        self.editor = Some(editor);
    }

    /// Read the aliases again after the config changed under the editor.
    fn reread(&mut self) {
        let aliases = read_config().blocks.plugin_aliases;
        match &mut self.editor {
            Some(Ok(editor)) => {
                if let Err(message) = editor.reread(aliases) {
                    self.editor = Some(Err(message));
                }
            }
            _ => self.read(),
        }
    }

    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        if let Some(question) = self.question {
            return self.answer(question, &key);
        }
        if self.input.is_some() {
            return self.type_key(&key);
        }
        let Some(Ok(editor)) = &mut self.editor else {
            if is_plain(&key, BareKey::Esc) || is_plain(&key, BareKey::Char('q')) {
                close_self();
            }
            return false;
        };
        let collapsed = self.layout.is_some_and(|layout| layout.is_collapsed());
        let page = match editor.focus {
            Focus::Sidebar => &self.sidebar_scroll,
            Focus::List => &self.scroll,
        }
        .as_ref()
        .map_or(10, |scroll| scroll.viewport_height().max(1)) as isize;
        // Ctrl s as well as s: Zellij's default keybindings take Ctrl s
        // for scroll mode, but it reaches the screen in locked mode.
        let is_save = key.bare_key == BareKey::Char('s')
            && (key.key_modifiers.is_empty() || key.has_modifiers(&[KeyModifier::Ctrl]));
        if is_save {
            self.save();
            return true;
        }
        let outcome = match key.bare_key {
            _ if !key.key_modifiers.is_empty() => return false,
            BareKey::Up | BareKey::Char('k') => editor.move_selection(-1),
            BareKey::Down | BareKey::Char('j') => editor.move_selection(1),
            BareKey::PageUp => editor.move_selection(-page),
            BareKey::PageDown => editor.move_selection(page),
            BareKey::Home | BareKey::Char('g') => editor.move_selection(isize::MIN / 2),
            BareKey::End | BareKey::Char('G') => editor.move_selection(isize::MAX / 2),
            // Without a sidebar, the keys that would move to it step
            // through the sections instead.
            BareKey::Left | BareKey::Char('h') if collapsed => editor.cycle_section(-1),
            BareKey::Right | BareKey::Char('l') | BareKey::Tab if collapsed => {
                editor.cycle_section(1)
            }
            BareKey::Left | BareKey::Char('h') => editor.set_focus(Focus::Sidebar),
            BareKey::Right | BareKey::Char('l') => editor.set_focus(Focus::List),
            BareKey::Tab => editor.toggle_focus(),
            BareKey::Enter | BareKey::Char(' ') => editor.activate(),
            BareKey::Backspace | BareKey::Delete | BareKey::Char('d') => editor.reset_selected(),
            BareKey::Char('r') => {
                self.view = self.view.toggled();
                return true;
            }
            BareKey::Esc | BareKey::Char('q') => {
                if editor.is_dirty() {
                    self.question = Some(Question::DiscardAndClose);
                } else {
                    close_self();
                }
                return true;
            }
            _ => return false,
        };
        self.carry_out(outcome)
    }

    fn handle_mouse(&mut self, mouse: Mouse) -> bool {
        if self.question.is_some() || self.input.is_some() {
            return false;
        }
        let (Some(Ok(editor)), Some(layout), Some(scroll), Some(sidebar_scroll)) = (
            &mut self.editor,
            self.layout,
            &mut self.scroll,
            &mut self.sidebar_scroll,
        ) else {
            return false;
        };
        let place = |line: isize, column: usize| {
            usize::try_from(line)
                .ok()
                .and_then(|line| layout.place(line, column))
        };
        match mouse {
            Mouse::Hover(line, column) => {
                self.pointer = place(line, column);
                false
            }
            // The wheel scrolls whichever side the pointer is over, without
            // moving the selection.
            Mouse::ScrollUp(lines) | Mouse::ScrollDown(lines) => {
                let lines = lines as isize;
                let delta = if matches!(mouse, Mouse::ScrollUp(_)) {
                    -lines
                } else {
                    lines
                };
                match self.pointer {
                    Some(Place::Sidebar) => sidebar_scroll.scroll_by(delta),
                    _ => scroll.scroll_by(delta),
                }
            }
            Mouse::LeftClick(line, column) => {
                self.pointer = place(line, column);
                let outcome = match self.pointer {
                    Some(Place::Sidebar) => {
                        let Some(index) = sidebar_scroll.row_at(line) else {
                            return false;
                        };
                        let shown = editor.select_section(index);
                        let focused = editor.set_focus(Focus::Sidebar);
                        if shown == Outcome::None {
                            focused
                        } else {
                            shown
                        }
                    }
                    Some(Place::List) => {
                        let Some(row) = scroll.row_at(line) else {
                            return false;
                        };
                        // A click selects; a click on the selected row acts
                        // on it.
                        if row == editor.selected && editor.focus == Focus::List {
                            editor.activate()
                        } else {
                            editor.select(row)
                        }
                    }
                    Some(Place::Previous) => editor.cycle_section(-1),
                    Some(Place::Next) => editor.cycle_section(1),
                    None => return false,
                };
                self.carry_out(outcome)
            }
            _ => false,
        }
    }

    fn carry_out(&mut self, outcome: Outcome) -> bool {
        match outcome {
            Outcome::None => false,
            Outcome::Redraw => {
                self.status = None;
                true
            }
            Outcome::StartField { field, text } => {
                let mut input = TextInput::new(text).focused();
                input.move_to_end();
                self.input = Some((field, input));
                self.status = None;
                true
            }
            Outcome::Changed => {
                self.status = None;
                self.preview();
                true
            }
            Outcome::SwitchedAlias => {
                if let Some(Ok(editor)) = &self.editor {
                    self.previewed = editor.working().clone();
                }
                self.status = None;
                true
            }
            Outcome::Refused(message) => {
                self.status = Some((message, true));
                true
            }
        }
    }

    fn type_key(&mut self, key: &KeyWithModifier) -> bool {
        let Some((_, input)) = &mut self.input else {
            return false;
        };
        match input.handle_key(key) {
            UiResponse::Cancelled => {
                self.input = None;
                true
            }
            UiResponse::Submitted(_) => {
                let (field, input) = self.input.take().expect("checked above");
                let Some(Ok(editor)) = &mut self.editor else {
                    return true;
                };
                let outcome = editor.submit(&field, input.get_text());
                if matches!(outcome, Outcome::Refused(_)) {
                    // Keep the text so it can be corrected.
                    self.input = Some((field, input));
                }
                self.carry_out(outcome)
            }
            UiResponse::NotHandled => false,
            _ => true,
        }
    }

    fn answer(&mut self, question: Question, key: &KeyWithModifier) -> bool {
        let BareKey::Char(answer) = key.bare_key else {
            if is_plain(key, BareKey::Esc) {
                self.question = None;
                return true;
            }
            return false;
        };
        match (question, answer) {
            (Question::DiscardAndClose, 'y') => {
                if let Some(Ok(editor)) = &mut self.editor {
                    editor.discard();
                }
                self.preview();
                self.revert_runtime();
                close_self();
            }
            (Question::DiscardAndClose, 'n') => {}
            (Question::OverwriteOrReload, 'o') => {
                self.waiting = Some(Waiting::Save);
                overwrite_config_file();
            }
            (Question::OverwriteOrReload, 'r') => {
                // Reloading takes the file as it is now. The edits are lost,
                // so the bars go back to the file's values too.
                if let Some(Ok(editor)) = &mut self.editor {
                    editor.discard();
                }
                self.preview();
                // Reloading keeps runtime aliases the file did not change, so
                // the failed save's edits would survive it.
                self.revert_runtime();
                self.waiting = Some(Waiting::Reload);
                reload_config_file();
            }
            _ => return false,
        }
        self.question = None;
        true
    }

    /// Tell the bars about every change since they were last told.
    fn preview(&mut self) {
        let Some(Ok(editor)) = &self.editor else {
            return;
        };
        let changes = live::changes(&self.previewed, editor.working());
        if changes.is_empty() {
            return;
        }
        // No URL and no destination: every plugin in the session gets it,
        // and nothing new is launched.
        pipe_message_to_plugin(
            MessageToPlugin::new(live::MESSAGE_NAME)
                .with_args(live::encode(editor.alias_name(), &changes)),
        );
        self.previewed = editor.working().clone();
    }

    /// Write the edits to the config file, keeping its comments and layout.
    fn save(&mut self) {
        let Some(Ok(editor)) = &self.editor else {
            return;
        };
        if !editor.is_dirty() {
            self.status = Some(("Nothing to save".to_string(), false));
            return;
        }
        // The plugins block replaces every alias, so all of them go back,
        // the others unchanged. Saving then rewrites only the changed keys.
        replace_config_blocks(plugin_aliases_kdl(&editor.aliases_to_save()));
        save_config();
        self.runtime_ahead = true;
        self.waiting = Some(Waiting::Save);
        self.status = Some(("Saving…".to_string(), false));
    }

    /// Put the session's aliases back to the file's after a save that did
    /// not reach it, so bars started later do not load the dropped edits.
    fn revert_runtime(&mut self) {
        if std::mem::take(&mut self.runtime_ahead) {
            revert_config(Some(SettingKey::PluginAliases));
        }
    }

    pub fn render(&mut self, rows: usize, cols: usize) {
        let editor = match &mut self.editor {
            None => {
                print_text_with_coordinates(
                    Text::from("Waiting for permission…"),
                    0,
                    0,
                    None,
                    None,
                );
                return;
            }
            Some(Err(message)) => {
                print_text_with_coordinates(
                    Text::from(view::truncate(message, cols)).error_color_all(),
                    0,
                    0,
                    None,
                    None,
                );
                print_text_with_coordinates(help(&[("Esc", "close")], cols), 0, 2, None, None);
                return;
            }
            Some(Ok(editor)) => editor,
        };

        let sections = editor.sections();
        let titles: Vec<&str> = sections.iter().map(|section| section.title()).collect();
        let layout = Layout::new(rows, cols, &titles);
        if layout.is_collapsed() {
            // Nothing to focus but the list without a sidebar.
            editor.focus = Focus::List;
        }
        self.layout = Some(layout);
        let editor = &*editor;

        let title = view::title(editor, self.view);
        let unsaved_from = title.find('●').map(|byte| title[..byte].chars().count());
        let mut title_text = Text::from(view::truncate(&title, cols)).color_range(2, ..);
        if let Some(from) = unsaved_from {
            title_text = title_text.color_range(1, from..);
        }
        print_text_with_coordinates(title_text, 0, 0, None, None);

        // Made whether or not there is room for the sidebar, so clicks find
        // both scroll views either way.
        let sidebar_scroll = self
            .sidebar_scroll
            .get_or_insert_with(|| ScrollView::new(0));
        match layout.sidebar {
            Some(width) => {
                let scroll = sidebar_scroll;
                scroll.set_total_rows(sections.len());
                scroll.layout(0, layout.body_y, width, layout.body_height);
                if self.shown_section != Some(editor.section()) {
                    scroll.ensure_visible(editor.section_index());
                    self.shown_section = Some(editor.section());
                }
                let content = scroll.content_width();
                for (index, section) in sections.iter().enumerate() {
                    let Some(y) = scroll.screen_row(index) else {
                        continue;
                    };
                    let line = view::sidebar_line(editor, *section, content);
                    let mut text = styled(&line);
                    if *section == editor.section() {
                        text = match editor.focus {
                            Focus::Sidebar => text.selected(),
                            Focus::List => text.color_range(2, ..),
                        };
                    }
                    print_text_with_coordinates(text, 0, y, Some(content), None);
                }
                scroll.render_indicators();
                for y in layout.body_y..layout.body_y + layout.body_height {
                    print_text_with_coordinates(Text::from("│").dim_all(), width, y, None, None);
                }
            }
            None => {
                // Bring the section into view again once the sidebar is back.
                self.shown_section = None;
                if layout.body_height > 0 {
                    let line = view::header_line(editor, cols);
                    let text = styled(&line).color_range(2, ..);
                    print_text_with_coordinates(text, 0, layout.body_y, None, None);
                }
            }
        }

        let list = editor.rows();
        let scroll = self.scroll.get_or_insert_with(|| ScrollView::new(0));
        scroll.set_total_rows(list.len());
        scroll.layout(
            layout.list_x,
            layout.list_y,
            layout.list_width,
            layout.list_height,
        );
        // Bring the selection into view when it moves, from the top when the
        // section changed, and leave the view where the wheel put it
        // otherwise.
        let selected = editor.selected;
        let shown = (editor.section(), selected);
        if self.shown_selected != Some(shown) {
            if self.shown_selected.map(|(section, _)| section) != Some(editor.section()) {
                scroll.set_offset(0);
            }
            scroll.ensure_visible(selected);
            self.shown_selected = Some(shown);
        }
        let width = scroll.content_width().saturating_sub(1);
        let list_focused = editor.focus == Focus::List;
        let mut editing = self.input.as_mut();
        let row_lines = view::Rows::new(editor, self.view);
        for (index, row) in list.iter().enumerate() {
            let Some(y) = scroll.screen_row(index) else {
                continue;
            };
            let x = layout.list_x;
            if index == selected {
                if let Some((field, input)) = editing.take() {
                    let name = match field {
                        Field::Value { key } => key.as_str(),
                        Field::NewKey => "new key",
                    };
                    let label = format!("  {name:NAME_COLUMNS$} ");
                    let columns = label.chars().count();
                    print_text_with_coordinates(Text::from(&label).selected(), x, y, None, None);
                    input.render(x + columns, y, width.saturating_sub(columns));
                    continue;
                }
            }
            let line = row_lines.line(row, width);
            let mut text = styled(&line);
            if index == selected && list_focused {
                text = text.selected();
            }
            print_text_with_coordinates(text, x, y, Some(width), None);
            // Text draws only the theme's palette, so a rendered format
            // string and a swatch, which need true color, go over the top
            // as plain ANSI.
            if let Some((column, ansi)) = &line.styled {
                print_ansi(x + column, y, ansi);
            }
            if let Some((column, color)) = line.swatch {
                print_ansi(x + column, y, &view::swatch_ansi(color));
            }
        }
        scroll.render_indicators();

        let footer = rows.saturating_sub(FOOTER_LINES);
        print_text_with_coordinates(
            Text::from(view::truncate(&view::describe(editor), cols)).dim_all(),
            0,
            footer,
            None,
            None,
        );
        if let Some((status, is_error)) = &self.status {
            let text = Text::from(view::truncate(status, cols));
            let text = if *is_error {
                text.error_color_all()
            } else {
                text.success_color_all()
            };
            print_text_with_coordinates(text, 0, footer + 1, None, None);
        }
        let keys = match (self.question, &self.input) {
            (Some(Question::DiscardAndClose), _) => {
                vec![("y", "discard and close"), ("n", "keep editing")]
            }
            (Some(Question::OverwriteOrReload), _) => vec![
                ("o", "overwrite the changed file"),
                ("r", "reload it, dropping the edits"),
                ("Esc", "cancel"),
            ],
            (None, Some(_)) => vec![("Enter", "set"), ("Esc", "cancel")],
            (None, None) => view::help_keys(editor.focus, layout.is_collapsed(), self.view),
        };
        if let Some(question) = self.question {
            let prompt = match question {
                Question::DiscardAndClose => "Discard the unsaved edits?",
                Question::OverwriteOrReload => "The config file changed since it was read.",
            };
            print_text_with_coordinates(
                Text::from(view::truncate(prompt, cols)).error_color_all(),
                0,
                footer + 1,
                None,
                None,
            );
        }
        print_text_with_coordinates(help(&keys, cols), 0, footer + 2, None, None);
    }
}

/// A line as a `Text`, with its dimmed part and edited marker colored.
fn styled(line: &Line) -> Text {
    let mut text = Text::from(&line.text);
    if let Some(range) = line.dim.clone() {
        text = text.dim_range(range);
    }
    if let Some(range) = line.edited.clone() {
        text = text.color_range(1, range);
    }
    text
}

/// Draw ANSI at a column and line of the pane, from a clean style.
fn print_ansi(x: usize, y: usize, ansi: &str) {
    print!("\u{1b}[{};{}H\u{1b}[0m{ansi}", y + 1, x + 1);
}

fn help(keys: &[(&str, &str)], cols: usize) -> Text {
    let mut line = String::new();
    let mut key_ranges = Vec::new();
    for (key, action) in keys {
        if !line.is_empty() {
            line.push_str("  ");
        }
        let start = line.chars().count();
        line.push_str(key);
        key_ranges.push(start..line.chars().count());
        line.push(' ');
        line.push_str(action);
    }
    let line = view::truncate(&line, cols);
    let visible = line.chars().count();
    key_ranges
        .into_iter()
        .filter(|range| range.end <= visible)
        .fold(Text::from(&line), |text, range| text.color_range(3, range))
}

fn is_plain(key: &KeyWithModifier, bare: BareKey) -> bool {
    key.bare_key == bare && key.key_modifiers.is_empty()
}
