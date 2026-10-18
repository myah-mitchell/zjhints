//! The settings screen as a running plugin: reading the config, turning keys
//! and clicks into edits, previewing them on the bars, saving, and drawing.

use super::editor::{Editor, Field, Outcome, Row};
use super::live;
use super::options::Kind;
use std::collections::BTreeMap;
use zellij_tile::prelude::*;
use zellij_utils::input::config_blocks::plugin_aliases_kdl;

/// Lines above the list: the title and a blank line.
const HEADER_LINES: usize = 2;
/// Lines below the list: the selected row's description, a status line and
/// the key help.
const FOOTER_LINES: usize = 3;
/// Columns a row's name is padded to, wide enough for the longest option
/// name. A longer key, such as a label's, pushes its value along.
const NAME_COLUMNS: usize = 21;

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
    scroll: Option<ScrollView>,
    /// The row the list was last scrolled to show, so the view follows the
    /// selection when it moves and the scroll wheel otherwise.
    shown_selected: Option<usize>,
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
        let page = self
            .scroll
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
            BareKey::Enter | BareKey::Char(' ') => editor.activate(),
            BareKey::Backspace | BareKey::Delete | BareKey::Char('d') => editor.reset_selected(),
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
        let (Some(Ok(editor)), Some(scroll)) = (&mut self.editor, &mut self.scroll) else {
            return false;
        };
        match mouse {
            Mouse::ScrollUp(lines) => scroll.scroll_by(-(lines as isize)),
            Mouse::ScrollDown(lines) => scroll.scroll_by(lines as isize),
            Mouse::LeftClick(line, _) => {
                let Some(row) = scroll.row_at(line) else {
                    return false;
                };
                // A click selects; a click on the selected row acts on it.
                let outcome = if row == editor.selected {
                    editor.activate()
                } else {
                    editor.select(row)
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
        let editor = match &self.editor {
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
                    Text::from(truncate(message, cols)).error_color_all(),
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

        let title = title(editor);
        let unsaved_from = title.find('●').map(|byte| title[..byte].chars().count());
        let mut title_text = Text::from(truncate(&title, cols)).color_range(2, ..);
        if let Some(from) = unsaved_from {
            title_text = title_text.color_range(1, from..);
        }
        print_text_with_coordinates(title_text, 0, 0, None, None);

        let list = editor.rows();
        let height = rows.saturating_sub(HEADER_LINES + FOOTER_LINES);
        let scroll = self.scroll.get_or_insert_with(|| ScrollView::new(0));
        scroll.set_total_rows(list.len());
        scroll.layout(0, HEADER_LINES, cols, height);
        // Bring the selection into view when it moves, with the heading over
        // it when there is one, and leave the view where the wheel put it
        // otherwise.
        let selected = editor.selected;
        if self.shown_selected != Some(selected) {
            if selected > 0 && matches!(list[selected - 1], Row::Heading(_)) {
                scroll.ensure_range_visible(selected - 1, 2);
            } else {
                scroll.ensure_visible(selected);
            }
            self.shown_selected = Some(selected);
        }
        let width = scroll.content_width().saturating_sub(1);
        let mut editing = self.input.as_mut();
        for (index, row) in list.iter().enumerate() {
            let Some(y) = scroll.screen_row(index) else {
                continue;
            };
            if index == selected {
                if let Some((field, input)) = editing.take() {
                    let name = match field {
                        Field::Value { key } => key.as_str(),
                        Field::NewKey => "new key",
                    };
                    let label = format!("  {name:NAME_COLUMNS$} ");
                    print_text_with_coordinates(Text::from(&label).selected(), 0, y, None, None);
                    input.render(label.chars().count(), y, width.saturating_sub(label.len()));
                    continue;
                }
            }
            let line = row_line(editor, row, width);
            let mut text = Text::from(&line.text);
            if let Some(range) = line.dim {
                text = text.dim_range(range);
            }
            if let Some(range) = line.edited {
                text = text.color_range(1, range);
            }
            if line.heading {
                text = text.color_all(2);
            }
            if index == selected {
                text = text.selected();
            }
            print_text_with_coordinates(text, 0, y, Some(width), None);
        }
        scroll.render_indicators();

        let footer = rows.saturating_sub(FOOTER_LINES);
        let description = describe(editor.selected_row().as_ref());
        print_text_with_coordinates(
            Text::from(truncate(&description, cols)).dim_all(),
            0,
            footer,
            None,
            None,
        );
        if let Some((status, is_error)) = &self.status {
            let text = Text::from(truncate(status, cols));
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
            (None, None) => vec![
                ("↑↓", "move"),
                ("Enter", "change"),
                ("Del", "reset"),
                ("s", "save"),
                ("Esc", "close"),
            ],
        };
        if let Some(question) = self.question {
            let prompt = match question {
                Question::DiscardAndClose => "Discard the unsaved edits?",
                Question::OverwriteOrReload => "The config file changed since it was read.",
            };
            print_text_with_coordinates(
                Text::from(truncate(prompt, cols)).error_color_all(),
                0,
                footer + 1,
                None,
                None,
            );
        }
        print_text_with_coordinates(help(&keys, cols), 0, footer + 2, None, None);
    }
}

/// The screen's first line: what is being edited and whether it is saved.
fn title(editor: &Editor) -> String {
    let mut title = format!("zjhints settings · alias \"{}\"", editor.alias_name());
    if editor.is_dirty() {
        title.push_str("  ● unsaved");
    }
    title
}

/// One row as text, with the parts to draw differently.
#[derive(Debug, PartialEq)]
struct Line {
    text: String,
    heading: bool,
    /// The characters of a value that is not set, so the default shows.
    dim: Option<std::ops::Range<usize>>,
    /// The characters of the marker on an edited value.
    edited: Option<std::ops::Range<usize>>,
}

fn row_line(editor: &Editor, row: &Row, width: usize) -> Line {
    let plain = |text: String| Line {
        text: truncate(&text, width),
        heading: false,
        dim: None,
        edited: None,
    };
    let (key, value, default) = match row {
        Row::Heading(title) => {
            return Line {
                text: truncate(title, width),
                heading: true,
                dim: None,
                edited: None,
            }
        }
        Row::Alias => {
            return plain(format!(
                "  {:NAME_COLUMNS$} {}  (Enter for the next of {})",
                "alias",
                editor.alias_name(),
                editor.alias_count()
            ))
        }
        Row::AddOther => return plain("  + add a key".to_string()),
        Row::Option(option) => (option.name, editor.value(option.name), option.default),
        Row::Other(key) => (key.as_str(), editor.value(key), None),
    };
    let prefix = format!("  {key:NAME_COLUMNS$} ");
    let start = prefix.chars().count();
    let (shown, dim) = match value {
        Some(value) => (show_value(value), false),
        None => (
            match default {
                Some(default) => format!("{} (default)", show_value(default)),
                None => "unset".to_string(),
            },
            true,
        ),
    };
    let mut text = format!("{prefix}{shown}");
    let end = text.chars().count();
    let mut edited = None;
    if editor.is_edited(key) {
        text.push_str("  ●");
        edited = Some(end + 2..end + 3);
    }
    let text = truncate(&text, width);
    let visible = text.chars().count();
    Line {
        text,
        heading: false,
        dim: dim.then(|| start.min(visible)..end.min(visible)),
        edited: edited.filter(|range| range.end <= visible),
    }
}

/// A value as shown in the list: quoted when it would otherwise be invisible
/// or ambiguous.
fn show_value(value: &str) -> String {
    if value.is_empty() || value.trim() != value {
        format!("\"{value}\"")
    } else {
        value.to_string()
    }
}

/// The footer line about the selected row.
fn describe(row: Option<&Row>) -> String {
    match row {
        Some(Row::Option(option)) => {
            let default = option.default.unwrap_or("unset");
            let how = match option.kind {
                Kind::Bool => "Enter toggles".to_string(),
                Kind::Choice(choices) => format!("Enter cycles {}", choices.join(" / ")),
                Kind::Integer | Kind::Fraction | Kind::Text => "Enter edits".to_string(),
            };
            format!("{}. Default {default}. {how}.", option.description)
        }
        Some(Row::Other(key)) => match super::options::family_of(key) {
            Some(family) => format!(
                "{} ({}). Del removes it.",
                family.description, family.pattern
            ),
            None => "Not an option zjhints reads. Del removes it.".to_string(),
        },
        Some(Row::AddOther) => {
            "Add a key not listed above, such as label_<id> or color_<name>.".to_string()
        }
        Some(Row::Alias) => "The plugins {} alias being edited.".to_string(),
        Some(Row::Heading(_)) | None => String::new(),
    }
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
    let line = truncate(&line, cols);
    let visible = line.chars().count();
    key_ranges
        .into_iter()
        .filter(|range| range.end <= visible)
        .fold(Text::from(&line), |text, range| text.color_range(3, range))
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(width.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

fn is_plain(key: &KeyWithModifier, bare: BareKey) -> bool {
    key.bare_key == bare && key.key_modifiers.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::options;

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

    #[test]
    fn an_unset_option_shows_its_default_dimmed() {
        let line = row_line(&editor(&[]), &option_row("auto_width"), 80);
        assert_eq!(line.text, format!("  {:22}true (default)", "auto_width"));
        assert_eq!(line.dim, Some(24..38));
        assert_eq!(line.edited, None);
    }

    #[test]
    fn a_set_option_shows_its_value_plainly() {
        let line = row_line(
            &editor(&[("max_length", "40")]),
            &option_row("max_length"),
            80,
        );
        assert_eq!(line.text, format!("  {:22}40", "max_length"));
        assert_eq!(line.dim, None);
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
        let line = row_line(&editor, &option_row("hint_spacer"), 80);
        assert_eq!(line.text, format!("  {:22}\" | \"  ●", "hint_spacer"));
        assert_eq!(line.edited, Some(31..32));
        assert!(title(&editor).ends_with("● unsaved"));
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
        let line = row_line(&editor, &option_row("hint_spacer"), 20);
        assert_eq!(line.text.chars().count(), 20);
        assert!(line.text.ends_with('…'));
        assert_eq!(line.edited, None);
    }

    #[test]
    fn a_key_longer_than_the_name_column_keeps_a_space_before_its_value() {
        let key = "label_locked_switch_to_mode_normal";
        let line = row_line(
            &editor(&[(key, "Unlock")]),
            &Row::Other(key.to_string()),
            80,
        );
        assert_eq!(line.text, format!("  {key} Unlock"));
    }

    #[test]
    fn the_footer_explains_the_selected_row() {
        let text = describe(Some(&option_row("key_order")));
        assert!(text.starts_with("How keys within a hint are ordered. Default qwerty."));
        assert!(text.contains("qwerty / dvorak"));
        let text = describe(Some(&Row::Other("color_blue".to_string())));
        assert!(text.contains("color_<name>"));
    }
}
