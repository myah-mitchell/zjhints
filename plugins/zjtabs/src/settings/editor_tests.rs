use super::SCHEMA;
use zellij_tile::prelude::*;
use zjkit::settings::editor::*;
use zjkit::settings::view::is_previewed;

const LOCATION: &str = "file:~/zjtabs.wasm";

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

#[test]
fn only_aliases_loading_the_screens_location_are_edited() {
    let release = "file:/opt/zjhints/target/wasm32-wasip1/release";
    let own = format!("{release}/zjtabs.wasm");
    let aliases = vec![
        // Another plugin of the same build.
        alias("zjhints", &format!("{release}/zjhints.wasm")),
        // Named nothing like zjtabs, but loaded from the same file.
        alias("tabs", &own),
        // Named like it, but a different file.
        alias("zjtabs", "file:/elsewhere/zjtabs.wasm"),
        alias("tab-bar", "zellij:tab-bar"),
    ];
    let editor = Editor::new(&SCHEMA, &own, aliases, vec![], None).unwrap();
    assert_eq!(editor.alias_name(), "tabs");
    assert!(!editor.sections().contains(&Section::Alias));
}

#[test]
fn without_a_zjtabs_alias_there_is_nothing_to_edit() {
    let aliases = vec![
        alias("zjhints", "file:~/zjhints.wasm"),
        alias("tab-bar", "zellij:tab-bar"),
    ];
    assert!(Editor::new(&SCHEMA, LOCATION, aliases, vec![], None).is_err());
}

#[test]
fn the_groups_are_listed_in_order_starting_with_the_tabs() {
    let aliases = vec![alias("zjtabs", LOCATION)];
    let editor = Editor::new(&SCHEMA, LOCATION, aliases.clone(), aliases, None).unwrap();
    assert_eq!(editor.section(), Section::Group("Tabs"));
    let sections = editor.sections();
    let groups: Vec<&str> = sections
        .iter()
        .filter_map(|section| match section {
            Section::Group(group) => Some(*group),
            _ => None,
        })
        .collect();
    assert_eq!(groups, SCHEMA.groups);
    assert!(is_previewed(&SCHEMA, Section::Group("Theme")));
    assert!(!is_previewed(&SCHEMA, Section::Group("Colors")));
}

/// An editor whose zjtabs alias is saved with `configuration`.
fn editor_with(configuration: &[(&str, &str)]) -> Editor {
    let mut zjtabs = alias("zjtabs", LOCATION);
    zjtabs.plugin.configuration = configuration
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    let aliases = vec![zjtabs];
    Editor::new(&SCHEMA, LOCATION, aliases.clone(), aliases, None).unwrap()
}

#[test]
fn the_deprecated_ambiguous_width_is_listed_only_when_set_and_cannot_be_added() {
    let listed = |editor: &Editor| {
        editor.sections().into_iter().any(|section| {
            editor.rows_of(section).iter().any(|row| match row {
                Row::Option(option) => option.name == "ambiguous_width",
                Row::Key(key) => key == "ambiguous_width",
                _ => false,
            })
        })
    };
    let mut unset = editor_with(&[]);
    assert!(!listed(&unset));
    assert!(matches!(
        unset.submit(&Field::NewKey, "ambiguous_width"),
        Outcome::Refused(_)
    ));
    let set = editor_with(&[("ambiguous_width", "2")]);
    assert!(set
        .rows_of(Section::Other)
        .contains(&Row::Key("ambiguous_width".to_string())));
    assert_eq!(set.value("ambiguous_width"), Some("2"));
}
