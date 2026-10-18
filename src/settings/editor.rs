//! What the settings screen is editing, and every edit it can make.
//!
//! Plain data and pure functions: the screen module feeds it the aliases read
//! from the config and the keys pressed, and carries out what it asks for
//! (previewing, saving) against the host.

use super::options::{self, Kind, OptionSpec, GROUPS, OPTIONS, RESERVED_KEYS, SCREEN_OPTIONS};
use std::collections::BTreeMap;
use zellij_tile::prelude::PluginAliasEntry;

/// One line of the list.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// Which alias is being edited, shown when there is more than one.
    Alias,
    Heading(&'static str),
    Option(&'static OptionSpec),
    /// A key outside the fixed options, such as `label_split_down`.
    Other(String),
    /// The line that adds a new key to "Other".
    AddOther,
}

impl Row {
    pub fn is_selectable(&self) -> bool {
        !matches!(self, Row::Heading(_))
    }
}

/// The heading over keys that are not fixed options.
pub const OTHER_HEADING: &str = "Other";

/// A value being typed.
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    /// A new value for `key`.
    Value { key: String },
    /// The name of a key to add under "Other".
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
    /// Index into `rows()` of the selected row.
    pub selected: usize,
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
            selected: 0,
        };
        editor.load_chosen();
        Ok(editor)
    }

    /// Take a fresh read of the aliases, after a save or a reload, keeping
    /// the same alias selected when it still exists.
    pub fn reread(&mut self, aliases: Vec<PluginAliasEntry>) -> Result<(), String> {
        let name = self.alias_name().to_string();
        let selected = self.selected;
        *self =
            Editor::new(aliases.clone(), Some(&name)).or_else(|_| Editor::new(aliases, None))?;
        self.selected = selected.min(self.rows().len().saturating_sub(1));
        self.select_nearest(1);
        Ok(())
    }

    fn load_chosen(&mut self) {
        let alias = &self.aliases[self.candidates[self.chosen]];
        self.saved = alias.plugin.configuration.iter().cloned().collect();
        self.working = self.saved.clone();
        self.selected = 0;
        self.select_nearest(1);
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
        let last = self.rows().len().saturating_sub(1);
        self.selected = self.selected.min(last);
        self.select_nearest(-1);
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

    /// The lines of the list, in order.
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        if self.candidates.len() > 1 {
            rows.push(Row::Alias);
        }
        for group in GROUPS {
            rows.push(Row::Heading(group));
            rows.extend(
                OPTIONS
                    .iter()
                    .filter(|option| option.group == *group)
                    .map(Row::Option),
            );
        }
        rows.push(Row::Heading(OTHER_HEADING));
        rows.extend(
            self.working
                .keys()
                .filter(|key| options::find(key).is_none())
                .filter(|key| !SCREEN_OPTIONS.contains(&key.as_str()))
                .map(|key| Row::Other(key.clone())),
        );
        rows.push(Row::AddOther);
        rows
    }

    pub fn selected_row(&self) -> Option<Row> {
        self.rows().into_iter().nth(self.selected)
    }

    /// The value a row's key has: set in the alias, or `None` when unset.
    pub fn value(&self, key: &str) -> Option<&str> {
        self.working.get(key).map(String::as_str)
    }

    /// Whether a key's pending value differs from the saved one.
    pub fn is_edited(&self, key: &str) -> bool {
        self.working.get(key) != self.saved.get(key)
    }

    /// Move the selection by `delta` selectable rows, stopping at either end.
    pub fn move_selection(&mut self, delta: isize) -> Outcome {
        let rows = self.rows();
        let selectable: Vec<usize> = (0..rows.len())
            .filter(|&index| rows[index].is_selectable())
            .collect();
        let Some(current) = selectable.iter().position(|&index| index == self.selected) else {
            return Outcome::None;
        };
        let target = current
            .saturating_add_signed(delta)
            .min(selectable.len() - 1);
        if selectable[target] == self.selected {
            return Outcome::None;
        }
        self.selected = selectable[target];
        Outcome::Redraw
    }

    /// Select `row` if it can be selected, as a click on it does.
    pub fn select(&mut self, row: usize) -> Outcome {
        match self.rows().get(row) {
            Some(found) if found.is_selectable() && row != self.selected => {
                self.selected = row;
                Outcome::Redraw
            }
            _ => Outcome::None,
        }
    }

    /// Step off a heading onto the nearest selectable row, looking in
    /// `direction` first.
    fn select_nearest(&mut self, direction: isize) {
        let rows = self.rows();
        if rows.get(self.selected).is_some_and(Row::is_selectable) {
            return;
        }
        let ahead = |from: usize| {
            if direction >= 0 {
                (from..rows.len()).find(|&index| rows[index].is_selectable())
            } else {
                (0..=from).rev().find(|&index| rows[index].is_selectable())
            }
        };
        let behind = |from: usize| {
            if direction >= 0 {
                (0..=from).rev().find(|&index| rows[index].is_selectable())
            } else {
                (from..rows.len()).find(|&index| rows[index].is_selectable())
            }
        };
        let from = self.selected.min(rows.len().saturating_sub(1));
        if let Some(index) = ahead(from).or_else(|| behind(from)) {
            self.selected = index;
        }
    }

    /// Act on the selected row: toggle a boolean, step through a choice, or
    /// start typing a value.
    pub fn activate(&mut self) -> Outcome {
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
                Kind::Integer | Kind::Fraction | Kind::Text => Outcome::StartField {
                    field: Field::Value {
                        key: option.name.to_string(),
                    },
                    text: self.value(option.name).unwrap_or_default().to_string(),
                },
            },
            Some(Row::Other(key)) => Outcome::StartField {
                text: self.value(&key).unwrap_or_default().to_string(),
                field: Field::Value { key },
            },
            Some(Row::AddOther) => Outcome::StartField {
                field: Field::NewKey,
                text: String::new(),
            },
            Some(Row::Heading(_)) | None => Outcome::None,
        }
    }

    /// Put the selected row's key back to its default by removing it from the
    /// alias. For a key under "Other" this deletes it.
    pub fn reset_selected(&mut self) -> Outcome {
        match self.selected_row() {
            Some(Row::Option(option)) => self.set(option.name, None),
            Some(Row::Other(key)) => {
                let outcome = self.set(&key, None);
                self.select_nearest(1);
                outcome
            }
            _ => Outcome::None,
        }
    }

    /// Finish typing `text` into `field`.
    pub fn submit(&mut self, field: &Field, text: &str) -> Outcome {
        match field {
            Field::Value { key } => {
                let kind = options::find(key).map_or(Kind::Text, |option| option.kind);
                if let Err(expected) = options::validate(kind, text) {
                    return Outcome::Refused(format!("{key} must be {expected}"));
                }
                // An empty number or choice means "unset". An empty string
                // stays, because an empty label hides its hint.
                let value = (kind == Kind::Text || !text.is_empty()).then(|| text.to_string());
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
                if let Some(row) = self.rows().iter().position(|row| match row {
                    Row::Option(option) => option.name == key,
                    Row::Other(other) => other == key,
                    _ => false,
                }) {
                    self.selected = row;
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

    fn set(&mut self, key: &str, value: Option<String>) -> Outcome {
        let previous = match value {
            Some(value) => self.working.insert(key.to_string(), value),
            None => self.working.remove(key),
        };
        if previous.as_deref() == self.value(key) {
            return Outcome::None;
        }
        if let Some(row) = self.rows().iter().position(|row| match row {
            Row::Option(option) => option.name == key,
            Row::Other(other) => other == key,
            _ => false,
        }) {
            self.selected = row;
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

    fn select_option(editor: &mut Editor, name: &str) {
        editor.selected = editor
            .rows()
            .iter()
            .position(|row| matches!(row, Row::Option(option) if option.name == name))
            .unwrap();
    }

    #[test]
    fn only_aliases_loading_zjhints_are_edited() {
        let editor = editor(&[]);
        assert_eq!(editor.alias_name(), "zjhints");
        assert_eq!(editor.alias_count(), 1);
        assert!(!editor.rows().contains(&Row::Alias));
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
        assert!(editor.rows().contains(&Row::Alias));
        assert!(Editor::new(aliases, Some("nope")).is_err());
    }

    #[test]
    fn the_first_row_selected_is_an_option_not_a_heading() {
        let editor = editor(&[]);
        assert_eq!(
            editor.selected_row(),
            Some(Row::Option(options::find("auto_width").unwrap()))
        );
    }

    #[test]
    fn moving_skips_headings_and_stops_at_the_ends() {
        let mut editor = editor(&[]);
        assert_eq!(editor.move_selection(-1), Outcome::None);
        select_option(&mut editor, "hint_precedence");
        editor.move_selection(1);
        assert_eq!(
            editor.selected_row(),
            Some(Row::Option(options::find("discover_hints").unwrap()))
        );
        editor.move_selection(1000);
        assert_eq!(editor.selected_row(), Some(Row::AddOther));
    }

    #[test]
    fn a_boolean_toggles_from_its_default() {
        let mut editor = editor(&[]);
        select_option(&mut editor, "auto_width");
        assert_eq!(editor.activate(), Outcome::Changed);
        assert_eq!(editor.value("auto_width"), Some("false"));
        assert!(editor.is_dirty());
        editor.activate();
        assert_eq!(editor.value("auto_width"), Some("true"));
    }

    #[test]
    fn a_choice_steps_through_its_values_and_wraps() {
        let mut editor = editor(&[("direction_keys", "letters")]);
        select_option(&mut editor, "direction_keys");
        editor.activate();
        assert_eq!(editor.value("direction_keys"), Some("both"));
        editor.activate();
        assert_eq!(editor.value("direction_keys"), Some("arrows"));
    }

    #[test]
    fn a_number_is_typed_and_checked() {
        let mut editor = editor(&[("max_length", "40")]);
        select_option(&mut editor, "max_length");
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
    fn an_empty_label_is_kept_because_it_hides_the_hint() {
        let mut editor = editor(&[("label_quit", "bye")]);
        let field = Field::Value {
            key: "label_quit".to_string(),
        };
        editor.submit(&field, "");
        assert_eq!(editor.value("label_quit"), Some(""));
    }

    #[test]
    fn reset_removes_the_key_from_the_alias() {
        let mut editor = editor(&[("show_mode", "true")]);
        select_option(&mut editor, "show_mode");
        assert_eq!(editor.reset_selected(), Outcome::Changed);
        assert_eq!(editor.value("show_mode"), None);
        assert_eq!(editor.reset_selected(), Outcome::None);
    }

    #[test]
    fn other_keys_are_listed_added_and_deleted() {
        let mut editor = editor(&[("label_quit", "bye"), ("color_blue", "#0000ff")]);
        let rows = editor.rows();
        let other = rows
            .iter()
            .position(|row| *row == Row::Heading(OTHER_HEADING))
            .unwrap();
        assert_eq!(
            rows[other + 1..],
            [
                Row::Other("color_blue".to_string()),
                Row::Other("label_quit".to_string()),
                Row::AddOther,
            ]
        );

        editor.selected = rows.len() - 1;
        let Outcome::StartField { field, .. } = editor.activate() else {
            panic!("expected a field");
        };
        assert!(matches!(
            editor.submit(&field, "bad key"),
            Outcome::Refused(_)
        ));
        let Outcome::StartField { field, .. } = editor.submit(&field, "keys_quit") else {
            panic!("expected a value field");
        };
        editor.submit(&field, "q");
        assert_eq!(editor.value("keys_quit"), Some("q"));
        assert_eq!(
            editor.selected_row(),
            Some(Row::Other("keys_quit".to_string()))
        );

        assert_eq!(editor.reset_selected(), Outcome::Changed);
        assert_eq!(editor.value("keys_quit"), None);
        assert!(editor.selected_row().unwrap().is_selectable());
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
    fn adding_a_key_that_exists_selects_it() {
        let mut editor = editor(&[]);
        assert_eq!(editor.submit(&Field::NewKey, "mouse"), Outcome::Redraw);
        assert_eq!(
            editor.selected_row(),
            Some(Row::Option(options::find("mouse").unwrap()))
        );
    }

    #[test]
    fn saving_writes_every_alias_with_only_the_edited_one_changed() {
        let mut editor = editor(&[("show_mode", "true")]);
        select_option(&mut editor, "mouse");
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
        editor.selected = editor.rows().len() - 2;
        editor.reset_selected();
        editor.discard();
        assert!(!editor.is_dirty());
        assert!(editor.selected_row().unwrap().is_selectable());
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
        select_option(&mut editor, "auto_width");
        editor.activate();
        editor.selected = 0;
        assert!(matches!(editor.activate(), Outcome::Refused(_)));
        editor.discard();
        editor.selected = 0;
        assert_eq!(editor.activate(), Outcome::SwitchedAlias);
        assert_eq!(editor.alias_name(), "zjhints-alt");
        assert_eq!(editor.value("mouse"), Some("true"));
    }
}
