use super::SCHEMA;
use zellij_tile::prelude::*;
use zjkit::settings::editor::*;

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

/// Where the screen in these tests runs from.
const LOCATION: &str = "file:~/zjhints.wasm";

fn aliases(configuration: &[(&str, &str)]) -> Vec<PluginAliasEntry> {
    vec![
        alias("tab-bar", "zellij:tab-bar", &[]),
        alias("zjhints", LOCATION, configuration),
    ]
}

/// An editor whose session holds `configuration`, saved as it is.
fn editor(configuration: &[(&str, &str)]) -> Editor {
    Editor::new(
        &SCHEMA,
        LOCATION,
        aliases(configuration),
        aliases(configuration),
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
    assert!(!editor.sections().contains(&Section::Alias));
}

#[test]
fn only_aliases_loading_the_screens_location_are_edited() {
    let release = "file:/opt/zjhints/target/wasm32-wasip1/release";
    let own = format!("{release}/zjhints.wasm");
    let aliases = vec![
        // Named after this plugin by its directory alone.
        alias("zjtabs", &format!("{release}/zjtabs.wasm"), &[]),
        // Named nothing like it, but loaded from the same file.
        alias("hints", &own, &[]),
        // Named like it, but a different file.
        alias("zjhints", "file:/elsewhere/zjhints.wasm", &[]),
    ];
    let editor = Editor::new(&SCHEMA, &own, aliases, vec![], None).unwrap();
    assert_eq!(editor.alias_name(), "hints");
    assert!(!editor.sections().contains(&Section::Alias));
}

#[test]
fn a_screen_started_by_alias_name_edits_that_aliases_location() {
    let aliases = vec![
        alias("tab-bar", "zellij:tab-bar", &[]),
        alias("hints", "file:/opt/bin/bar.wasm", &[]),
    ];
    assert_eq!(
        own_location(&aliases, "hints"),
        Some("file:/opt/bin/bar.wasm")
    );
    assert_eq!(own_location(&aliases, "file:/x.wasm"), Some("file:/x.wasm"));
    assert_eq!(own_location(&aliases, "nope"), None);
}

#[test]
fn without_a_zjhints_alias_there_is_nothing_to_edit() {
    let result = Editor::new(
        &SCHEMA,
        LOCATION,
        vec![alias("tab-bar", "zellij:tab-bar", &[])],
        vec![],
        None,
    );
    assert!(result.is_err());
}

#[test]
fn a_pinned_alias_is_edited_and_an_unknown_one_refused() {
    let aliases = vec![
        alias("zjhints", "file:~/zjhints.wasm", &[]),
        alias("zjhints-alt", "file:~/zjhints.wasm", &[]),
    ];
    let editor = Editor::new(
        &SCHEMA,
        LOCATION,
        aliases.clone(),
        aliases.clone(),
        Some("zjhints-alt"),
    )
    .unwrap();
    assert_eq!(editor.alias_name(), "zjhints-alt");
    assert_eq!(editor.sections()[0], Section::Alias);
    assert_eq!(editor.rows_of(Section::Alias), [Row::Alias]);
    assert!(Editor::new(&SCHEMA, LOCATION, aliases.clone(), aliases, Some("nope")).is_err());
}

#[test]
fn the_first_group_and_option_are_shown_first() {
    let editor = editor(&[]);
    assert_eq!(editor.section(), Section::Group("Fitting"));
    assert_eq!(editor.focus, Focus::List);
    assert_eq!(
        editor.selected_row(),
        Some(Row::Option(SCHEMA.find("auto_width").unwrap()))
    );
}

#[test]
fn every_group_is_in_the_sidebar_with_other_last() {
    let sections = editor(&[]).sections();
    assert_eq!(sections.len(), SCHEMA.groups.len() + 1);
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
        Some(Row::Option(SCHEMA.find("hint_precedence").unwrap()))
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
        Some(Row::Option(SCHEMA.find("mouse").unwrap()))
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
    Editor::new(&SCHEMA, LOCATION, aliases(session), aliases(saved), None).unwrap()
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
    let editor = Editor::new(
        &SCHEMA,
        LOCATION,
        aliases(&[("mouse", "true")]),
        vec![],
        None,
    )
    .unwrap();
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
        .contains(&Row::Option(SCHEMA.find("max_length").unwrap())));
    editor.start_search("hjkl");
    assert!(editor
        .rows()
        .contains(&Row::Option(SCHEMA.find("direction_keys").unwrap())));

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
        Some(Row::Option(SCHEMA.find("auto_width").unwrap()))
    );
    // Adding a key that exists leaves the search to show it.
    editor.submit(&Field::NewKey, "mouse");
    assert_eq!(editor.search(), None);
    assert_eq!(editor.section(), Section::Group("Mouse"));
}

#[test]
fn the_alias_is_chosen_from_a_list_once_the_edits_are_saved_or_reverted() {
    let both = vec![
        alias("zjhints", "file:~/zjhints.wasm", &[]),
        alias("zjhints-alt", "file:~/zjhints.wasm", &[("mouse", "true")]),
    ];
    let mut editor = Editor::new(&SCHEMA, LOCATION, both.clone(), both, None).unwrap();
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
