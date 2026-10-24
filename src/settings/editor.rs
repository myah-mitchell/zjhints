//! What the settings screen is editing, and every edit it can make.
//!
//! Plain data and pure functions: the screen module feeds it the aliases read
//! from the config and the keys pressed, and carries out what it asks for
//! (previewing, saving) against the host.

use super::options::{
    self, Kind, OptionSpec, FAMILIES, GROUPS, OPTIONS, RESERVED_KEYS, SCREEN_OPTIONS,
};
use std::collections::BTreeMap;
use zellij_tile::prelude::{fuzzy_match_indices, PluginAliasEntry};

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
    /// The title of the section the search results under it come from. It
    /// cannot be selected.
    Heading(&'static str),
}

/// The heading over keys that fit no group.
pub const OTHER_HEADING: &str = "Other";

/// Which part of the screen the keys act on.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Focus {
    /// The sections down the left.
    Sidebar,
    /// The rows of the chosen section, or the search results.
    #[default]
    List,
    /// The search box, which takes the sidebar's place while searching.
    Search,
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
    /// Open the list of choices for the selected row (see `Editor::choices`).
    OpenChoices,
    /// The working values changed: redraw and preview them on the bars.
    Changed,
    /// The alias being edited changed: redraw and reset the preview.
    SwitchedAlias,
    /// Something could not be done; show `message`.
    Refused(String),
}

pub struct Editor {
    /// Every alias in the session, as last read. All of them are written
    /// back on save, because the `plugins` block replaces the aliases
    /// wholesale.
    aliases: Vec<PluginAliasEntry>,
    /// Every alias in the config file, as last read.
    saved_aliases: Vec<PluginAliasEntry>,
    /// Indexes into `aliases` of the ones that load zjhints.
    candidates: Vec<usize>,
    /// Index into `candidates` of the alias being edited.
    chosen: usize,
    /// The edited alias's configuration in the config file.
    saved: BTreeMap<String, String>,
    /// The edited alias's configuration as the session holds it, which is
    /// what bars started from it load. It runs ahead of `saved` when edits
    /// were left unsaved.
    session: BTreeMap<String, String>,
    /// The edited alias's configuration with the pending edits applied.
    working: BTreeMap<String, String>,
    /// The section the list shows.
    section: Section,
    /// What is being searched for, while searching. The list then shows the
    /// matching rows of every section instead of the chosen one.
    search: Option<String>,
    /// Index into `rows()` of the selected row.
    pub selected: usize,
    /// The part of the screen the keys act on.
    pub focus: Focus,
}

impl Editor {
    /// Edit the zjhints alias named `pinned`, or the first one when nothing
    /// is pinned, from the session's `aliases` and the config file's
    /// `saved_aliases`. An error, worded for the screen, when there is no
    /// zjhints alias to edit or the pinned one is not among them.
    pub fn new(
        aliases: Vec<PluginAliasEntry>,
        saved_aliases: Vec<PluginAliasEntry>,
        pinned: Option<&str>,
    ) -> Result<Self, String> {
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
            saved_aliases,
            candidates,
            chosen,
            saved: BTreeMap::new(),
            session: BTreeMap::new(),
            working: BTreeMap::new(),
            section: Section::Group(GROUPS[0]),
            search: None,
            selected: 0,
            focus: Focus::List,
        };
        editor.load_chosen();
        Ok(editor)
    }

    /// Take a fresh read of the aliases, after a save or a reload, keeping
    /// the same alias, section, search and row selected when they still
    /// exist.
    pub fn reread(
        &mut self,
        aliases: Vec<PluginAliasEntry>,
        saved_aliases: Vec<PluginAliasEntry>,
    ) -> Result<(), String> {
        let name = self.alias_name().to_string();
        let (section, search, selected, focus) =
            (self.section, self.search.take(), self.selected, self.focus);
        *self = Editor::new(aliases.clone(), saved_aliases.clone(), Some(&name))
            .or_else(|_| Editor::new(aliases, saved_aliases, None))?;
        if self.sections().contains(&section) {
            self.section = section;
            self.search = search;
            self.selected = selected;
            self.focus = focus;
        }
        self.clamp_selection();
        Ok(())
    }

    fn load_chosen(&mut self) {
        let alias = &self.aliases[self.candidates[self.chosen]];
        self.session = alias.plugin.configuration.iter().cloned().collect();
        // An alias the file does not have yet saves every key it holds.
        self.saved = self
            .saved_aliases
            .iter()
            .find(|saved| saved.name == alias.name)
            .map(|saved| saved.plugin.configuration.iter().cloned().collect())
            .unwrap_or_default();
        self.working = self.session.clone();
        self.clamp_selection();
    }

    pub fn alias_name(&self) -> &str {
        &self.aliases[self.candidates[self.chosen]].name
    }

    pub fn working(&self) -> &BTreeMap<String, String> {
        &self.working
    }

    /// Whether the pending values differ from the config file's.
    pub fn is_dirty(&self) -> bool {
        self.saved != self.working
    }

    /// How many keys have a pending value that differs from the file's.
    pub fn unsaved_count(&self) -> usize {
        let mut keys: Vec<&String> = self.saved.keys().chain(self.working.keys()).collect();
        keys.sort();
        keys.dedup();
        keys.into_iter().filter(|key| self.is_edited(key)).count()
    }

    /// Whether the session's alias holds something other than the pending
    /// values, so bars started from now on would not load them.
    pub fn session_differs(&self) -> bool {
        self.session != self.working
    }

    /// Record that the session's alias now holds the pending values.
    pub fn session_updated(&mut self) {
        self.session = self.working.clone();
    }

    /// Put every key back to the config file's value.
    pub fn revert_all(&mut self) {
        self.working = self.saved.clone();
        self.clamp_selection();
    }

    /// Every alias with the pending edits applied to the edited one, as the
    /// `plugins` block to replace the session's with.
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

    /// The lines of the list: the search results while searching, and the
    /// chosen section's rows otherwise.
    pub fn rows(&self) -> Vec<Row> {
        match &self.search {
            Some(query) => self.search_rows(query),
            None => self.rows_of(self.section),
        }
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

    /// The options and keys of every section that match `query`, each
    /// section's under a heading. An empty query matches them all.
    fn search_rows(&self, query: &str) -> Vec<Row> {
        let query = query.trim().to_lowercase();
        let matches = |row: &Row| {
            let (key, description) = match row {
                Row::Option(option) => (option.name, option.description),
                Row::Key(key) => (
                    key.as_str(),
                    options::family_of(key).map_or("", |family| family.description),
                ),
                _ => return false,
            };
            query.is_empty()
                || fuzzy_match_indices(&query, &key.to_lowercase()).is_some()
                || description.to_lowercase().contains(&query)
        };
        let mut rows = Vec::new();
        for section in self.sections() {
            let found: Vec<Row> = self
                .rows_of(section)
                .into_iter()
                .filter(|row| matches(row))
                .collect();
            if !found.is_empty() {
                rows.push(Row::Heading(section.title()));
                rows.extend(found);
            }
        }
        rows
    }

    /// What is being searched for, while searching.
    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }

    /// How many rows the search found, headings left out.
    pub fn match_count(&self) -> usize {
        self.rows()
            .iter()
            .filter(|row| !matches!(row, Row::Heading(_)))
            .count()
    }

    /// Show the rows matching `query` from every section, with the search
    /// box focused.
    pub fn start_search(&mut self, query: &str) -> Outcome {
        self.search = Some(query.to_string());
        self.focus = Focus::Search;
        self.selected = 0;
        self.clamp_selection();
        Outcome::Redraw
    }

    /// Go back to the chosen section's rows, with the sidebar focused.
    pub fn end_search(&mut self) -> Outcome {
        if self.search.take().is_none() {
            return Outcome::None;
        }
        self.focus = Focus::Sidebar;
        self.selected = 0;
        self.clamp_selection();
        Outcome::Redraw
    }

    /// The part of the screen to the left of the list: the search box while
    /// searching, the sidebar otherwise.
    pub fn menu_focus(&self) -> Focus {
        if self.search.is_some() {
            Focus::Search
        } else {
            Focus::Sidebar
        }
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
    /// In the list it passes over headings.
    pub fn move_selection(&mut self, delta: isize) -> Outcome {
        match self.focus {
            Focus::Sidebar => {
                let last = self.sections().len() - 1;
                let target = self.section_index().saturating_add_signed(delta).min(last);
                self.select_section(target)
            }
            Focus::List => {
                let selectable = self.selectable_rows();
                let Some(at) = selectable.iter().position(|&row| row == self.selected) else {
                    return Outcome::None;
                };
                let last = selectable.len().saturating_sub(1);
                let target = selectable[at.saturating_add_signed(delta).min(last)];
                if target == self.selected {
                    return Outcome::None;
                }
                self.selected = target;
                Outcome::Redraw
            }
            Focus::Search => Outcome::None,
        }
    }

    /// Indexes into `rows()` of the rows that can be selected.
    fn selectable_rows(&self) -> Vec<usize> {
        self.rows()
            .iter()
            .enumerate()
            .filter(|(_, row)| !matches!(row, Row::Heading(_)))
            .map(|(index, _)| index)
            .collect()
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

    /// Move the keys to `focus`.
    pub fn set_focus(&mut self, focus: Focus) -> Outcome {
        if self.focus == focus {
            return Outcome::None;
        }
        self.focus = focus;
        Outcome::Redraw
    }

    /// Select `row` of the list if it can be, as a click on it does.
    pub fn select(&mut self, row: usize) -> Outcome {
        if !self.selectable_rows().contains(&row) {
            return Outcome::None;
        }
        let focused = self.set_focus(Focus::List);
        if row != self.selected {
            self.selected = row;
            return Outcome::Redraw;
        }
        focused
    }

    /// Keep the selection on a row that exists and is not a heading, after
    /// rows went away.
    fn clamp_selection(&mut self) {
        if !self.sections().contains(&self.section) {
            self.section = Section::Group(GROUPS[0]);
        }
        let selectable = self.selectable_rows();
        self.selected = selectable
            .iter()
            .copied()
            .find(|&row| row >= self.selected)
            .or(selectable.last().copied())
            .unwrap_or(0);
    }

    /// Act on the selected row: toggle a boolean, open the choices of a
    /// choice, or start typing a value. In the sidebar, move over to the
    /// list.
    pub fn activate(&mut self) -> Outcome {
        match self.focus {
            Focus::Sidebar | Focus::Search => return self.set_focus(Focus::List),
            Focus::List => {}
        }
        match self.selected_row() {
            Some(Row::Alias) => Outcome::OpenChoices,
            Some(Row::Option(option)) => match option.kind {
                Kind::Bool => {
                    let on = self
                        .value(option.name)
                        .or(option.default)
                        .is_some_and(|value| value == "true");
                    self.set(option.name, Some((!on).to_string()))
                }
                Kind::Choice(_) => Outcome::OpenChoices,
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
            Some(Row::Heading(_)) | None => Outcome::None,
        }
    }

    /// The choices the selected row offers, and the index of the one in
    /// effect: the aliases to edit, or a choice option's values. A value
    /// the option does not list is offered too, last, so it shows as it is.
    pub fn choices(&self) -> Option<(Vec<String>, usize)> {
        self.choices_of(&self.selected_row()?)
    }

    /// The choices `row` offers, as `choices` gives them for the selected
    /// row.
    pub fn choices_of(&self, row: &Row) -> Option<(Vec<String>, usize)> {
        match row {
            Row::Alias => Some((
                self.candidates
                    .iter()
                    .map(|&index| self.aliases[index].name.clone())
                    .collect(),
                self.chosen,
            )),
            Row::Option(OptionSpec {
                name,
                kind: Kind::Choice(values),
                default,
                ..
            }) => {
                let mut values: Vec<String> =
                    values.iter().map(|value| value.to_string()).collect();
                let current = self.value(name).or(*default);
                let index = match current {
                    Some(current) => match values.iter().position(|value| value == current) {
                        Some(index) => index,
                        None => {
                            values.push(current.to_string());
                            values.len() - 1
                        }
                    },
                    None => 0,
                };
                Some((values, index))
            }
            _ => None,
        }
    }

    /// Take the choice at `index` in `choices()` for the selected row.
    pub fn choose(&mut self, index: usize) -> Outcome {
        let Some((values, current)) = self.choices() else {
            return Outcome::None;
        };
        if index == current || index >= values.len() {
            return Outcome::None;
        }
        match self.selected_row() {
            Some(Row::Alias) => self.switch_alias(index),
            Some(Row::Option(option)) => self.set(option.name, Some(values[index].clone())),
            _ => Outcome::None,
        }
    }

    /// Step the selected row's choice to the next or previous one, stopping
    /// at either end.
    pub fn step_choice(&mut self, forward: bool) -> Outcome {
        let Some((values, current)) = self.choices() else {
            return Outcome::None;
        };
        let target = if forward {
            (current + 1).min(values.len() - 1)
        } else {
            current.saturating_sub(1)
        };
        self.choose(target)
    }

    /// Put the selected row's key back to its default by removing it from the
    /// alias. For a key outside the fixed options this deletes it.
    pub fn reset_selected(&mut self) -> Outcome {
        if self.focus != Focus::List {
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

    /// Put the selected row's key back to the config file's value. The key
    /// reverted, or `None` when it had no unsaved change.
    pub fn revert_selected(&mut self) -> Option<String> {
        if self.focus != Focus::List {
            return None;
        }
        let key = match self.selected_row()? {
            Row::Option(option) => option.name.to_string(),
            Row::Key(key) => key,
            _ => return None,
        };
        if !self.is_edited(&key) {
            return None;
        }
        let saved = self.saved.get(&key).cloned();
        self.set(&key, saved);
        self.clamp_selection();
        Some(key)
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
                if crate::popup::INTERNAL_OPTIONS.contains(&key) {
                    return Outcome::Refused(format!("{key} is used by zjhints itself"));
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

    /// Show `section` with `row` selected and the list focused, leaving any
    /// search behind.
    fn show(&mut self, section: Section, row: usize) {
        self.search = None;
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
        // The search results keep their place; anywhere else the list moves
        // to where the key is listed, which a new key changes.
        match (&self.search, self.locate(key)) {
            (None, Some((section, row))) => self.show(section, row),
            _ => self.clamp_selection(),
        }
        Outcome::Changed
    }

    /// Edit the alias at `index` in `candidates`. Refused while there are
    /// unsaved edits, so they are never carried silently from one alias to
    /// another.
    fn switch_alias(&mut self, index: usize) -> Outcome {
        if self.is_dirty() {
            return Outcome::Refused("Save or revert the changes first".to_string());
        }
        self.chosen = index;
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

    fn aliases(configuration: &[(&str, &str)]) -> Vec<PluginAliasEntry> {
        vec![
            alias("tab-bar", "zellij:tab-bar", &[]),
            alias("zjhints", "file:~/zjhints.wasm", configuration),
        ]
    }

    /// An editor whose session holds `configuration`, saved as it is.
    fn editor(configuration: &[(&str, &str)]) -> Editor {
        Editor::new(aliases(configuration), aliases(configuration), None).unwrap()
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
        assert!(!editor.sections().contains(&Section::Alias));
    }

    #[test]
    fn without_a_zjhints_alias_there_is_nothing_to_edit() {
        let result = Editor::new(vec![alias("tab-bar", "zellij:tab-bar", &[])], vec![], None);
        assert!(result.is_err());
    }

    #[test]
    fn a_pinned_alias_is_edited_and_an_unknown_one_refused() {
        let aliases = vec![
            alias("zjhints", "file:a/zjhints.wasm", &[]),
            alias("zjhints-alt", "file:b/zjhints.wasm", &[]),
        ];
        let editor = Editor::new(aliases.clone(), aliases.clone(), Some("zjhints-alt")).unwrap();
        assert_eq!(editor.alias_name(), "zjhints-alt");
        assert_eq!(editor.sections()[0], Section::Alias);
        assert_eq!(editor.rows_of(Section::Alias), [Row::Alias]);
        assert!(Editor::new(aliases.clone(), aliases, Some("nope")).is_err());
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
        assert_eq!(editor.set_focus(Focus::Sidebar), Outcome::Redraw);
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
    fn selecting_the_section_shown_or_none_does_nothing() {
        let mut editor = editor(&[]);
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
            styling[4..],
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
            [
                key("mode_format_locked"),
                Row::Add("mode_format_"),
                Row::Add("mode_label_"),
                Row::Add("mode_key_"),
            ]
        );
        assert_eq!(editor.rows_of(Section::Other), [key("bogus"), Row::Add("")]);
        assert_eq!(editor.rows_of(Section::Group("Fitting")).len(), 6);
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
    fn a_choice_opens_its_values_and_steps_without_wrapping() {
        let mut editor = editor(&[("direction_keys", "arrows")]);
        select_key(&mut editor, "direction_keys");
        assert_eq!(editor.activate(), Outcome::OpenChoices);
        let (values, current) = editor.choices().unwrap();
        assert_eq!(values, ["both", "arrows", "letters"]);
        assert_eq!(current, 1);
        assert_eq!(editor.choose(1), Outcome::None);
        assert_eq!(editor.choose(2), Outcome::Changed);
        assert_eq!(editor.value("direction_keys"), Some("letters"));
        assert_eq!(editor.step_choice(true), Outcome::None);
        assert_eq!(editor.step_choice(false), Outcome::Changed);
        assert_eq!(editor.value("direction_keys"), Some("arrows"));
        assert_eq!(editor.choose(9), Outcome::None);
    }

    #[test]
    fn an_unset_choice_starts_at_its_default_and_an_unknown_value_is_offered() {
        let mut editor = editor(&[]);
        select_key(&mut editor, "direction_keys");
        assert_eq!(editor.choices().unwrap().1, 0);
        let mut editor = editor_with("direction_keys", "sideways");
        select_key(&mut editor, "direction_keys");
        let (values, current) = editor.choices().unwrap();
        assert_eq!(values.last().map(String::as_str), Some("sideways"));
        assert_eq!(current, values.len() - 1);
        select_key(&mut editor, "max_length");
        assert_eq!(editor.choices(), None);
    }

    fn editor_with(key: &str, value: &str) -> Editor {
        editor(&[(key, value)])
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
        // Any of them would turn the bar into a hover popup.
        for key in crate::popup::INTERNAL_OPTIONS {
            assert!(matches!(
                editor.submit(&Field::NewKey, key),
                Outcome::Refused(_)
            ));
        }
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

    /// An editor whose session holds `session` while the file holds
    /// `saved`.
    fn unsaved_editor(saved: &[(&str, &str)], session: &[(&str, &str)]) -> Editor {
        Editor::new(aliases(session), aliases(saved), None).unwrap()
    }

    #[test]
    fn edits_left_in_the_session_show_as_unsaved() {
        let editor = unsaved_editor(&[("max_length", "40")], &[("mouse", "true")]);
        assert_eq!(editor.value("mouse"), Some("true"));
        assert!(editor.is_dirty());
        assert!(!editor.session_differs());
        assert_eq!(editor.unsaved_count(), 2);
        assert!(editor.is_edited("max_length"));
        assert!(editor.section_is_edited(Section::Group("Mouse")));

        // An alias the file lacks counts every key as unsaved.
        let editor = Editor::new(aliases(&[("mouse", "true")]), vec![], None).unwrap();
        assert_eq!(editor.unsaved_count(), 1);
    }

    #[test]
    fn the_session_is_tracked_apart_from_the_file() {
        let mut editor = editor(&[]);
        select_key(&mut editor, "auto_width");
        editor.activate();
        assert!(editor.session_differs());
        editor.session_updated();
        assert!(!editor.session_differs());
        assert!(editor.is_dirty());
    }

    #[test]
    fn revert_puts_one_key_back_to_the_files_value() {
        let mut editor = unsaved_editor(
            &[("max_length", "40"), ("label_quit", "bye")],
            &[
                ("max_length", "60"),
                ("label_quit", "bye"),
                ("color_x", "#000000"),
            ],
        );
        select_key(&mut editor, "max_length");
        assert_eq!(editor.revert_selected().as_deref(), Some("max_length"));
        assert_eq!(editor.value("max_length"), Some("40"));
        assert_eq!(editor.revert_selected(), None);

        select_key(&mut editor, "label_quit");
        assert_eq!(editor.revert_selected(), None);

        // A key the file lacks goes away, and the selection stays in its group.
        select_key(&mut editor, "color_x");
        assert_eq!(editor.revert_selected().as_deref(), Some("color_x"));
        assert_eq!(editor.value("color_x"), None);
        assert_eq!(editor.selected_row(), Some(Row::Add("color_")));
        assert!(!editor.is_dirty());

        editor.set_focus(Focus::Sidebar);
        assert_eq!(editor.revert_selected(), None);
    }

    #[test]
    fn revert_all_restores_the_files_values() {
        let mut editor = unsaved_editor(&[("label_quit", "bye")], &[("mouse", "true")]);
        select_key(&mut editor, "mouse");
        editor.revert_all();
        assert!(!editor.is_dirty());
        assert_eq!(editor.value("label_quit"), Some("bye"));
        assert_eq!(editor.value("mouse"), None);
        assert!(editor.selected_row().is_some());
        assert!(editor.session_differs());
    }

    #[test]
    fn search_lists_matches_from_every_section_under_headings() {
        let mut editor = editor(&[("color_blue", "#0000ff")]);
        editor.start_search("blue");
        assert_eq!(editor.focus, Focus::Search);
        assert_eq!(editor.menu_focus(), Focus::Search);
        assert_eq!(
            editor.rows(),
            [Row::Heading("Colors"), Row::Key("color_blue".to_string())]
        );
        assert_eq!(editor.match_count(), 1);
        // The heading is passed over.
        assert_eq!(editor.selected, 1);

        // Names match fuzzily, descriptions by their words.
        editor.start_search("mxlen");
        assert!(editor
            .rows()
            .contains(&Row::Option(options::find("max_length").unwrap())));
        editor.start_search("hjkl");
        assert!(editor
            .rows()
            .contains(&Row::Option(options::find("direction_keys").unwrap())));

        editor.start_search("zzzzqqq");
        assert_eq!(editor.match_count(), 0);
        assert_eq!(editor.selected_row(), None);
        assert_eq!(editor.activate(), Outcome::Redraw);
        assert_eq!(editor.activate(), Outcome::None);

        assert_eq!(editor.end_search(), Outcome::Redraw);
        assert_eq!(editor.focus, Focus::Sidebar);
        assert_eq!(editor.search(), None);
        assert_eq!(editor.end_search(), Outcome::None);
    }

    #[test]
    fn moving_through_search_results_skips_headings() {
        let mut editor = editor(&[]);
        editor.start_search("");
        editor.set_focus(Focus::List);
        let rows = editor.rows();
        assert!(matches!(rows[0], Row::Heading(_)));
        assert_eq!(editor.selected, 1);
        assert_eq!(editor.move_selection(-1), Outcome::None);
        let heading = rows
            .iter()
            .skip(1)
            .position(|row| matches!(row, Row::Heading(_)))
            .unwrap()
            + 1;
        editor.selected = heading - 1;
        editor.move_selection(1);
        assert_eq!(editor.selected, heading + 1);
        assert_eq!(editor.select(heading), Outcome::None);
    }

    #[test]
    fn an_edit_from_the_search_results_keeps_them_shown() {
        let mut editor = editor(&[]);
        editor.start_search("auto_width");
        editor.set_focus(Focus::List);
        assert_eq!(editor.activate(), Outcome::Changed);
        assert_eq!(editor.search(), Some("auto_width"));
        assert_eq!(
            editor.selected_row(),
            Some(Row::Option(options::find("auto_width").unwrap()))
        );
        // Adding a key that exists leaves the search to show it.
        editor.submit(&Field::NewKey, "mouse");
        assert_eq!(editor.search(), None);
        assert_eq!(editor.section(), Section::Group("Mouse"));
    }

    #[test]
    fn the_alias_is_chosen_from_a_list_once_the_edits_are_saved_or_reverted() {
        let both = vec![
            alias("zjhints", "file:a/zjhints.wasm", &[]),
            alias("zjhints-alt", "file:b/zjhints.wasm", &[("mouse", "true")]),
        ];
        let mut editor = Editor::new(both.clone(), both, None).unwrap();
        select_key(&mut editor, "auto_width");
        editor.activate();
        editor.select_section(0);
        editor.set_focus(Focus::List);
        assert_eq!(editor.activate(), Outcome::OpenChoices);
        assert_eq!(
            editor.choices(),
            Some((vec!["zjhints".to_string(), "zjhints-alt".to_string()], 0))
        );
        assert!(matches!(editor.choose(1), Outcome::Refused(_)));
        editor.revert_all();
        assert_eq!(editor.choose(1), Outcome::SwitchedAlias);
        assert_eq!(editor.alias_name(), "zjhints-alt");
        assert_eq!(editor.value("mouse"), Some("true"));
        assert_eq!(editor.section(), Section::Alias);
    }
}
