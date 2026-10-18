//! What the settings screen is editing, and every edit it can make.
//!
//! Plain data and pure functions: the screen module feeds it the aliases read
//! from the config and the keys pressed, and carries out what it asks for
//! (previewing, saving) against the host.

use super::options::{
    self, Kind, OptionSpec, FAMILIES, GROUPS, OPTIONS, RESERVED_KEYS, SCREEN_OPTIONS,
};
use std::collections::BTreeMap;
use zellij_tile::prelude::PluginAliasEntry;

/// One entry of the sidebar, whose rows the list shows while it is chosen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Section {
    /// Which alias is being edited, listed when there is more than one.
    Alias,
    /// One of the option groups.
    Group(&'static str),
    /// Keys that fit no group.
    Other,
}

impl Section {
    pub fn title(self) -> &'static str {
        match self {
            Section::Alias => "Alias",
            Section::Group(group) => group,
            Section::Other => OTHER_HEADING,
        }
    }
}

/// One line of the list.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// Which alias is being edited.
    Alias,
    Option(&'static OptionSpec),
    /// A key outside the fixed options, such as `label_split_down`.
    Key(String),
    /// The line that adds a new key starting with this prefix: a family's,
    /// or nothing under "Other".
    Add(&'static str),
}

/// The heading over keys that fit no group.
pub const OTHER_HEADING: &str = "Other";

/// Which side of the screen the movement keys act on.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Focus {
    /// The sections down the left.
    Sidebar,
    /// The rows of the chosen section.
    #[default]
    List,
}

/// A value being typed.
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    /// A new value for `key`.
    Value { key: String },
    /// The name of a key to add.
    NewKey,
}

/// What the screen should do after an edit.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// Nothing visible changed.
    None,
    /// Redraw.
    Redraw,
    /// Start typing into `field`, starting from `text`.
    StartField { field: Field, text: String },
    /// The working values changed: redraw and preview them on the bars.
    Changed,
    /// The alias being edited changed: redraw and reset the preview.
    SwitchedAlias,
    /// Something could not be done; show `message`.
    Refused(String),
}

pub struct Editor {
    /// Every alias in the config, as last read. All of them are written back
    /// on save, because the `plugins` block replaces the aliases wholesale.
    aliases: Vec<PluginAliasEntry>,
    /// Indexes into `aliases` of the ones that load zjhints.
    candidates: Vec<usize>,
    /// Index into `candidates` of the alias being edited.
    chosen: usize,
    /// The edited alias's configuration as last saved.
    saved: BTreeMap<String, String>,
    /// The edited alias's configuration with the pending edits applied.
    working: BTreeMap<String, String>,
    /// The section the list shows.
    section: Section,
    /// Index into `rows()` of the selected row.
    pub selected: usize,
    /// The side the movement keys act on.
    pub focus: Focus,
}

impl Editor {
    /// Edit the zjhints alias named `pinned`, or the first one when nothing
    /// is pinned. An error, worded for the screen, when there is no zjhints
    /// alias to edit or the pinned one is not among them.
    pub fn new(aliases: Vec<PluginAliasEntry>, pinned: Option<&str>) -> Result<Self, String> {
        let candidates: Vec<usize> = aliases
            .iter()
            .enumerate()
            .filter(|(_, alias)| alias.plugin.location.contains("zjhints"))
            .map(|(index, _)| index)
            .collect();
        if candidates.is_empty() {
            return Err(
                "No alias in the plugins {} block loads zjhints, so there is nothing to edit."
                    .to_string(),
            );
        }
        let chosen = match pinned {
            Some(name) => candidates
                .iter()
                .position(|&index| aliases[index].name == name)
                .ok_or_else(|| format!("No zjhints alias is named \"{name}\"."))?,
            None => 0,
        };
        let mut editor = Editor {
            aliases,
            candidates,
            chosen,
            saved: BTreeMap::new(),
            working: BTreeMap::new(),
            section: Section::Group(GROUPS[0]),
            selected: 0,
            focus: Focus::List,
        };
        editor.load_chosen();
        Ok(editor)
    }

    /// Take a fresh read of the aliases, after a save or a reload, keeping
    /// the same alias, section and row selected when they still exist.
    pub fn reread(&mut self, aliases: Vec<PluginAliasEntry>) -> Result<(), String> {
        let name = self.alias_name().to_string();
        let (section, selected, focus) = (self.section, self.selected, self.focus);
        *self =
            Editor::new(aliases.clone(), Some(&name)).or_else(|_| Editor::new(aliases, None))?;
        if self.sections().contains(&section) {
            self.section = section;
            self.selected = selected;
            self.focus = focus;
        }
        self.clamp_selection();
        Ok(())
    }

    fn load_chosen(&mut self) {
        let alias = &self.aliases[self.candidates[self.chosen]];
        self.saved = alias.plugin.configuration.iter().cloned().collect();
        self.working = self.saved.clone();
        self.clamp_selection();
    }

    pub fn alias_name(&self) -> &str {
        &self.aliases[self.candidates[self.chosen]].name
    }

    pub fn alias_count(&self) -> usize {
        self.candidates.len()
    }

    pub fn working(&self) -> &BTreeMap<String, String> {
        &self.working
    }

    pub fn is_dirty(&self) -> bool {
        self.saved != self.working
    }

    /// Throw the pending edits away.
    pub fn discard(&mut self) {
        self.working = self.saved.clone();
        self.clamp_selection();
    }

    /// Every alias with the pending edits applied to the edited one, as the
    /// `plugins` block to replace the config's with.
    pub fn aliases_to_save(&self) -> Vec<PluginAliasEntry> {
        let mut aliases = self.aliases.clone();
        aliases[self.candidates[self.chosen]].plugin.configuration = self
            .working
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        aliases
    }

    /// The entries of the sidebar, in order.
    pub fn sections(&self) -> Vec<Section> {
        let alias = (self.candidates.len() > 1).then_some(Section::Alias);
        alias
            .into_iter()
            .chain(GROUPS.iter().map(|group| Section::Group(group)))
            .chain([Section::Other])
            .collect()
    }

    /// The section the list shows.
    pub fn section(&self) -> Section {
        self.section
    }

    /// Index into `sections()` of the section the list shows.
    pub fn section_index(&self) -> usize {
        self.sections()
            .iter()
            .position(|section| *section == self.section)
            .unwrap_or(0)
    }

    /// The section a key is listed in.
    fn section_of(key: &str) -> Section {
        options::group_of(key).map_or(Section::Other, Section::Group)
    }

    /// The lines of the list for the chosen section, in order.
    pub fn rows(&self) -> Vec<Row> {
        self.rows_of(self.section)
    }

    /// The lines of the list for `section`: its fixed options, then the
    /// keys set in it, then a line to add a key for each family prefix in
    /// it. "Other" holds the keys that fit nowhere else.
    pub fn rows_of(&self, section: Section) -> Vec<Row> {
        let group = match section {
            Section::Alias => return vec![Row::Alias],
            Section::Group(group) => Some(group),
            Section::Other => None,
        };
        let mut rows: Vec<Row> = OPTIONS
            .iter()
            .filter(|option| Some(option.group) == group)
            .map(Row::Option)
            .collect();
        rows.extend(
            self.working
                .keys()
                .filter(|key| options::find(key).is_none())
                .filter(|key| !SCREEN_OPTIONS.contains(&key.as_str()))
                .filter(|key| Editor::section_of(key) == section)
                .map(|key| Row::Key(key.clone())),
        );
        match group {
            Some(group) => {
                let mut prefixes: Vec<&'static str> = Vec::new();
                for family in FAMILIES.iter().filter(|family| family.group == group) {
                    if !prefixes.contains(&family.prefix()) {
                        prefixes.push(family.prefix());
                    }
                }
                rows.extend(prefixes.into_iter().map(Row::Add));
            }
            None => rows.push(Row::Add("")),
        }
        rows
    }

    pub fn selected_row(&self) -> Option<Row> {
        self.rows().into_iter().nth(self.selected)
    }

    /// Where `key` is listed: its section, and the index of its row there.
    pub fn locate(&self, key: &str) -> Option<(Section, usize)> {
        let section = Editor::section_of(key);
        let row = self.rows_of(section).iter().position(|row| match row {
            Row::Option(option) => option.name == key,
            Row::Key(other) => other == key,
            _ => false,
        })?;
        Some((section, row))
    }

    /// Whether any key in `section` has a pending value that differs from
    /// the saved one, a key removed since the save included.
    pub fn section_is_edited(&self, section: Section) -> bool {
        section != Section::Alias
            && self
                .saved
                .keys()
                .chain(self.working.keys())
                .any(|key| Editor::section_of(key) == section && self.is_edited(key))
    }

    /// The value a row's key has: set in the alias, or `None` when unset.
    pub fn value(&self, key: &str) -> Option<&str> {
        self.working.get(key).map(String::as_str)
    }

    /// Whether a key's pending value differs from the saved one.
    pub fn is_edited(&self, key: &str) -> bool {
        self.working.get(key) != self.saved.get(key)
    }

    /// Move the selection by `delta` on the focused side, stopping at either
    /// end. In the sidebar this shows another section, from its first row.
    pub fn move_selection(&mut self, delta: isize) -> Outcome {
        match self.focus {
            Focus::Sidebar => {
                let last = self.sections().len() - 1;
                let target = self.section_index().saturating_add_signed(delta).min(last);
                self.select_section(target)
            }
            Focus::List => {
                let last = self.rows().len().saturating_sub(1);
                let target = self.selected.saturating_add_signed(delta).min(last);
                if target == self.selected {
                    return Outcome::None;
                }
                self.selected = target;
                Outcome::Redraw
            }
        }
    }

    /// Show the section at `index` in `sections()`, from its first row.
    pub fn select_section(&mut self, index: usize) -> Outcome {
        match self.sections().get(index) {
            Some(&section) if section != self.section => {
                self.section = section;
                self.selected = 0;
                Outcome::Redraw
            }
            _ => Outcome::None,
        }
    }

    /// Show the section `delta` along from the chosen one, wrapping around
    /// at either end, for when there is no room for the sidebar.
    pub fn cycle_section(&mut self, delta: isize) -> Outcome {
        let count = self.sections().len() as isize;
        let target = (self.section_index() as isize + delta).rem_euclid(count);
        self.select_section(target as usize)
    }

    /// Move the movement keys to `focus`.
    pub fn set_focus(&mut self, focus: Focus) -> Outcome {
        if self.focus == focus {
            return Outcome::None;
        }
        self.focus = focus;
        Outcome::Redraw
    }

    /// Move the movement keys to the other side.
    pub fn toggle_focus(&mut self) -> Outcome {
        self.set_focus(match self.focus {
            Focus::Sidebar => Focus::List,
            Focus::List => Focus::Sidebar,
        })
    }

    /// Select `row` of the list if there is one, as a click on it does.
    pub fn select(&mut self, row: usize) -> Outcome {
        let focused = self.set_focus(Focus::List);
        if row < self.rows().len() && row != self.selected {
            self.selected = row;
            return Outcome::Redraw;
        }
        focused
    }

    /// Keep the selection on a row that exists, after rows went away.
    fn clamp_selection(&mut self) {
        if !self.sections().contains(&self.section) {
            self.section = Section::Group(GROUPS[0]);
        }
        self.selected = self.selected.min(self.rows().len().saturating_sub(1));
    }

    /// Act on the selected row: toggle a boolean, step through a choice, or
    /// start typing a value. In the sidebar, move over to the list.
    pub fn activate(&mut self) -> Outcome {
        if self.focus == Focus::Sidebar {
            return self.set_focus(Focus::List);
        }
        match self.selected_row() {
            Some(Row::Alias) => self.next_alias(),
            Some(Row::Option(option)) => match option.kind {
                Kind::Bool => {
                    let on = self
                        .value(option.name)
                        .or(option.default)
                        .is_some_and(|value| value == "true");
                    self.set(option.name, Some((!on).to_string()))
                }
                Kind::Choice(choices) => {
                    let current = self.value(option.name).or(option.default);
                    let next = current
                        .and_then(|value| choices.iter().position(|choice| *choice == value))
                        .map_or(0, |index| (index + 1) % choices.len());
                    self.set(option.name, Some(choices[next].to_string()))
                }
                Kind::Integer | Kind::Fraction | Kind::Text | Kind::Format => Outcome::StartField {
                    field: Field::Value {
                        key: option.name.to_string(),
                    },
                    text: self.value(option.name).unwrap_or_default().to_string(),
                },
            },
            Some(Row::Key(key)) => Outcome::StartField {
                text: self.value(&key).unwrap_or_default().to_string(),
                field: Field::Value { key },
            },
            Some(Row::Add(prefix)) => Outcome::StartField {
                field: Field::NewKey,
                text: prefix.to_string(),
            },
            None => Outcome::None,
        }
    }

    /// Put the selected row's key back to its default by removing it from the
    /// alias. For a key outside the fixed options this deletes it.
    pub fn reset_selected(&mut self) -> Outcome {
        if self.focus == Focus::Sidebar {
            return Outcome::None;
        }
        match self.selected_row() {
            Some(Row::Option(option)) => self.set(option.name, None),
            Some(Row::Key(key)) => {
                let outcome = self.set(&key, None);
                self.clamp_selection();
                outcome
            }
            _ => Outcome::None,
        }
    }

    /// Finish typing `text` into `field`.
    pub fn submit(&mut self, field: &Field, text: &str) -> Outcome {
        match field {
            Field::Value { key } => {
                let kind = options::kind_of(key);
                if let Err(expected) = options::validate(kind, text) {
                    return Outcome::Refused(format!("{key} must be {expected}"));
                }
                // An empty number or choice means "unset". An empty string
                // stays, because an empty label hides its hint.
                let value = (kind.is_text() || !text.is_empty()).then(|| text.to_string());
                self.set(key, value)
            }
            Field::NewKey => {
                let key = text.trim();
                if key.is_empty()
                    || !key
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric() || character == '_')
                {
                    return Outcome::Refused(
                        "A key is letters, digits and underscores".to_string(),
                    );
                }
                if let Some(family) = FAMILIES.iter().find(|family| family.prefix() == key) {
                    return Outcome::Refused(format!("Finish the key, as in {}", family.pattern));
                }
                if let Some((section, row)) = self.locate(key) {
                    self.show(section, row);
                    return Outcome::Redraw;
                }
                if SCREEN_OPTIONS.contains(&key) {
                    return Outcome::Refused(format!("{key} is read only by the settings screen"));
                }
                if RESERVED_KEYS.contains(&key) {
                    return Outcome::Refused(format!("{key} is reserved by Zellij"));
                }
                Outcome::StartField {
                    field: Field::Value {
                        key: key.to_string(),
                    },
                    text: String::new(),
                }
            }
        }
    }

    /// Show `section` with `row` selected and the list focused.
    fn show(&mut self, section: Section, row: usize) {
        self.section = section;
        self.selected = row;
        self.focus = Focus::List;
    }

    fn set(&mut self, key: &str, value: Option<String>) -> Outcome {
        let previous = match value {
            Some(value) => self.working.insert(key.to_string(), value),
            None => self.working.remove(key),
        };
        if previous.as_deref() == self.value(key) {
            return Outcome::None;
        }
        if let Some((section, row)) = self.locate(key) {
            self.show(section, row);
        }
        Outcome::Changed
    }

    /// Edit the next zjhints alias. Refused while there are pending edits, so
    /// they are never carried silently from one alias to another.
    fn next_alias(&mut self) -> Outcome {
        if self.is_dirty() {
            return Outcome::Refused("Save or discard the edits first".to_string());
        }
        self.chosen = (self.chosen + 1) % self.candidates.len();
        self.load_chosen();
        Outcome::SwitchedAlias
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_tile::prelude::PluginEntry;

    fn alias(name: &str, location: &str, configuration: &[(&str, &str)]) -> PluginAliasEntry {
        PluginAliasEntry {
            name: name.to_string(),
            plugin: PluginEntry {
                location: location.to_string(),
                cwd: None,
                configuration: configuration
                    .iter()
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect(),
            },
        }
    }

    fn editor(configuration: &[(&str, &str)]) -> Editor {
        Editor::new(
            vec![
                alias("tab-bar", "zellij:tab-bar", &[]),
                alias("zjhints", "file:~/zjhints.wasm", configuration),
            ],
            None,
        )
        .unwrap()
    }

    fn select_key(editor: &mut Editor, key: &str) {
        let (section, row) = editor.locate(key).unwrap();
        editor.show(section, row);
    }

    fn show_section(editor: &mut Editor, section: Section) {
        let index = editor.sections().iter().position(|s| *s == section);
        editor.select_section(index.unwrap());
    }

    #[test]
    fn only_aliases_loading_zjhints_are_edited() {
        let editor = editor(&[]);
        assert_eq!(editor.alias_name(), "zjhints");
        assert_eq!(editor.alias_count(), 1);
        assert!(!editor.sections().contains(&Section::Alias));
    }

    #[test]
    fn without_a_zjhints_alias_there_is_nothing_to_edit() {
        let result = Editor::new(vec![alias("tab-bar", "zellij:tab-bar", &[])], None);
        assert!(result.is_err());
    }

    #[test]
    fn a_pinned_alias_is_edited_and_an_unknown_one_refused() {
        let aliases = vec![
            alias("zjhints", "file:a/zjhints.wasm", &[]),
            alias("zjhints-alt", "file:b/zjhints.wasm", &[]),
        ];
        let editor = Editor::new(aliases.clone(), Some("zjhints-alt")).unwrap();
        assert_eq!(editor.alias_name(), "zjhints-alt");
        assert_eq!(editor.sections()[0], Section::Alias);
        assert_eq!(editor.rows_of(Section::Alias), [Row::Alias]);
        assert!(Editor::new(aliases, Some("nope")).is_err());
    }

    #[test]
    fn the_first_group_and_option_are_shown_first() {
        let editor = editor(&[]);
        assert_eq!(editor.section(), Section::Group("Fitting"));
        assert_eq!(editor.focus, Focus::List);
        assert_eq!(
            editor.selected_row(),
            Some(Row::Option(options::find("auto_width").unwrap()))
        );
    }

    #[test]
    fn every_group_is_in_the_sidebar_with_other_last() {
        let sections = editor(&[]).sections();
        assert_eq!(sections.len(), GROUPS.len() + 1);
        assert_eq!(sections.last(), Some(&Section::Other));
        assert!(sections.contains(&Section::Group("Colors")));
    }

    #[test]
    fn moving_in_the_list_stops_at_the_ends_of_the_group() {
        let mut editor = editor(&[]);
        assert_eq!(editor.move_selection(-1), Outcome::None);
        editor.move_selection(1000);
        assert_eq!(
            editor.selected_row(),
            Some(Row::Option(options::find("hint_precedence").unwrap()))
        );
        assert_eq!(editor.move_selection(1), Outcome::None);
        assert_eq!(editor.section(), Section::Group("Fitting"));
    }

    #[test]
    fn moving_in_the_sidebar_shows_each_group_from_its_first_row() {
        let mut editor = editor(&[]);
        editor.move_selection(2);
        assert_eq!(editor.toggle_focus(), Outcome::Redraw);
        assert_eq!(editor.focus, Focus::Sidebar);
        assert_eq!(editor.move_selection(1), Outcome::Redraw);
        assert_eq!(editor.section(), Section::Group("Hints"));
        assert_eq!(editor.selected, 0);
        assert_eq!(editor.move_selection(-100), Outcome::Redraw);
        assert_eq!(editor.section(), Section::Group("Fitting"));
        assert_eq!(editor.move_selection(-1), Outcome::None);
        editor.move_selection(100);
        assert_eq!(editor.section(), Section::Other);

        // Enter in the sidebar moves over to the list rather than editing.
        assert_eq!(editor.activate(), Outcome::Redraw);
        assert_eq!(editor.focus, Focus::List);
        assert_eq!(editor.set_focus(Focus::List), Outcome::None);
    }

    #[test]
    fn cycling_sections_wraps_around() {
        let mut editor = editor(&[]);
        editor.cycle_section(-1);
        assert_eq!(editor.section(), Section::Other);
        editor.cycle_section(1);
        assert_eq!(editor.section(), Section::Group("Fitting"));
        assert_eq!(editor.select_section(0), Outcome::None);
        assert_eq!(editor.select_section(99), Outcome::None);
    }

    #[test]
    fn a_click_in_the_list_focuses_it() {
        let mut editor = editor(&[]);
        editor.set_focus(Focus::Sidebar);
        assert_eq!(editor.select(0), Outcome::Redraw);
        assert_eq!(editor.focus, Focus::List);
        assert_eq!(editor.select(0), Outcome::None);
        assert_eq!(editor.select(99), Outcome::None);
    }

    #[test]
    fn family_keys_are_listed_in_their_groups_with_an_add_line_each() {
        let editor = editor(&[
            ("color_blue", "#0000ff"),
            ("label_quit", "bye"),
            ("label_locked_quit", "bye"),
            ("mod_alias_ctrl", "^"),
            ("chord_alias_leader", "&"),
            ("key_format_quit", "{key}"),
            ("mode_format_locked", "{mode}"),
            ("bogus", "x"),
            ("settings", "true"),
        ]);
        let key = |key: &str| Row::Key(key.to_string());
        assert_eq!(
            editor.rows_of(Section::Group("Colors")),
            [key("color_blue"), Row::Add("color_")]
        );
        assert_eq!(
            editor.rows_of(Section::Group("Labels")),
            [
                key("label_locked_quit"),
                key("label_quit"),
                Row::Add("label_")
            ]
        );
        assert_eq!(
            editor.rows_of(Section::Group("Modifier aliases")),
            [
                key("chord_alias_leader"),
                key("mod_alias_ctrl"),
                Row::Add("mod_alias_"),
                Row::Add("chord_mods_"),
                Row::Add("chord_alias_"),
            ]
        );
        let styling = editor.rows_of(Section::Group("Styling"));
        assert_eq!(
            styling[3..],
            [
                key("key_format_quit"),
                Row::Add("key_format_"),
                Row::Add("desc_format_"),
                Row::Add("keys_"),
            ]
        );
        let mode = editor.rows_of(Section::Group("Mode prefix"));
        assert_eq!(
            mode[2..],
            [key("mode_format_locked"), Row::Add("mode_format_")]
        );
        assert_eq!(editor.rows_of(Section::Other), [key("bogus"), Row::Add("")]);
        assert_eq!(editor.rows_of(Section::Group("Fitting")).len(), 7);
    }

    #[test]
    fn a_boolean_toggles_from_its_default() {
        let mut editor = editor(&[]);
        select_key(&mut editor, "auto_width");
        assert_eq!(editor.activate(), Outcome::Changed);
        assert_eq!(editor.value("auto_width"), Some("false"));
        assert!(editor.is_dirty());
        editor.activate();
        assert_eq!(editor.value("auto_width"), Some("true"));
    }

    #[test]
    fn a_choice_steps_through_its_values_and_wraps() {
        let mut editor = editor(&[("direction_keys", "letters")]);
        select_key(&mut editor, "direction_keys");
        editor.activate();
        assert_eq!(editor.value("direction_keys"), Some("both"));
        editor.activate();
        assert_eq!(editor.value("direction_keys"), Some("arrows"));
    }

    #[test]
    fn a_number_is_typed_and_checked() {
        let mut editor = editor(&[("max_length", "40")]);
        select_key(&mut editor, "max_length");
        let Outcome::StartField { field, text } = editor.activate() else {
            panic!("expected a field");
        };
        assert_eq!(text, "40");
        assert!(matches!(editor.submit(&field, "x"), Outcome::Refused(_)));
        assert_eq!(editor.submit(&field, "60"), Outcome::Changed);
        assert_eq!(editor.value("max_length"), Some("60"));
        assert_eq!(editor.submit(&field, ""), Outcome::Changed);
        assert_eq!(editor.value("max_length"), None);
    }

    #[test]
    fn an_empty_label_or_format_is_kept() {
        let mut editor = editor(&[("label_quit", "bye"), ("key_format", "{key}")]);
        for key in ["label_quit", "key_format"] {
            let field = Field::Value {
                key: key.to_string(),
            };
            editor.submit(&field, "");
            assert_eq!(editor.value(key), Some(""));
        }
    }

    #[test]
    fn reset_removes_the_key_from_the_alias() {
        let mut editor = editor(&[("show_mode", "true")]);
        select_key(&mut editor, "show_mode");
        assert_eq!(editor.reset_selected(), Outcome::Changed);
        assert_eq!(editor.value("show_mode"), None);
        assert_eq!(editor.reset_selected(), Outcome::None);
        editor.set_focus(Focus::Sidebar);
        assert_eq!(editor.reset_selected(), Outcome::None);
    }

    #[test]
    fn keys_are_added_from_their_group_and_deleted() {
        let mut editor = editor(&[]);
        show_section(&mut editor, Section::Group("Colors"));
        assert_eq!(editor.selected_row(), Some(Row::Add("color_")));
        let Outcome::StartField { field, text } = editor.activate() else {
            panic!("expected a field");
        };
        assert_eq!((&field, text.as_str()), (&Field::NewKey, "color_"));
        assert!(matches!(
            editor.submit(&field, "color_"),
            Outcome::Refused(_)
        ));
        assert!(matches!(
            editor.submit(&field, "bad key"),
            Outcome::Refused(_)
        ));
        let Outcome::StartField { field, .. } = editor.submit(&field, "color_red") else {
            panic!("expected a value field");
        };
        editor.submit(&field, "#ff0000");
        assert_eq!(editor.value("color_red"), Some("#ff0000"));
        assert_eq!(
            editor.selected_row(),
            Some(Row::Key("color_red".to_string()))
        );
        assert!(editor.section_is_edited(Section::Group("Colors")));
        assert!(!editor.section_is_edited(Section::Group("Labels")));

        assert_eq!(editor.reset_selected(), Outcome::Changed);
        assert_eq!(editor.value("color_red"), None);
        assert_eq!(editor.selected_row(), Some(Row::Add("color_")));
    }

    #[test]
    fn a_key_added_under_other_moves_to_its_group() {
        let mut editor = editor(&[]);
        show_section(&mut editor, Section::Other);
        let Outcome::StartField { field, text } = editor.activate() else {
            panic!("expected a field");
        };
        assert_eq!(text, "");
        let Outcome::StartField { field, .. } = editor.submit(&field, "keys_quit") else {
            panic!("expected a value field");
        };
        editor.submit(&field, "q");
        assert_eq!(editor.section(), Section::Group("Styling"));
        assert_eq!(
            editor.selected_row(),
            Some(Row::Key("keys_quit".to_string()))
        );
    }

    #[test]
    fn keys_zellij_reserves_cannot_be_added() {
        let mut editor = editor(&[]);
        assert!(matches!(
            editor.submit(&Field::NewKey, "cwd"),
            Outcome::Refused(_)
        ));
        assert!(matches!(
            editor.submit(&Field::NewKey, "settings"),
            Outcome::Refused(_)
        ));
    }

    #[test]
    fn adding_a_key_that_exists_shows_it() {
        let mut editor = editor(&[]);
        assert_eq!(editor.submit(&Field::NewKey, "mouse"), Outcome::Redraw);
        assert_eq!(editor.section(), Section::Group("Mouse"));
        assert_eq!(
            editor.selected_row(),
            Some(Row::Option(options::find("mouse").unwrap()))
        );
    }

    #[test]
    fn saving_writes_every_alias_with_only_the_edited_one_changed() {
        let mut editor = editor(&[("show_mode", "true")]);
        select_key(&mut editor, "mouse");
        editor.activate();
        let aliases = editor.aliases_to_save();
        assert_eq!(aliases[0], alias("tab-bar", "zellij:tab-bar", &[]));
        assert_eq!(
            aliases[1],
            alias(
                "zjhints",
                "file:~/zjhints.wasm",
                &[("mouse", "true"), ("show_mode", "true")]
            )
        );
    }

    #[test]
    fn discard_restores_the_saved_values() {
        let mut editor = editor(&[("label_quit", "bye")]);
        select_key(&mut editor, "label_quit");
        editor.reset_selected();
        assert!(editor.section_is_edited(Section::Group("Labels")));
        editor.discard();
        assert!(!editor.is_dirty());
        assert!(editor.selected_row().is_some());
    }

    #[test]
    fn switching_alias_waits_until_the_edits_are_saved_or_discarded() {
        let mut editor = Editor::new(
            vec![
                alias("zjhints", "file:a/zjhints.wasm", &[]),
                alias("zjhints-alt", "file:b/zjhints.wasm", &[("mouse", "true")]),
            ],
            None,
        )
        .unwrap();
        select_key(&mut editor, "auto_width");
        editor.activate();
        editor.select_section(0);
        assert!(matches!(editor.activate(), Outcome::Refused(_)));
        editor.discard();
        assert_eq!(editor.activate(), Outcome::SwitchedAlias);
        assert_eq!(editor.alias_name(), "zjhints-alt");
        assert_eq!(editor.value("mouse"), Some("true"));
        assert_eq!(editor.section(), Section::Alias);
    }
}
