//! Holds a plugin's settings `Schema` to its configuration reference page.
//!
//! Each plugin documents its options in a Markdown page with one table per
//! group, under a `## Group` heading, one row per option:
//!
//! ```text
//! | `name` | default | what it does |
//! ```
//!
//! The default is `unset` when the option has none, and is otherwise written
//! in backticks or plain. A plugin's tests pass its page (through
//! `include_str!`) and its `Schema` to the `assert_*` functions here, so an
//! option added to the code and not the page, or the reverse, fails the build
//! instead of going undocumented.
//!
//! This is plain std code with no dependency on Zellij, and nothing outside
//! tests calls it, so the linker drops it from a plugin's wasm build.

use std::collections::BTreeMap;

use super::schema::Schema;

/// What the reference page says about one option.
#[derive(Debug, PartialEq, Eq)]
pub struct Documented {
    /// The default as written on the page, or `None` for "unset".
    pub default: Option<String>,
    /// The `##` heading the option's row is under.
    pub heading: String,
}

/// The options a reference page lists, by name as written (a family's name
/// is its pattern, such as `color_<name>`).
///
/// A row is a line of at least three cells whose first cell starts with a
/// backtick, so header rows, separator rows and prose are skipped. A row
/// before any `##` heading gets an empty heading. A name listed twice keeps
/// its last row.
pub fn parse(page: &str) -> BTreeMap<String, Documented> {
    let mut heading = String::new();
    let mut options = BTreeMap::new();
    for line in page.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            heading = title.to_string();
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        // "| `name` | default | what it does |" splits into five cells: the
        // empty ones outside the outer pipes, and the three between.
        if cells.len() < 5 || !cells[1].starts_with('`') {
            continue;
        }
        let name = cells[1].trim_matches('`').to_string();
        let default = match cells[2] {
            "unset" => None,
            written => Some(written.trim_matches('`').to_string()),
        };
        options.insert(
            name,
            Documented {
                default,
                heading: heading.clone(),
            },
        );
    }
    options
}

/// Panics unless the page documents exactly the schema's options, families
/// and screen options: none missing from the page, none only on the page.
pub fn assert_names_match(page: &str, schema: &Schema) {
    fail_on(schema, "names", names_problems(page, schema));
}

/// Panics unless each option's default and group, and each family's group,
/// match what the page says.
pub fn assert_defaults_and_groups_match(page: &str, schema: &Schema) {
    fail_on(
        schema,
        "defaults and groups",
        defaults_and_groups_problems(page, schema),
    );
}

/// Panics unless the page's `##` headings that name a schema group come in
/// the schema's order, and every option and family names a schema group.
pub fn assert_group_order_matches(page: &str, schema: &Schema) {
    fail_on(schema, "group order", group_order_problems(page, schema));
}

fn fail_on(schema: &Schema, what: &str, problems: Vec<String>) {
    assert!(
        problems.is_empty(),
        "the {what} of {plugin}'s options differ from its configuration reference:\n{list}",
        plugin = schema.plugin,
        list = problems.join("\n"),
    );
}

fn names_problems(page: &str, schema: &Schema) -> Vec<String> {
    let documented = parse(page);
    let in_code: Vec<&str> = schema
        .options
        .iter()
        .map(|option| option.name)
        .chain(schema.families.iter().map(|family| family.pattern))
        .chain(schema.screen_options.iter().copied())
        .collect();
    let mut problems = Vec::new();
    for name in &in_code {
        if !documented.contains_key(*name) {
            problems.push(format!("{name} is in the code but not on the page"));
        }
    }
    for name in documented.keys() {
        if !in_code.contains(&name.as_str()) {
            problems.push(format!("{name} is on the page but not in the code"));
        }
    }
    problems
}

fn defaults_and_groups_problems(page: &str, schema: &Schema) -> Vec<String> {
    let documented = parse(page);
    let mut problems = Vec::new();
    for option in schema.options {
        let Some(row) = documented.get(option.name) else {
            problems.push(format!("{} is not on the page", option.name));
            continue;
        };
        if option.default.map(str::to_string) != row.default {
            problems.push(format!(
                "default of {}: code has {:?}, page has {:?}",
                option.name, option.default, row.default
            ));
        }
        if option.group != row.heading {
            problems.push(format!(
                "group of {}: code has {:?}, page has {:?}",
                option.name, option.group, row.heading
            ));
        }
    }
    for family in schema.families {
        let Some(row) = documented.get(family.pattern) else {
            problems.push(format!("{} is not on the page", family.pattern));
            continue;
        };
        if family.group != row.heading {
            problems.push(format!(
                "group of {}: code has {:?}, page has {:?}",
                family.pattern, family.group, row.heading
            ));
        }
    }
    problems
}

fn group_order_problems(page: &str, schema: &Schema) -> Vec<String> {
    let headings: Vec<&str> = page
        .lines()
        .filter_map(|line| line.strip_prefix("## "))
        .filter(|heading| schema.groups.contains(heading))
        .collect();
    let mut problems = Vec::new();
    if headings != schema.groups {
        problems.push(format!(
            "headings on the page are {headings:?}, the schema's groups are {:?}",
            schema.groups
        ));
    }
    for option in schema.options {
        if !schema.groups.contains(&option.group) {
            problems.push(format!(
                "{} names group {:?}, which is not in the schema",
                option.name, option.group
            ));
        }
    }
    for family in schema.families {
        if !schema.groups.contains(&family.group) {
            problems.push(format!(
                "{} names group {:?}, which is not in the schema",
                family.pattern, family.group
            ));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::schema::{family, option, Kind};

    const PAGE: &str = "\
# Configuration

## General

| Option | Default | What it does |
| --- | --- | --- |
| `max_length` | `0` | Widest the bar gets |
| `theme` | unset | The theme |

## Mouse

| `mouse` | true | Clickable items |
| `color_<name>` | unset | A color alias | extra |
| not an option | x | Prose in a table |
| `short` | `1` |
Text with `inline` code | and | pipes
";

    fn schema(
        options: &'static [crate::settings::schema::OptionSpec],
        families: &'static [crate::settings::schema::FamilySpec],
        groups: &'static [&'static str],
    ) -> Schema {
        Schema {
            options,
            families,
            groups,
            plugin: "zjtest",
            screen_options: &[],
            internal_options: &[],
            live_message: "zjtest::configure",
            preview_group: None,
            permissions: Vec::new,
            layer: |configuration, _| configuration.clone(),
            sample_values: |_, _| Vec::new(),
            draw_bar: |_, _, _| String::new(),
        }
    }

    #[test]
    fn parse_reads_names_defaults_and_headings() {
        let documented = parse(PAGE);
        assert_eq!(
            documented["max_length"],
            Documented {
                default: Some("0".to_string()),
                heading: "General".to_string()
            }
        );
        assert_eq!(documented["theme"].default, None);
        assert_eq!(documented["mouse"].default.as_deref(), Some("true"));
        assert_eq!(documented["mouse"].heading, "Mouse");
        // A row with more than three cells still parses.
        assert_eq!(documented["color_<name>"].default, None);
    }

    #[test]
    fn parse_skips_headers_separators_prose_and_short_rows() {
        let documented = parse(PAGE);
        let names: Vec<&str> = documented.keys().map(String::as_str).collect();
        assert_eq!(names, ["color_<name>", "max_length", "mouse", "theme"]);
    }

    #[test]
    fn parse_gives_a_row_before_any_heading_an_empty_heading() {
        let documented = parse("| `a` | 1 | x |\n## G\n| `b` | 2 | y |");
        assert_eq!(documented["a"].heading, "");
        assert_eq!(documented["b"].heading, "G");
    }

    #[test]
    fn parse_keeps_the_last_row_of_a_repeated_name() {
        let documented = parse("| `a` | 1 | x |\n| `a` | 2 | y |");
        assert_eq!(documented["a"].default.as_deref(), Some("2"));
    }

    static MATCHING_OPTIONS: &[crate::settings::schema::OptionSpec] = &[
        option("max_length", "General", Kind::Integer, Some("0"), "Width"),
        option("theme", "General", Kind::Text, None, "Theme"),
        option("mouse", "Mouse", Kind::Bool, Some("true"), "Clicks"),
    ];
    static MATCHING_FAMILIES: &[crate::settings::schema::FamilySpec] =
        &[family("color_<name>", "Mouse", Kind::Text, "A color")];
    const GROUPS: &[&str] = &["General", "Mouse"];

    #[test]
    fn a_schema_that_matches_the_page_passes() {
        let matching = schema(MATCHING_OPTIONS, MATCHING_FAMILIES, GROUPS);
        assert_names_match(PAGE, &matching);
        assert_defaults_and_groups_match(PAGE, &matching);
        assert_group_order_matches(PAGE, &matching);
    }

    #[test]
    fn an_undocumented_option_is_reported() {
        static OPTIONS: &[crate::settings::schema::OptionSpec] = &[option(
            "hidden",
            "General",
            Kind::Bool,
            None,
            "Not on the page",
        )];
        let problems = names_problems(PAGE, &schema(OPTIONS, &[], GROUPS));
        assert!(problems.contains(&"hidden is in the code but not on the page".to_string()));
        assert!(problems.contains(&"theme is on the page but not in the code".to_string()));
    }

    #[test]
    fn a_mismatched_default_or_group_is_reported() {
        static OPTIONS: &[crate::settings::schema::OptionSpec] = &[
            option("max_length", "Mouse", Kind::Integer, Some("5"), "Width"),
            option("theme", "General", Kind::Text, Some("x"), "Theme"),
        ];
        let problems = defaults_and_groups_problems(PAGE, &schema(OPTIONS, &[], GROUPS));
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(problems[0].contains("default of max_length"));
        assert!(problems[1].contains("group of max_length"));
        assert!(problems[2].contains("default of theme"));
    }

    #[test]
    fn headings_out_of_the_schema_order_are_reported() {
        let reversed: &'static [&'static str] = &["Mouse", "General"];
        let problems = group_order_problems(PAGE, &schema(&[], &[], reversed));
        assert_eq!(problems.len(), 1, "{problems:?}");
    }

    #[test]
    fn an_option_in_an_unknown_group_is_reported() {
        static OPTIONS: &[crate::settings::schema::OptionSpec] = &[option(
            "max_length",
            "Elsewhere",
            Kind::Integer,
            None,
            "Width",
        )];
        let problems = group_order_problems(PAGE, &schema(OPTIONS, &[], GROUPS));
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("Elsewhere"));
    }

    #[test]
    #[should_panic(expected = "zjtest's options differ from its configuration reference")]
    fn the_assert_functions_panic_naming_the_plugin() {
        assert_names_match(PAGE, &schema(&[], &[], GROUPS));
    }
}
