use super::SCHEMA;
use zellij_tile::prelude::*;
use zjkit::settings::editor::*;
use zjkit::settings::view::{describe, is_previewed, Control, FormatView, Marker, Rows};

/// Where the screen in these tests runs from.
const LOCATION: &str = "file:~/zjtaskbar.wasm";

fn alias(name: &str, location: &str) -> PluginAliasEntry {
    PluginAliasEntry {
        name: name.to_string(),
        plugin: PluginEntry {
            location: location.to_string(),
            cwd: None,
            configuration: Default::default(),
        },
    }
}

/// An editor of a zjtaskbar alias with nothing set yet.
fn editor() -> Editor {
    let aliases = vec![alias("zjtaskbar", LOCATION)];
    Editor::new(&SCHEMA, LOCATION, aliases.clone(), aliases, None).unwrap()
}

fn show_section(editor: &mut Editor, section: Section) {
    let index = editor.sections().iter().position(|s| *s == section);
    editor.select_section(index.unwrap());
}

/// The lines of the selected section that add a key, or start an entry.
fn add_rows(editor: &Editor) -> Vec<Row> {
    let rows = editor.rows().into_iter();
    rows.filter(|row| matches!(row, Row::Add(_) | Row::AddEntry(_)))
        .collect()
}

/// The lines of the selected section that hold a key set in the alias.
fn key_rows(editor: &Editor) -> Vec<Row> {
    let rows = editor.rows().into_iter();
    rows.filter(|row| matches!(row, Row::Key(_))).collect()
}

/// Select `line`, and start typing the key from what it fills in.
fn start_from(editor: &mut Editor, line: Row) -> (Field, String) {
    let row = editor.rows().iter().position(|row| *row == line);
    editor.select(row.unwrap_or_else(|| panic!("no line {line:?}")));
    let Outcome::StartField { field, text } = editor.activate() else {
        panic!("expected a field");
    };
    (field, text)
}

/// Select the line adding a key of the family with `pattern`, and start
/// typing the key from what it fills in.
fn start_adding(editor: &mut Editor, pattern: &'static str) -> (Field, String) {
    start_from(editor, Row::Add(pattern))
}

/// Start typing a new launcher entry's first key, from the line that adds
/// an entry.
fn start_entry(editor: &mut Editor) -> (Field, String) {
    start_from(editor, Row::AddEntry("launch_<n>_name"))
}

/// Finish the key `field` started with `text` as it was filled in, and give
/// it `value`.
fn accept(editor: &mut Editor, (field, text): (Field, String), value: &str) {
    let Outcome::StartField { field, .. } = editor.submit(&field, &text) else {
        panic!("expected a value field for {text}");
    };
    assert_eq!(editor.submit(&field, value), Outcome::Changed);
}

/// Add `key` with `value` from the line of the family with `pattern`.
fn add(editor: &mut Editor, pattern: &'static str, key: &str, value: &str) {
    let (field, _) = start_adding(editor, pattern);
    let Outcome::StartField { field, .. } = editor.submit(&field, key) else {
        panic!("expected a value field for {key}");
    };
    assert_eq!(editor.submit(&field, value), Outcome::Changed);
}

#[test]
fn only_aliases_loading_the_screens_location_are_edited() {
    let release = "file:/opt/zjhints/target/wasm32-wasip1/release";
    let own = format!("{release}/zjtaskbar.wasm");
    let aliases = vec![
        // Other plugins of the same build.
        alias("zjtabs", &format!("{release}/zjtabs.wasm")),
        alias("zjhints", &format!("{release}/zjhints.wasm")),
        // Named nothing like zjtaskbar, but loaded from the same file.
        alias("taskbar", &own),
        // Named like it, but a different file.
        alias("zjtaskbar", "file:/elsewhere/zjtaskbar.wasm"),
        alias("tab-bar", "zellij:tab-bar"),
    ];
    let editor = Editor::new(&SCHEMA, &own, aliases, vec![], None).unwrap();
    assert_eq!(editor.alias_name(), "taskbar");
    assert!(!editor.sections().contains(&Section::Alias));
}

#[test]
fn without_a_zjtaskbar_alias_there_is_nothing_to_edit() {
    let aliases = vec![
        alias("zjtabs", "file:~/zjtabs.wasm"),
        alias("tab-bar", "zellij:tab-bar"),
    ];
    assert!(Editor::new(&SCHEMA, LOCATION, aliases, vec![], None).is_err());
}

#[test]
fn the_groups_are_listed_in_order_starting_with_the_entries() {
    let editor = editor();
    assert_eq!(editor.section(), Section::Group("Entries"));
    let sections = editor.sections();
    let groups: Vec<&str> = sections
        .iter()
        .filter_map(|section| match section {
            Section::Group(group) => Some(*group),
            _ => None,
        })
        .collect();
    assert_eq!(groups, SCHEMA.groups);
    assert_eq!(
        groups,
        [
            "Entries", "Flags", "Overflow", "Launcher", "Tray", "Clock", "Session", "Mode",
            "Theme", "Colors", "Behavior", "Mouse",
        ]
    );
    assert!(is_previewed(&SCHEMA, Section::Group("Theme")));
    assert!(!is_previewed(&SCHEMA, Section::Group("Colors")));
}

#[test]
fn the_pane_scope_is_a_choice_of_every_tab_or_the_active_one() {
    let mut editor = editor();
    let (section, row) = editor.locate("pane_scope").unwrap();
    assert_eq!(section, Section::Group("Entries"));
    editor.show(section, row);
    assert_eq!(
        editor.choices(),
        Some((vec!["all".to_string(), "tab".to_string()], 0))
    );
}

#[test]
fn the_launcher_group_has_a_line_to_add_an_entry_and_one_for_each_field() {
    let mut editor = editor();
    show_section(&mut editor, Section::Group("Launcher"));
    let patterns = [
        "launch_<n>_name",
        "launch_<n>_command",
        "launch_<n>_args",
        "launch_<n>_cwd",
        "launch_<n>_plugin",
        "launch_<n>_config_<key>",
    ];
    let mut lines = vec![Row::AddEntry("launch_<n>_name")];
    lines.extend(patterns.map(Row::Add));
    assert_eq!(add_rows(&editor), lines);
    let rows = Rows::new(&editor, FormatView::Raw, None);
    assert_eq!(
        rows.view(&Row::AddEntry("launch_<n>_name")).control,
        Control::Button("+ add launch entry".to_string())
    );
    for pattern in patterns {
        let control = Rows::new(&editor, FormatView::Raw, None)
            .view(&Row::Add(pattern))
            .control;
        assert_eq!(control, Control::Button(format!("+ add {pattern}")));
        // Each line is described as the field it adds.
        start_adding(&mut editor, pattern);
        let description = describe(&editor).unwrap();
        let family = SCHEMA.families.iter().find(|f| f.pattern == pattern);
        assert!(
            description.contains(family.unwrap().description),
            "{pattern}: {description:?}"
        );
    }
}

#[test]
fn a_launcher_entry_is_added_a_field_at_a_time_without_typing_its_number() {
    let mut editor = editor();
    show_section(&mut editor, Section::Group("Launcher"));

    // The first entry is numbered 1, and each field's line fills in the
    // number of the entry being added, so Enter takes the key as it is.
    let entry = start_entry(&mut editor);
    assert_eq!(entry.1, "launch_1_name");
    assert_eq!(
        describe(&editor).unwrap(),
        "Add an entry, starting with its launch_<n>_name key as launch_1_name."
    );
    accept(&mut editor, entry, "Logs");
    assert_eq!(editor.section(), Section::Group("Launcher"));
    assert_eq!(
        editor.selected_row(),
        Some(Row::Key("launch_1_name".to_string()))
    );
    let command = start_adding(&mut editor, "launch_<n>_command");
    assert_eq!(command.1, "launch_1_command");
    assert!(describe(&editor)
        .unwrap()
        .starts_with("Add a launch_<n>_command key, starting as launch_1_command. "));
    accept(&mut editor, command, "tail");
    let args = start_adding(&mut editor, "launch_<n>_args");
    assert_eq!(args.1, "launch_1_args");
    accept(&mut editor, args, "-f /var/log/syslog");
    assert_eq!(editor.value("launch_1_name"), Some("Logs"));
    assert_eq!(editor.value("launch_1_command"), Some("tail"));
    assert_eq!(editor.value("launch_1_args"), Some("-f /var/log/syslog"));
    assert_eq!(
        key_rows(&editor),
        ["launch_1_args", "launch_1_command", "launch_1_name"].map(|key| Row::Key(key.to_string()))
    );
    // The add lines are all still there for the next entry.
    assert_eq!(add_rows(&editor).len(), 7);

    // The next entry is numbered after it, and its fields' lines follow.
    let entry = start_entry(&mut editor);
    assert_eq!(entry.1, "launch_2_name");
    accept(&mut editor, entry, "Top");
    assert_eq!(
        start_adding(&mut editor, "launch_<n>_command").1,
        "launch_2_command"
    );

    // Changing a key of the first entry makes it the one being added to.
    let (field, _) = start_from(&mut editor, Row::Key("launch_1_name".to_string()));
    assert_eq!(editor.submit(&field, "Syslog"), Outcome::Changed);
    assert_eq!(
        start_adding(&mut editor, "launch_<n>_cwd").1,
        "launch_1_cwd"
    );
    assert_eq!(start_entry(&mut editor).1, "launch_3_name");
}

#[test]
fn a_key_typed_short_of_a_placeholder_is_refused_until_finished() {
    let mut editor = editor();
    show_section(&mut editor, Section::Group("Launcher"));
    // A plugin's key still needs its name after the number filled in.
    let (field, text) = start_adding(&mut editor, "launch_<n>_config_<key>");
    assert_eq!(text, "launch_1_config_");
    let refused = Outcome::Refused("Finish the key, as in launch_<n>_config_<key>".to_string());
    assert_eq!(editor.submit(&field, &text), refused);
    // Cut back to the prefix, the key names the line it was started from.
    let (field, _) = start_adding(&mut editor, "launch_<n>_command");
    assert_eq!(
        editor.submit(&field, "launch_"),
        Outcome::Refused("Finish the key, as in launch_<n>_command".to_string())
    );
    let Outcome::StartField { .. } = editor.submit(&field, "launch_1_config_theme") else {
        panic!("expected a value field");
    };
}

#[test]
fn a_new_entry_follows_the_highest_numbered_one_set() {
    let mut editor = editor();
    show_section(&mut editor, Section::Group("Launcher"));
    add(&mut editor, "launch_<n>_name", "launch_5_name", "Five");
    add(&mut editor, "launch_<n>_name", "launch_2_name", "Two");
    // The line of a field numbers it for the entry last changed.
    assert_eq!(
        start_adding(&mut editor, "launch_<n>_plugin").1,
        "launch_2_plugin"
    );
    assert_eq!(start_entry(&mut editor).1, "launch_6_name");
    // With the entry last changed removed, the highest is added to.
    let row = editor
        .rows()
        .iter()
        .position(|row| *row == Row::Key("launch_2_name".to_string()));
    editor.select(row.unwrap());
    assert_eq!(editor.reset_selected(), Outcome::Changed);
    assert_eq!(
        start_adding(&mut editor, "launch_<n>_plugin").1,
        "launch_5_plugin"
    );

    // No number follows the largest, so the line leaves the number to type.
    let largest = format!("launch_{}_name", u64::MAX);
    add(&mut editor, "launch_<n>_name", &largest, "Last");
    assert_eq!(start_entry(&mut editor).1, "launch_");
}

#[test]
fn another_alias_s_entries_are_added_to_from_its_own_highest() {
    // Two aliases with the same two entries.
    let with_entries = |name: &str| {
        let mut alias = alias(name, LOCATION);
        alias.plugin.configuration = ["launch_1_name", "launch_2_name"]
            .iter()
            .map(|key| (key.to_string(), "x".to_string()))
            .collect();
        alias
    };
    let aliases = vec![with_entries("first"), with_entries("second")];
    let mut editor = Editor::new(&SCHEMA, LOCATION, aliases.clone(), aliases, None).unwrap();
    show_section(&mut editor, Section::Group("Launcher"));
    // Changed and changed back, so entry 1 was the last added to and
    // nothing is left unsaved.
    for value in ["y", "x"] {
        let (field, _) = start_from(&mut editor, Row::Key("launch_1_name".to_string()));
        assert_eq!(editor.submit(&field, value), Outcome::Changed);
    }
    assert_eq!(
        start_adding(&mut editor, "launch_<n>_cwd").1,
        "launch_1_cwd"
    );
    show_section(&mut editor, Section::Alias);
    editor.select(0);
    assert_eq!(editor.choose(1), Outcome::SwitchedAlias);
    show_section(&mut editor, Section::Group("Launcher"));
    assert_eq!(
        start_adding(&mut editor, "launch_<n>_cwd").1,
        "launch_2_cwd"
    );
}

#[test]
fn a_plugin_s_configuration_key_is_not_taken_for_a_field_of_the_entry() {
    for (key, pattern) in [
        ("launch_1_name", "launch_<n>_name"),
        ("launch_12_command", "launch_<n>_command"),
        ("launch_1_config_foo", "launch_<n>_config_<key>"),
        // A plugin's key named like a field is still the plugin's.
        ("launch_1_config_name", "launch_<n>_config_<key>"),
        ("launch_2_config_welcome_screen", "launch_<n>_config_<key>"),
    ] {
        assert_eq!(
            SCHEMA.family_of(key).map(|family| family.pattern),
            Some(pattern),
            "{key}"
        );
    }
    let mut editor = editor();
    show_section(&mut editor, Section::Group("Launcher"));
    add(
        &mut editor,
        "launch_<n>_config_<key>",
        "launch_1_config_foo",
        "bar",
    );
    assert_eq!(editor.section(), Section::Group("Launcher"));
    assert_eq!(
        key_rows(&editor),
        [Row::Key("launch_1_config_foo".to_string())]
    );
}

#[test]
fn a_key_fitting_no_field_of_an_entry_is_listed_with_a_warning() {
    // A key with no field, a misspelled one, or an entry's number the
    // launcher does not read fits none.
    for key in [
        "launch_1",
        "launch_1_nmae",
        "launch_x_name",
        "launch_01_name",
    ] {
        assert!(SCHEMA.family_of(key).is_none(), "{key}");
        assert_eq!(SCHEMA.group_of(key), Some("Launcher"), "{key}");
    }
    let mut editor = editor();
    show_section(&mut editor, Section::Group("Launcher"));
    add(&mut editor, "launch_<n>_name", "launch_1_nmae", "Logs");
    assert_eq!(editor.section(), Section::Group("Launcher"));
    assert_eq!(key_rows(&editor), [Row::Key("launch_1_nmae".to_string())]);
    assert_eq!(
        describe(&editor).unwrap(),
        "launch_1_nmae: fits no launch_<n>_* option, so zjtaskbar ignores it. Del removes it."
    );
    // The warning shows on the row itself, unsaved or not.
    let row = Row::Key("launch_1_nmae".to_string());
    let marker = |editor: &Editor| Rows::new(editor, FormatView::Raw, None).view(&row).marker;
    assert!(editor.is_edited("launch_1_nmae"));
    assert_eq!(marker(&editor), Marker::Ignored);
    assert_eq!(Marker::Ignored.text(), "✗ ignored");
    // A key fitting a field is no misfit, and counts as an entry.
    add(&mut editor, "launch_<n>_name", "launch_1_name", "Logs");
    let row = Row::Key("launch_1_name".to_string());
    assert_eq!(
        Rows::new(&editor, FormatView::Raw, None).view(&row).marker,
        Marker::Unsaved
    );
}

#[test]
fn a_mode_s_format_label_and_symbol_each_have_a_line_to_add_them() {
    let mut editor = editor();
    show_section(&mut editor, Section::Group("Mode"));
    assert_eq!(
        add_rows(&editor),
        [
            Row::Add("mode_format_<mode>"),
            Row::Add("mode_label_<mode>"),
            Row::Add("mode_key_<mode>"),
        ]
    );
    add(&mut editor, "mode_key_<mode>", "mode_key_locked", "L");
    assert_eq!(editor.section(), Section::Group("Mode"));
    assert_eq!(key_rows(&editor), [Row::Key("mode_key_locked".to_string())]);
    // A word that is no mode of Zellij's fits none of them, and the key is
    // listed with the mode's keys, warning that the bar ignores it.
    assert!(SCHEMA.family_of("mode_key_bogus").is_none());
    add(&mut editor, "mode_key_<mode>", "mode_key_bogus", "B");
    assert_eq!(editor.section(), Section::Group("Mode"));
    assert_eq!(
        describe(&editor).unwrap(),
        "mode_key_bogus: fits no mode_key_<mode> option, so zjtaskbar ignores it. Del removes it."
    );
}
