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
