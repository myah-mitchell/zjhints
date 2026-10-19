//! The settings screen as a running plugin: reading the config, turning keys
//! and clicks into edits, previewing them on the bars, saving, and drawing.
//! It looks and answers keys like Zellij's own settings screen.

use super::editor::{Editor, Field, Focus, Outcome, Row, Section};
use super::live;
use super::view::{self, Command, Control, FormatView, HelpState, Layout, Marker, RowView};
use std::collections::BTreeMap;
use zellij_tile::prelude::*;
use zellij_utils::input::config_blocks::plugin_aliases_kdl;

/// What the host is to report back on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Waiting {
    Save,
    Reload,
}

/// What a prompt shown by the host asks.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Question {
    RevertAll,
    /// The file changed on disk since it was read, so saving would lose that.
    ChangedOutside,
}

/// How long the notice about unsaved edits stays up before the screen
/// closes.
const CLOSE_SECONDS: f64 = 1.5;

/// Where a row of the list was drawn, for the clicks after it.
#[derive(Clone, Copy, Debug)]
struct RowArea {
    /// Index into `Editor::rows`.
    row: usize,
    area: Rect,
}

#[derive(Default)]
pub struct SettingsScreen {
    /// The alias the `alias` option pins, if any.
    pinned_alias: Option<String>,
    /// What is being edited, or why nothing can be.
    editor: Option<Result<Editor, String>>,
    /// The config file, as the header names it.
    config_file: Option<String>,
    /// The value being typed, and what it is for.
    input: Option<(Field, TextInput)>,
    /// The search box, shown in the menu's place while searching.
    search: Option<TextInput>,
    /// The sections down the left.
    menu: Option<SideMenu>,
    /// The open list of choices, and the index of its row in the list.
    dropdown: Option<(usize, Dropdown)>,
    /// The buttons in the header, and the one over the preview.
    buttons: Vec<(Command, Button)>,
    overrides_button: Option<Button>,
    /// Timers the buttons started to show a press, which come back before
    /// the one closing the screen.
    button_timers: usize,
    /// The last thing worth telling the user.
    notice: Option<String>,
    /// The prompt the host is showing, by its id.
    question: Option<(u64, Question)>,
    /// The screen closes on the next timer or key.
    closing: bool,
    /// The alias's configuration as the bars were last told it.
    previewed: BTreeMap<String, String>,
    /// A save or reload was asked for and its result has not come back yet.
    waiting: Option<Waiting>,
    /// How format strings are shown in the list.
    view: FormatView,
    /// Where everything went at the last render, for the clicks after it.
    layout: Option<Layout>,
    scroll: Option<ScrollView>,
    rows: Vec<RowArea>,
    /// The row under the pointer.
    hovered_row: Option<usize>,
    /// Columns the labels of the list took at the last render, which a
    /// field or list opened after it lines up with.
    label_width: usize,
    /// The section and row the list was last scrolled to show, and whether
    /// it showed search results, so the view follows the selection when it
    /// moves and the scroll wheel otherwise.
    shown_selected: Option<(Section, usize, bool)>,
    /// The session's mode, keybindings and Zellij theme, which the preview
    /// draws the bar in.
    mode_info: Option<ModeInfo>,
    /// The preview leaves out the user's overrides of the themes.
    overrides_hidden: bool,
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
            EventType::Timer,
            EventType::BeforeClose,
            EventType::PromptResult,
            EventType::PermissionRequestResult,
            EventType::ConfigWasWrittenToDisk,
            EventType::FailedToWriteConfigToDisk,
            EventType::ConfigFileChangedSinceRead,
            // For the preview.
            EventType::ModeUpdate,
            EventType::InitialKeybinds,
        ]);
    }

    pub fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(PermissionStatus::Granted) => {
                // Pinned, the screen stays up when a click lands in another
                // pane, so it can sit beside the bar it edits without holding
                // the keyboard. Pinning needs `ChangeApplicationState`, one
                // of the mouse permissions asked for above.
                set_floating_pane_pinned(PaneId::Plugin(get_plugin_ids().plugin_id), true);
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
            Event::Timer(_) => self.handle_timer(),
            Event::BeforeClose => {
                self.keep_in_session();
                false
            }
            Event::PromptResult(id, result) => self.answer(id, &result),
            Event::ModeUpdate(mut mode_info) => {
                // With `InitialKeybinds` subscribed, the keybindings arrive
                // on their own rather than with every mode.
                if let Some(previous) = &self.mode_info {
                    if mode_info.keybinds.is_empty() {
                        mode_info.keybinds = previous.keybinds.clone();
                    }
                }
                let changed = self.mode_info.as_ref() != Some(&mode_info);
                self.mode_info = Some(mode_info);
                changed && self.shows_preview()
            }
            Event::InitialKeybinds(keybinds) => {
                self.mode_info
                    .get_or_insert_with(ModeInfo::default)
                    .keybinds = keybinds;
                self.shows_preview()
            }
            Event::ConfigWasWrittenToDisk if self.waiting.is_some() => {
                let done = match self.waiting.take() {
                    Some(Waiting::Reload) => "Reloaded the config file",
                    _ => "Saved",
                };
                self.reread();
                if let Some(Ok(editor)) = &self.editor {
                    self.previewed = editor.working().clone();
                }
                self.notice = Some(done.to_string());
                true
            }
            Event::FailedToWriteConfigToDisk(path) if self.waiting.is_some() => {
                let verb = match self.waiting.take() {
                    Some(Waiting::Reload) => "read",
                    _ => "write",
                };
                let path = path.unwrap_or_else(|| "the config file".to_string());
                self.notice = Some(format!("Could not {verb} {path}"));
                true
            }
            Event::ConfigFileChangedSinceRead if self.waiting == Some(Waiting::Save) => {
                self.waiting = None;
                self.ask_changed_outside();
                true
            }
            Event::ConfigFileChangedSinceRead => {
                // Values not saved, or applied to the session only, would be
                // lost to the file's.
                let unsaved = matches!(
                    &self.editor,
                    Some(Ok(editor)) if editor.is_dirty() || editor.session_differs()
                );
                if !unsaved {
                    self.reread();
                }
                true
            }
            _ => false,
        }
    }

    /// Whether the list on screen has the preview under it.
    fn shows_preview(&self) -> bool {
        matches!(&self.editor, Some(Ok(editor)) if view::is_previewed(editor.section()) && editor.search().is_none())
    }

    /// Read the aliases fresh and start editing.
    fn read(&mut self) {
        let snapshot = read_config();
        self.config_file = snapshot.config_file_path.clone();
        let editor = Editor::new(
            snapshot.blocks.plugin_aliases,
            snapshot.saved_blocks.plugin_aliases,
            self.pinned_alias.as_deref(),
        );
        if let Ok(editor) = &editor {
            self.previewed = editor.working().clone();
        }
        self.editor = Some(editor);
    }

    /// Read the aliases again after the config changed under the editor.
    fn reread(&mut self) {
        let snapshot = read_config();
        self.config_file = snapshot.config_file_path.clone();
        match &mut self.editor {
            Some(Ok(editor)) => {
                let reread = editor.reread(
                    snapshot.blocks.plugin_aliases,
                    snapshot.saved_blocks.plugin_aliases,
                );
                if let Err(message) = reread {
                    self.editor = Some(Err(message));
                }
            }
            _ => self.read(),
        }
    }

    fn editor_mut(&mut self) -> Option<&mut Editor> {
        match &mut self.editor {
            Some(Ok(editor)) => Some(editor),
            _ => None,
        }
    }

    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        if self.closing {
            close_self();
            return false;
        }
        if self.question.is_some() {
            return false;
        }
        if self.input.is_some() {
            return self.type_key(&key);
        }
        if self.dropdown.is_some() {
            return self.choose_key(&key);
        }
        if key.has_modifiers(&[KeyModifier::Ctrl]) && key.key_modifiers.len() == 1 {
            return match key.bare_key {
                BareKey::Char('a') => self.run(Command::Save),
                BareKey::Char('r') => self.run(Command::RevertAll),
                _ => false,
            };
        }
        let previewed = self.shows_preview();
        let Some(editor) = self.editor_mut() else {
            if is_plain(&key, BareKey::Esc) {
                close_self();
            }
            return false;
        };
        if is_shift_tab(&key) {
            let outcome = match editor.focus {
                Focus::List => match editor.move_selection(-1) {
                    Outcome::None => {
                        let menu = editor.menu_focus();
                        editor.set_focus(menu)
                    }
                    outcome => outcome,
                },
                _ => Outcome::None,
            };
            return self.carry_out(outcome);
        }
        if !key.key_modifiers.is_empty() {
            return false;
        }
        match key.bare_key {
            BareKey::Char('f') if editor.focus != Focus::Search => {
                return self.run(Command::ToggleFormats)
            }
            BareKey::Char('o') if editor.focus != Focus::Search && previewed => {
                self.overrides_hidden = !self.overrides_hidden;
                return true;
            }
            BareKey::Char('/') if editor.focus != Focus::Search => {
                let query = editor.search().unwrap_or_default().to_string();
                let outcome = editor.start_search(&query);
                let mut search = TextInput::new(query).search_mode().focused();
                search.move_to_end();
                self.search = Some(search);
                return self.carry_out(outcome);
            }
            _ => {}
        }
        match editor.focus {
            Focus::Sidebar => self.menu_key(&key),
            Focus::Search => self.search_key(&key),
            Focus::List => self.list_key(&key),
        }
    }

    fn menu_key(&mut self, key: &KeyWithModifier) -> bool {
        let Some(editor) = self.editor_mut() else {
            return false;
        };
        let outcome = match key.bare_key {
            BareKey::Up | BareKey::Char('k') => editor.move_selection(-1),
            BareKey::Down | BareKey::Char('j') => editor.move_selection(1),
            BareKey::Home => editor.select_section(0),
            BareKey::End => {
                let last = editor.sections().len() - 1;
                editor.select_section(last)
            }
            BareKey::Tab | BareKey::Right | BareKey::Char('l') | BareKey::Enter => {
                editor.set_focus(Focus::List)
            }
            BareKey::Esc => return self.begin_close(),
            _ => return false,
        };
        self.carry_out(outcome)
    }

    fn search_key(&mut self, key: &KeyWithModifier) -> bool {
        let (Some(Ok(editor)), Some(search)) = (&mut self.editor, &mut self.search) else {
            return false;
        };
        let outcome = match key.bare_key {
            BareKey::Down | BareKey::Tab | BareKey::Enter if editor.match_count() > 0 => {
                editor.set_focus(Focus::List)
            }
            _ => match search.handle_key(key) {
                UiResponse::Changed(_) => {
                    // Other matches: show them from the top.
                    self.shown_selected = None;
                    editor.start_search(search.get_text())
                }
                UiResponse::Cancelled => {
                    self.search = None;
                    editor.end_search()
                }
                UiResponse::NotHandled => return false,
                _ => Outcome::Redraw,
            },
        };
        self.carry_out(outcome)
    }

    fn list_key(&mut self, key: &KeyWithModifier) -> bool {
        let page = self
            .scroll
            .as_ref()
            .map_or(10, |scroll| scroll.viewport_height().max(1)) as isize;
        let Some(editor) = self.editor_mut() else {
            return false;
        };
        let choice_row = editor.choices().is_some();
        let searching = editor.search().is_some();
        let outcome = match key.bare_key {
            BareKey::Up | BareKey::Char('k') => match editor.move_selection(-1) {
                Outcome::None if searching => editor.set_focus(Focus::Search),
                outcome => outcome,
            },
            BareKey::Down | BareKey::Char('j') | BareKey::Tab => editor.move_selection(1),
            BareKey::PageUp => editor.move_selection(-page),
            BareKey::PageDown => editor.move_selection(page),
            BareKey::Home => editor.move_selection(isize::MIN / 2),
            BareKey::End => editor.move_selection(isize::MAX / 2),
            BareKey::Left if choice_row => editor.step_choice(false),
            BareKey::Right if choice_row => editor.step_choice(true),
            BareKey::Left | BareKey::Char('h') => {
                let menu = editor.menu_focus();
                editor.set_focus(menu)
            }
            BareKey::Enter => editor.activate(),
            // Space changes a value in place; typing a value starts with
            // Enter, as in Zellij's settings.
            BareKey::Char(' ') if !is_text_row(editor) => editor.activate(),
            BareKey::Backspace | BareKey::Delete => editor.reset_selected(),
            BareKey::Char('r') => match editor.revert_selected() {
                Some(key) => {
                    self.notice = Some(format!("Reverted {}", crate::show_controls(&key)));
                    self.preview();
                    return true;
                }
                None => return false,
            },
            BareKey::Esc if searching => {
                let outcome = editor.end_search();
                self.search = None;
                outcome
            }
            BareKey::Esc => return self.begin_close(),
            _ => return false,
        };
        self.carry_out(outcome)
    }

    /// Pass a key to the open dropdown.
    fn choose_key(&mut self, key: &KeyWithModifier) -> bool {
        let Some((_, dropdown)) = &mut self.dropdown else {
            return false;
        };
        let response = dropdown.handle_key(key);
        self.dropdown_response(response)
    }

    fn dropdown_response(&mut self, response: UiResponse) -> bool {
        match response {
            UiResponse::Changed(UiValue::Choice { index, .. }) => {
                self.dropdown = None;
                let Some(editor) = self.editor_mut() else {
                    return true;
                };
                let outcome = editor.choose(index);
                self.carry_out(outcome);
                true
            }
            UiResponse::Cancelled | UiResponse::NotHandled => {
                self.dropdown = None;
                true
            }
            _ => {
                // The list stays open, with the highlight moved.
                if !self
                    .dropdown
                    .as_ref()
                    .is_some_and(|(_, dropdown)| dropdown.is_open())
                {
                    self.dropdown = None;
                }
                true
            }
        }
    }

    fn handle_mouse(&mut self, mouse: Mouse) -> bool {
        if self.question.is_some() || self.closing {
            return false;
        }
        if let Some((_, dropdown)) = &mut self.dropdown {
            let response = dropdown.handle_mouse(mouse);
            return match response {
                UiResponse::NotHandled => false,
                response => self.dropdown_response(response),
            };
        }
        if self.layout.is_none() {
            return false;
        }
        match mouse {
            Mouse::Hover(line, column) => self.hover(line, column),
            Mouse::ScrollUp(lines) | Mouse::ScrollDown(lines) => {
                if let Some(menu) = &mut self.menu {
                    if menu.handle_mouse(mouse) != UiResponse::NotHandled {
                        return true;
                    }
                }
                let delta = lines.max(1) as isize;
                let delta = if matches!(mouse, Mouse::ScrollUp(_)) {
                    -delta
                } else {
                    delta
                };
                self.scroll
                    .as_mut()
                    .is_some_and(|scroll| scroll.scroll_by(delta))
            }
            Mouse::LeftClick(line, column) => self.click(line, column),
            _ => false,
        }
    }

    /// Track what the pointer is over, redrawing when that changes.
    fn hover(&mut self, line: isize, column: usize) -> bool {
        let mouse = Mouse::Hover(line, column);
        let mut changed = false;
        for (_, button) in &mut self.buttons {
            changed |= button.handle_mouse(mouse) != UiResponse::NotHandled;
        }
        if let Some(button) = &mut self.overrides_button {
            changed |= button.handle_mouse(mouse) != UiResponse::NotHandled;
        }
        if let Some(menu) = &mut self.menu {
            changed |= menu.handle_mouse(mouse) != UiResponse::NotHandled;
        }
        let hovered = self.row_at(line, column);
        changed |= hovered != self.hovered_row;
        self.hovered_row = hovered;
        changed
    }

    fn row_at(&self, line: isize, column: usize) -> Option<usize> {
        self.rows
            .iter()
            .find(|row| row.area.contains(line, column))
            .map(|row| row.row)
    }

    fn click(&mut self, line: isize, column: usize) -> bool {
        let mouse = Mouse::LeftClick(line, column);
        // A click anywhere else ends typing, dropping what was typed.
        if let Some((_, input)) = &mut self.input {
            if input.hit_test(line, column) {
                input.handle_mouse(mouse);
                return true;
            }
            self.input = None;
        }
        let command = self.buttons.iter_mut().find_map(|(command, button)| {
            (button.handle_mouse(mouse) == UiResponse::Activated).then_some(*command)
        });
        if let Some(command) = command {
            self.button_timers += 1;
            return self.run(command);
        }
        if let Some(button) = &mut self.overrides_button {
            if button.handle_mouse(mouse) == UiResponse::Activated {
                self.button_timers += 1;
                self.overrides_hidden = !self.overrides_hidden;
                return true;
            }
        }
        if let Some(search) = &mut self.search {
            if search.hit_test(line, column) {
                search.handle_mouse(mouse);
                let Some(editor) = self.editor_mut() else {
                    return false;
                };
                let outcome = editor.set_focus(Focus::Search);
                self.carry_out(outcome);
                return true;
            }
        }
        if let (Some(menu), Some(Ok(editor))) = (&mut self.menu, &mut self.editor) {
            if editor.search().is_none() && menu.hit_test(line, column) {
                menu.handle_mouse(mouse);
                let shown = editor.select_section(menu.selected_index());
                let focused = editor.set_focus(Focus::Sidebar);
                let outcome = if shown == Outcome::None {
                    focused
                } else {
                    shown
                };
                self.carry_out(outcome);
                return true;
            }
        }
        if let Some(row) = self.row_at(line, column) {
            // A click selects the row and acts on it, as a key would.
            let Some(editor) = self.editor_mut() else {
                return false;
            };
            editor.select(row);
            let outcome = editor.activate();
            self.carry_out(outcome);
            return true;
        }
        false
    }

    fn handle_timer(&mut self) -> bool {
        if self.button_timers > 0 {
            self.button_timers -= 1;
            let mut changed = false;
            for (_, button) in &mut self.buttons {
                changed |= button.handle_timer();
            }
            if let Some(button) = &mut self.overrides_button {
                changed |= button.handle_timer();
            }
            return changed;
        }
        if self.closing {
            close_self();
        }
        false
    }

    /// Run a header command, from its key or its button.
    fn run(&mut self, command: Command) -> bool {
        match command {
            Command::Save => self.save(),
            Command::RevertAll => self.ask_revert_all(),
            Command::ToggleFormats => self.view = self.view.toggled(),
            Command::Close => return self.begin_close(),
        }
        true
    }

    fn carry_out(&mut self, outcome: Outcome) -> bool {
        match outcome {
            Outcome::None => false,
            Outcome::Redraw => true,
            Outcome::StartField { field, text } => {
                let label = match &field {
                    Field::Value { key } => crate::show_controls(key).into_owned(),
                    Field::NewKey => "new key".to_string(),
                };
                let mut input = TextInput::new(text)
                    .label(self.label(&label).as_str())
                    .label_width(self.label_width)
                    .focused();
                input.move_to_end();
                self.input = Some((field, input));
                self.notice = None;
                true
            }
            Outcome::OpenChoices => {
                let Some(Ok(editor)) = &self.editor else {
                    return false;
                };
                let Some((choices, chosen)) = editor.choices() else {
                    return false;
                };
                let label = match editor.selected_row() {
                    Some(Row::Option(option)) => option.name,
                    _ => "alias",
                };
                let mut dropdown = Dropdown::new(self.label(label).as_str(), choices)
                    .selected(chosen)
                    .label_width(self.label_width)
                    .focused();
                dropdown.open();
                self.dropdown = Some((editor.selected, dropdown));
                true
            }
            Outcome::Changed => {
                self.notice = None;
                self.preview();
                true
            }
            Outcome::SwitchedAlias => {
                if let Some(Ok(editor)) = &self.editor {
                    self.previewed = editor.working().clone();
                }
                self.notice = None;
                true
            }
            Outcome::Refused(message) => {
                self.notice = Some(message);
                true
            }
        }
    }

    /// A row's label, cut to the label column.
    fn label(&self, label: &str) -> String {
        view::truncate(label, self.label_width.saturating_sub(2))
    }

    fn type_key(&mut self, key: &KeyWithModifier) -> bool {
        let Some((_, input)) = &mut self.input else {
            return false;
        };
        match input.handle_key(key) {
            UiResponse::Cancelled => {
                self.input = None;
                self.notice = None;
                true
            }
            UiResponse::Submitted(_) => {
                let (field, input) = self.input.take().expect("checked above");
                let Some(editor) = self.editor_mut() else {
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

    fn ask_revert_all(&mut self) {
        let unsaved = match &self.editor {
            Some(Ok(editor)) => editor.unsaved_count(),
            _ => return,
        };
        if unsaved == 0 {
            self.notice = Some("Nothing to revert".to_string());
            return;
        }
        let message = format!(
            "Put the {} back to the config file's values?",
            view::unsaved_text(unsaved)
        );
        let request = PromptRequest::confirm(message)
            .title("Revert all?")
            .yes("Revert all")
            .no("Cancel");
        self.question = Some((prompt(request), Question::RevertAll));
    }

    fn ask_changed_outside(&mut self) {
        let file = self
            .config_file
            .as_deref()
            .and_then(|path| path.rsplit('/').next())
            .unwrap_or("The config file")
            .to_string();
        let request = PromptRequest::menu(vec![
            ChoiceItem::labeled("overwrite", "Overwrite: save your changes over the file"),
            ChoiceItem::labeled(
                "reload",
                "Reload: take the file as it is, dropping your unsaved changes",
            ),
            ChoiceItem::labeled("cancel", "Cancel"),
        ])
        .title(format!("{file} changed outside Zellij"));
        self.question = Some((prompt(request), Question::ChangedOutside));
    }

    fn answer(&mut self, id: u64, result: &PromptResult) -> bool {
        let Some((asked, question)) = self.question else {
            return false;
        };
        if asked != id {
            return false;
        }
        self.question = None;
        match (question, result) {
            (Question::RevertAll, PromptResult::Confirmed(true)) => {
                if let Some(editor) = self.editor_mut() {
                    editor.revert_all();
                }
                self.preview();
                self.keep_in_session();
                self.notice = Some("Reverted all changes".to_string());
            }
            (Question::ChangedOutside, PromptResult::Answered(PromptValue::Choice(choice))) => {
                match choice.as_str() {
                    "overwrite" => {
                        self.waiting = Some(Waiting::Save);
                        overwrite_config_file();
                    }
                    "reload" => {
                        // Reloading takes the file as it is now. The edits
                        // are dropped, so the bars and the session go back to
                        // the file's values first: reloading keeps session
                        // aliases the file did not change.
                        if let Some(editor) = self.editor_mut() {
                            editor.revert_all();
                            editor.session_updated();
                        }
                        self.preview();
                        revert_config(Some(SettingKey::PluginAliases));
                        self.waiting = Some(Waiting::Reload);
                        reload_config_file();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
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

    /// Put the pending values in the session's alias, so bars started from
    /// now on load them, as Zellij keeps settings edits applied to the
    /// session until they are saved or reverted.
    fn keep_in_session(&mut self) {
        let Some(editor) = self.editor_mut() else {
            return;
        };
        if editor.session_differs() {
            replace_config_blocks(plugin_aliases_kdl(&editor.aliases_to_save()));
            editor.session_updated();
        }
    }

    /// Close, after a moment's notice when edits stay unsaved.
    fn begin_close(&mut self) -> bool {
        let unsaved = match &self.editor {
            Some(Ok(editor)) => editor.unsaved_count(),
            _ => 0,
        };
        self.keep_in_session();
        if unsaved == 0 {
            close_self();
            return false;
        }
        let noun = if unsaved == 1 {
            "change stays"
        } else {
            "changes stay"
        };
        self.notice = Some(format!(
            "{unsaved} unsaved {noun} applied to this session. Ctrl+a in settings saves them."
        ));
        self.closing = true;
        set_timeout(CLOSE_SECONDS);
        true
    }

    /// Write the edits to the config file, keeping its comments and layout.
    fn save(&mut self) {
        let Some(editor) = self.editor_mut() else {
            return;
        };
        if !editor.is_dirty() {
            self.notice = Some("Nothing to save".to_string());
            return;
        }
        // The plugins block replaces every alias, so all of them go back,
        // the others unchanged. Saving then rewrites only the changed keys.
        replace_config_blocks(plugin_aliases_kdl(&editor.aliases_to_save()));
        editor.session_updated();
        save_config();
        self.waiting = Some(Waiting::Save);
        self.notice = Some("Saving…".to_string());
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
                let (line, keys) = view::hint_line("Help: ", &[("<Esc>", "close")], cols);
                print_text_with_coordinates(colored_keys(&line, keys), 0, 2, None, None);
                return;
            }
            Some(Ok(editor)) => editor,
        };

        let items = view::menu_items(editor);
        let list = editor.rows();
        let searching = editor.search().is_some();
        let preview_rows =
            (view::is_previewed(editor.section()) && !searching).then_some(list.len());
        let Some(layout) = Layout::new(rows, cols, &items, preview_rows) else {
            self.layout = None;
            self.rows.clear();
            let message = format!(
                "Pane too small for settings ({}x{} needed)",
                view::MIN_COLS,
                view::MIN_ROWS
            );
            print_text_with_coordinates(
                Text::from(view::truncate(&message, cols)).color_all(3),
                0,
                rows / 2,
                None,
                None,
            );
            return;
        };
        self.layout = Some(layout);
        let editor = &*editor;

        // The header: the file, then the unsaved count and the buttons.
        let file = self.config_file.as_deref().unwrap_or("none");
        let file_line = format!("{}{file}", view::FILE_PREFIX);
        print_text_with_coordinates(
            Text::from(view::truncate(&file_line, layout.width)),
            layout.x,
            0,
            None,
            None,
        );
        let unsaved = editor.unsaved_count();
        let count = format!("{} · ", view::unsaved_text(unsaved));
        let mut count_text = Text::from(&count);
        if unsaved > 0 {
            count_text = count_text.color_range(1, ..view::unsaved_text(unsaved).chars().count());
        }
        print_text_with_coordinates(count_text, layout.x, 2, None, None);
        let header_buttons = view::header_buttons(self.view);
        if self.buttons.len() != header_buttons.len() {
            self.buttons = header_buttons
                .iter()
                .map(|(_, _, command)| (*command, Button::new("")))
                .collect();
        }
        let mut x = layout.x + view::columns(&count);
        for ((key, action, _), (_, button)) in header_buttons.iter().zip(&mut self.buttons) {
            let label = format!("{key} {action}");
            button.set_label(Text::from(&label).color_range(3, ..key.chars().count()));
            let width = button.natural_width();
            if x + width > layout.x + layout.width {
                button.clear_area();
                continue;
            }
            button.render(x, 2);
            x += width + 1;
        }

        // The menu, or the search box in its place.
        let menu = self
            .menu
            .get_or_insert_with(|| SideMenu::new(Vec::<String>::new()));
        menu.set_items(items.clone());
        menu.set_selected(editor.section_index());
        menu.set_focused(editor.focus == Focus::Sidebar);
        match &mut self.search {
            Some(search) if searching => {
                menu.clear_area();
                search.set_focused(editor.focus == Focus::Search);
                search.set_match_count(None);
                search.render(layout.x, layout.body_y, layout.menu_width);
                print_text_with_coordinates(
                    Text::from(view::truncate(
                        &view::match_text(editor.match_count()),
                        layout.menu_width,
                    ))
                    .dim_all(),
                    layout.x + 1,
                    layout.body_y + 2,
                    None,
                    None,
                );
            }
            _ => {
                self.search = None;
                menu.render(
                    layout.x,
                    layout.body_y,
                    layout.menu_width,
                    layout.body_height,
                );
                menu_markers(menu, &items, layout);
            }
        }

        // The rows.
        let scroll = self.scroll.get_or_insert_with(|| ScrollView::new(0));
        scroll.set_total_rows(list.len());
        scroll.layout(
            layout.content_x,
            layout.body_y,
            layout.content_width,
            layout.list_height,
        );
        // Bring the selection into view when it moves, from the top when the
        // list changed, and leave the view where the wheel put it otherwise.
        let selected = editor.selected;
        let shown = (editor.section(), selected, searching);
        if self.shown_selected != Some(shown) {
            let same_list = self
                .shown_selected
                .is_some_and(|(section, _, was_searching)| {
                    section == editor.section() && was_searching == searching
                });
            if !same_list {
                scroll.set_offset(0);
            }
            scroll.ensure_visible(selected);
            self.shown_selected = Some(shown);
        }
        let width = scroll.content_width().saturating_sub(1);
        let x = layout.content_x;
        let zellij_palette = self
            .mode_info
            .as_ref()
            .map(|mode_info| &mode_info.style.colors);
        let rows_view = view::Rows::new(editor, self.view, zellij_palette);
        let views: Vec<RowView> = list.iter().map(|row| rows_view.view(row)).collect();
        let (label_width, value_width) = view::field_widths(&views, width);
        let element_width = label_width + value_width;
        self.label_width = label_width;
        let list_focused = editor.focus == Focus::List;
        self.rows.clear();
        for (index, row_view) in views.iter().enumerate() {
            let Some(y) = scroll.screen_row(index) else {
                continue;
            };
            let focused = index == selected && list_focused;
            let hovered = self.hovered_row == Some(index) && self.dropdown.is_none();
            if let Some((_, input)) = self.input.as_mut().filter(|_| index == selected) {
                input.set_show_cursor(true);
                input.render(x, y, element_width);
                continue;
            }
            if let Some((_, dropdown)) = self.dropdown.as_mut().filter(|(row, _)| *row == index) {
                dropdown.render(x, y, element_width);
            } else {
                draw_row(
                    row_view,
                    x,
                    y,
                    label_width,
                    value_width,
                    width,
                    focused,
                    hovered,
                );
            }
            if !matches!(row_view.control, Control::Heading(_)) {
                self.rows.push(RowArea {
                    row: index,
                    area: Rect::new(x, y, element_width.min(width), 1),
                });
            }
        }
        if list.is_empty() {
            let empty = if searching {
                "No settings match"
            } else {
                "Nothing to show"
            };
            print_text_with_coordinates(Text::from(empty).dim_all(), x, layout.body_y, None, None);
        }
        scroll.render_indicators();

        // The preview, under a Theme list.
        match (layout.preview_y, &self.mode_info) {
            (Some(y), Some(mode_info)) => {
                let shown = !self.overrides_hidden;
                // With a theme's list open, the preview follows the
                // highlighted choice.
                let mut configuration = editor.working().clone();
                if let Some((row, dropdown)) = &self.dropdown {
                    if let (Some(Row::Option(option)), Some(index)) =
                        (list.get(*row), dropdown.highlighted_index())
                    {
                        if let Some(value) = dropdown.options().get(index) {
                            configuration.insert(option.name.to_string(), value.clone());
                        }
                    }
                }
                let overrides =
                    view::theme_overrides(&configuration, Some(&mode_info.style.colors));
                let caption = view::preview_caption(overrides.len(), shown);
                let caption = view::truncate(&caption, width);
                print_text_with_coordinates(Text::from(&caption).dim_all(), x, y, None, None);
                let button = self.overrides_button.get_or_insert_with(|| Button::new(""));
                if overrides.is_empty() {
                    button.clear_area();
                } else {
                    let (key, action) = view::overrides_button(shown);
                    let label = format!("{key} {action}");
                    button.set_label(Text::from(&label).color_range(3, ..key.chars().count()));
                    let button_x = x + view::columns(&caption) + 2;
                    if button_x + button.natural_width() <= x + width {
                        button.render(button_x, y);
                    } else {
                        button.clear_area();
                    }
                }
                let bar = view::theme_preview(&configuration, mode_info, shown, width);
                print_ansi(x, y + 1, &bar);
            }
            _ => {
                if let Some(button) = &mut self.overrides_button {
                    button.clear_area();
                }
            }
        }

        // The footer: a notice or the selected row's description, then the
        // keys, each under a blank row.
        let footer = match (&self.notice, view::describe(editor)) {
            (Some(notice), _) => {
                Some(Text::from(view::truncate(notice, layout.width)).color_all(3))
            }
            (None, Some(description)) => {
                Some(Text::from(view::truncate(&description, layout.width)).color_all(0))
            }
            (None, None) => None,
        };
        if let Some(text) = footer {
            print_text_with_coordinates(text, layout.x, layout.footer_y + 1, None, None);
        }
        let state = if self.input.is_some() {
            HelpState::Editing
        } else if self.dropdown.is_some() {
            HelpState::Choosing
        } else {
            HelpState::Browsing {
                focus: editor.focus,
                text_row: views.get(selected).is_some_and(RowView::is_text),
                searching,
            }
        };
        let (line, keys) = view::hint_line("Help: ", &view::help_hints(state), layout.width);
        print_text_with_coordinates(
            colored_keys(&line, keys),
            layout.x,
            layout.footer_y + 3,
            None,
            None,
        );

        // The open list goes over everything else.
        if let Some((_, dropdown)) = &mut self.dropdown {
            dropdown.render_overlay(rows, cols);
        }
    }
}

/// Whether the selected row is typed into.
fn is_text_row(editor: &Editor) -> bool {
    match editor.selected_row() {
        Some(row) => view::Rows::new(editor, FormatView::Raw, None)
            .view(&row)
            .is_text(),
        None => false,
    }
}

/// Draw one row of the list as its widget, with its swatch and marker.
#[allow(clippy::too_many_arguments)]
fn draw_row(
    row: &RowView,
    x: usize,
    y: usize,
    label_width: usize,
    value_width: usize,
    width: usize,
    focused: bool,
    hovered: bool,
) {
    let label = view::truncate(&row.label, label_width.saturating_sub(2));
    let element_width = label_width + value_width;
    match &row.control {
        Control::Heading(title) => {
            print_text_with_coordinates(
                Text::from(view::truncate(title, width)).color_all(2),
                x,
                y,
                None,
                None,
            );
            return;
        }
        Control::Button(text) => {
            let mut button = Button::new(text.as_str()).left_aligned();
            if focused {
                button = button.focused();
            }
            if hovered {
                button = button.hovered();
            }
            button.render(x, y);
            return;
        }
        Control::Toggle(on) => {
            let mut toggle = Toggle::new(label.as_str(), *on).label_width(label_width);
            toggle.set_field_width(value_width);
            if focused {
                toggle = toggle.focused();
            }
            if hovered {
                toggle = toggle.hovered();
            }
            toggle.render(x, y);
        }
        Control::Dropdown(options, chosen) => {
            let mut dropdown = Dropdown::new(label.as_str(), options.clone())
                .selected(*chosen)
                .label_width(label_width);
            if focused {
                dropdown = dropdown.focused();
            }
            if hovered {
                dropdown = dropdown.hovered();
            }
            dropdown.render(x, y, element_width);
        }
        Control::Text { text, placeholder } => {
            let mut input = if *placeholder {
                TextInput::new("").placeholder(text.as_str())
            } else {
                TextInput::new(text.as_str())
            }
            .label(label.as_str())
            .label_width(label_width);
            if focused {
                input = input.focused();
            }
            if hovered {
                input = input.hovered();
            }
            input.set_show_cursor(false);
            input.render(x, y, element_width);
            // Text draws only the theme's palette, so a rendered format
            // string, which needs true color, goes over the field's text as
            // plain ANSI. A hovered field keeps its highlight instead.
            if let Some(ansi) = row.styled.as_ref().filter(|_| !hovered) {
                let room = value_width.saturating_sub(4);
                print_ansi(x + label_width + 2, y, &view::take_visible(ansi, room));
            }
        }
    }
    let (swatch_x, marker_x) = view::trailer_columns(row, x + element_width, x + width);
    if let Some(color) = row.swatch {
        print_ansi(swatch_x, y, &view::swatch_ansi(color));
    }
    let marker = row.marker.text();
    if !marker.is_empty() {
        let text = Text::from(marker);
        let text = match row.marker {
            Marker::Unsaved => text.color_all(1),
            _ => text.dim_all(),
        };
        print_text_with_coordinates(text, marker_x, y, None, None);
    }
}

/// Redraw the ` ●` after a menu item with unsaved edits in the unsaved
/// color, as Zellij's settings screen does.
fn menu_markers(menu: &SideMenu, items: &[String], layout: Layout) {
    let has_indicators = items.len() > layout.body_height;
    let first_line = layout.body_y + usize::from(has_indicators);
    let visible_rows = if has_indicators {
        layout.body_height.saturating_sub(2)
    } else {
        layout.body_height
    };
    let offset = menu.scroll_offset();
    for (index, item) in items.iter().enumerate().skip(offset).take(visible_rows) {
        let Some(title) = item.strip_suffix(" ●") else {
            continue;
        };
        let column = layout.x + 1 + view::columns(title) + 1;
        if column + 1 >= layout.x + layout.menu_width {
            continue;
        }
        let mut marker = Text::from("●").color_all(1);
        if index == menu.selected_index() || menu.hovered_index() == Some(index) {
            marker = marker.selected();
        }
        print_text_with_coordinates(marker, column, first_line + index - offset, None, None);
    }
}

/// A help line with its keys in their own color.
fn colored_keys(line: &str, keys: Vec<std::ops::Range<usize>>) -> Text {
    keys.into_iter()
        .fold(Text::from(line), |text, range| text.color_range(3, range))
}

/// Draw ANSI at a column and line of the pane, from a clean style.
fn print_ansi(x: usize, y: usize, ansi: &str) {
    print!("\u{1b}[{};{}H\u{1b}[0m{ansi}", y + 1, x + 1);
}

fn is_plain(key: &KeyWithModifier, bare: BareKey) -> bool {
    key.bare_key == bare && key.key_modifiers.is_empty()
}

fn is_shift_tab(key: &KeyWithModifier) -> bool {
    key.bare_key == BareKey::Tab && key.has_modifiers(&[KeyModifier::Shift])
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_tile::prelude::{PluginAliasEntry, PluginEntry};

    /// A screen searching the options of an alias with no configuration.
    fn searching() -> SettingsScreen {
        let alias = vec![PluginAliasEntry {
            name: "zjhints".to_string(),
            plugin: PluginEntry {
                location: "file:~/zjhints.wasm".to_string(),
                cwd: None,
                configuration: Default::default(),
            },
        }];
        let mut editor = Editor::new(alias.clone(), alias, None).unwrap();
        editor.start_search("");
        SettingsScreen {
            editor: Some(Ok(editor)),
            search: Some(TextInput::new(String::new()).search_mode().focused()),
            ..Default::default()
        }
    }

    fn type_keys(screen: &mut SettingsScreen, keys: &str) {
        for character in keys.chars() {
            screen.search_key(&KeyWithModifier::new(BareKey::Char(character)));
        }
    }

    fn focus(screen: &SettingsScreen) -> Focus {
        match &screen.editor {
            Some(Ok(editor)) => editor.focus,
            _ => panic!("no editor"),
        }
    }

    #[test]
    fn typing_a_search_shows_the_matches_from_the_top() {
        let mut screen = searching();
        screen.shown_selected = Some((Section::Other, 0, true));
        type_keys(&mut screen, "mo");
        assert_eq!(screen.shown_selected, None);
    }

    #[test]
    fn a_search_with_no_matches_keeps_the_focus() {
        let mut screen = searching();
        type_keys(&mut screen, "no such option");
        for key in [BareKey::Down, BareKey::Tab, BareKey::Enter] {
            screen.search_key(&KeyWithModifier::new(key));
            assert_eq!(focus(&screen), Focus::Search, "{key:?}");
        }
        // With matches, the same keys move to the list.
        let mut screen = searching();
        type_keys(&mut screen, "mouse");
        screen.search_key(&KeyWithModifier::new(BareKey::Down));
        assert_eq!(focus(&screen), Focus::List);
    }
}
