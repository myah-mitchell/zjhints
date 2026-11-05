//! The settings screen: a floating pane that edits one of a plugin's aliases
//! in the Zellij config, previews every change on the running bars, and
//! saves the result to the config file. Each plugin describes its options in
//! a `schema::Schema`.

pub mod editor;
pub mod live;
pub mod reference;
pub mod schema;
pub mod screen;
pub mod view;

/// A small plugin for the screen's own tests.
#[cfg(test)]
pub(crate) static TEST_SCHEMA: schema::Schema = {
    use schema::{family, option, Kind};
    schema::Schema {
        plugin: "zjtest",
        groups: &["General", "Mouse"],
        options: &[
            option(
                "max_length",
                "General",
                Kind::Integer,
                Some("0"),
                "Widest the bar gets",
            ),
            option(
                "mouse",
                "Mouse",
                Kind::Bool,
                Some("true"),
                "Clickable items",
            ),
        ],
        families: &[family(
            "color_<name>",
            "General",
            Kind::Text,
            "A color alias",
        )],
        screen_options: &["settings", "alias"],
        internal_options: &[],
        live_message: "zjtest::configure",
        preview_group: None,
        permissions: Vec::new,
        layer: |configuration, _| configuration.clone(),
        sample_values: |_, _| Vec::new(),
        draw_bar: |_, _, _| String::new(),
    }
};
