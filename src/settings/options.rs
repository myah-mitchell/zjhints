//! Every option the bar reads, as the settings screen presents it.
//!
//! This is the one list of options in the code. A test holds it to the
//! configuration reference (docs/configuration.md), so an option added to one
//! and not the other fails the build rather than going missing from the screen.

/// A heading the options are grouped under, in the order they are shown. The
/// same headings, in the same order, as the configuration reference.
///
/// A group may hold fixed options, families, or both. Adding one takes its
/// name here, a heading in the reference, and its options or families naming
/// it as their group.
pub const GROUPS: &[&str] = &[
    "Fitting",
    "Hints",
    "Keys and ordering",
    "Theme",
    "Styling",
    "Colors",
    "Key aliases",
    "Modifier aliases",
    "Labels",
    "Mode prefix",
    "Nested sessions",
    "Mouse",
    "Integration",
];

/// What sort of value an option takes, which decides how it is edited.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// `true` or `false`, toggled.
    Bool,
    /// One of a fixed set of values, cycled through.
    Choice(&'static [&'static str]),
    /// A whole number, typed.
    Integer,
    /// A number from 0.0 to 1.0, typed.
    Fraction,
    /// Anything, typed.
    Text,
    /// A zjstatus-style format string, typed. Shown rendered in the list
    /// unless the raw text is asked for.
    Format,
}

impl Kind {
    /// Whether the value is free text, which the list always shows quoted.
    pub fn is_text(self) -> bool {
        matches!(self, Kind::Text | Kind::Format)
    }
}

/// One option with a fixed name.
#[derive(Debug, PartialEq)]
pub struct OptionSpec {
    pub name: &'static str,
    pub group: &'static str,
    pub kind: Kind,
    /// The value in effect when the option is not set, or `None` when unset
    /// means "off" rather than a value.
    pub default: Option<&'static str>,
    pub description: &'static str,
}

/// A family of options sharing a prefix, such as `label_<id>`. Keys in a
/// family have no fixed name, so the screen lists them as plain pairs.
#[derive(Debug)]
pub struct FamilySpec {
    /// The name as the reference writes it, placeholder included.
    pub pattern: &'static str,
    pub group: &'static str,
    pub kind: Kind,
    pub description: &'static str,
}

impl FamilySpec {
    /// The part of the name before the first placeholder, such as `color_`.
    pub fn prefix(&self) -> &'static str {
        self.pattern
            .split_once('<')
            .map_or(self.pattern, |(prefix, _)| prefix)
    }
}

const fn family(
    pattern: &'static str,
    group: &'static str,
    kind: Kind,
    description: &'static str,
) -> FamilySpec {
    FamilySpec {
        pattern,
        group,
        kind,
        description,
    }
}

const fn option(
    name: &'static str,
    group: &'static str,
    kind: Kind,
    default: Option<&'static str>,
    description: &'static str,
) -> OptionSpec {
    OptionSpec {
        name,
        group,
        kind,
        default,
        description,
    }
}

pub const OPTIONS: &[OptionSpec] = &[
    option(
        "auto_width",
        "Fitting",
        Kind::Bool,
        Some("true"),
        "Fit the hints to the bar's own pane, or to the terminal when feeding zjstatus",
    ),
    option(
        "max_length",
        "Fitting",
        Kind::Integer,
        Some("0"),
        "Hard cap on the line's width in columns, 0 for unlimited",
    ),
    option(
        "reserve_columns",
        "Fitting",
        Kind::Integer,
        Some("0"),
        "Columns to leave free beside the hints in zjstatus; unused in the bar's own pane",
    ),
    option(
        "ambiguous_width",
        "Fitting",
        Kind::Choice(&["1", "2"]),
        Some("1"),
        "Columns an East Asian Ambiguous character occupies",
    ),
    option(
        "overflow_str",
        "Fitting",
        Kind::Text,
        Some("..."),
        "Appended when a lone hint is too wide to fit and must be cut mid-hint",
    ),
    option(
        "drop_indicator",
        "Fitting",
        Kind::Format,
        None,
        "Format string marking where hints were dropped (… when mouse is on)",
    ),
    option(
        "hint_precedence",
        "Fitting",
        Kind::Choice(&["tl", "lt"]),
        Some("tl"),
        "Which pinned group is kept longest",
    ),
    option(
        "discover_hints",
        "Hints",
        Kind::Bool,
        Some("true"),
        "Also show every enabled keybinding beyond the curated list",
    ),
    option(
        "hide_shared_hints",
        "Hints",
        Kind::Bool,
        Some("true"),
        "Hide bindings inherited from the base mode, except in locked mode",
    ),
    option(
        "hide_in_base_mode",
        "Hints",
        Kind::Bool,
        Some("false"),
        "Hide hints in the base mode, whichever mode default_mode names",
    ),
    option(
        "hidden",
        "Hints",
        Kind::Bool,
        Some("false"),
        "Start with the bar hidden; only a zjhints::show or zjhints::toggle keybinding or zellij pipe brings it back",
    ),
    option(
        "direction_keys",
        "Keys and ordering",
        Kind::Choice(&["both", "arrows", "letters"]),
        Some("both"),
        "Which keys to show when a hint is bound to both hjkl and the arrows",
    ),
    option(
        "key_order",
        "Keys and ordering",
        Kind::Choice(&["qwerty", "dvorak", "colemak", "abcdef", "none"]),
        Some("qwerty"),
        "How keys within a hint are ordered",
    ),
    option(
        "hint_order",
        "Keys and ordering",
        Kind::Text,
        None,
        "Hint ids pinned to the start or end of each mode, around a * for the rest",
    ),
    option(
        "theme",
        "Theme",
        Kind::Choice(&crate::theme::CHOICES),
        Some("zellij"),
        "Follow the Zellij theme, style the bar yourself, or use a built-in theme",
    ),
    option(
        "key_format",
        "Styling",
        Kind::Format,
        None,
        "Format string for each hint's keys, theme palette when unset",
    ),
    option(
        "desc_format",
        "Styling",
        Kind::Format,
        None,
        "Format string for each hint's description, theme palette when unset",
    ),
    option(
        "hint_spacer",
        "Styling",
        Kind::Format,
        None,
        "Format string drawn between hints and after the mode prefix",
    ),
    option(
        "hide_when_nested",
        "Nested sessions",
        Kind::Bool,
        Some("true"),
        "Render nothing while this session is nested inside another",
    ),
    option(
        "collapse_when_empty",
        "Nested sessions",
        Kind::Bool,
        Some("true"),
        "Hand the layout row back while the bar has nothing to draw",
    ),
    option(
        "dim_when_unfocused",
        "Nested sessions",
        Kind::Bool,
        Some("true"),
        "Dim hints while this session is not the one receiving input",
    ),
    option(
        "dim_strength",
        "Nested sessions",
        Kind::Fraction,
        Some("0.5"),
        "How strongly to dim, from 0.0 to 1.0",
    ),
    option(
        "show_mode",
        "Mode prefix",
        Kind::Bool,
        Some("false"),
        "Prefix the hints line with the current input mode",
    ),
    option(
        "mode_format",
        "Mode prefix",
        Kind::Format,
        None,
        "Format string for the mode prefix, with a {mode} placeholder",
    ),
    option(
        "mouse",
        "Mouse",
        Kind::Bool,
        Some("false"),
        "Right-click menu, clickable hints, and menus of hints that did not fit",
    ),
    option(
        "mouse_nested",
        "Mouse",
        Kind::Bool,
        Some("false"),
        "Also click a descended-into nested session's hints",
    ),
    option(
        "pipe_name",
        "Integration",
        Kind::Text,
        Some("zjhints"),
        "Name of the pipe zjstatus reads the hints from",
    ),
];

pub const FAMILIES: &[FamilySpec] = &[
    family(
        "key_format_<id>",
        "Styling",
        Kind::Format,
        "Format string for one hint's keys",
    ),
    family(
        "desc_format_<id>",
        "Styling",
        Kind::Format,
        "Format string for one hint's description",
    ),
    family(
        "keys_<id>",
        "Styling",
        Kind::Text,
        "A fixed string in place of a hint's keys",
    ),
    family(
        "color_<name>",
        "Colors",
        Kind::Text,
        "A color alias, used as $name in format strings",
    ),
    family(
        "key_alias_<name>",
        "Key aliases",
        Kind::Text,
        "A symbol in place of a key's name",
    ),
    family(
        "mod_alias_<name>",
        "Modifier aliases",
        Kind::Text,
        "A symbol in place of a modifier's name",
    ),
    family(
        "chord_mods_<name>",
        "Modifier aliases",
        Kind::Text,
        "The modifiers of a chord alias",
    ),
    family(
        "chord_alias_<name>",
        "Modifier aliases",
        Kind::Text,
        "The symbol shown for a chord alias",
    ),
    family(
        "label_<id>",
        "Labels",
        Kind::Text,
        "A hint's label; empty hides the hint",
    ),
    family(
        "label_<mode>_<id>",
        "Labels",
        Kind::Text,
        "A hint's label in one mode; empty hides it",
    ),
    family(
        "mode_format_<mode>",
        "Mode prefix",
        Kind::Format,
        "The mode prefix's format string for one mode",
    ),
];

/// Options of the settings screen itself, read only when it is opened. They
/// configure the screen rather than the bar, so they are not listed on it.
pub const SCREEN_OPTIONS: &[&str] = &["settings", "alias"];

/// Keys Zellij takes for itself in a plugin's configuration: they are dropped
/// or read as something else rather than reaching the plugin.
pub const RESERVED_KEYS: &[&str] = &[
    "location",
    "path",
    "_allow_exec_host_cmd",
    "hold_on_close",
    "hold_on_start",
    "cwd",
    "name",
    "direction",
    "floating",
    "move_to_focused_tab",
    "launch_new",
    "payload",
    "skip_cache",
    "title",
    "in_place",
    "skip_plugin_cache",
];

/// The option named `name`, if it has a fixed name.
pub fn find(name: &str) -> Option<&'static OptionSpec> {
    OPTIONS.iter().find(|option| option.name == name)
}

/// The mode names per-mode keys are suffixed with, as `mode_config_suffix`
/// spells them.
const MODE_NAMES: &[&str] = &[
    "normal",
    "locked",
    "resize",
    "pane",
    "tab",
    "scroll",
    "enter_search",
    "search",
    "rename_tab",
    "rename_pane",
    "session",
    "move",
    "prompt",
    "tmux",
];

/// What a key outside the fixed options is, by the family it fits. A key
/// fitting both `label_<mode>_<id>` and `label_<id>` is taken as the
/// mode-scoped one, the way the bar reads it.
pub fn family_of(key: &str) -> Option<&'static FamilySpec> {
    let fits = |family: &&FamilySpec| {
        let (prefix, placeholders) = family.pattern.split_once('<').unwrap_or_default();
        let Some(rest) = key.strip_prefix(prefix).filter(|rest| !rest.is_empty()) else {
            return false;
        };
        match placeholders {
            "mode>" => MODE_NAMES.contains(&rest),
            "mode>_<id>" => MODE_NAMES.iter().any(|mode| {
                rest.strip_prefix(mode)
                    .and_then(|after| after.strip_prefix('_'))
                    .is_some_and(|id| !id.is_empty())
            }),
            _ => true,
        }
    };
    FAMILIES
        .iter()
        .filter(fits)
        .max_by_key(|family| (family.pattern.find('<'), family.pattern.contains("<mode>")))
}

/// The group a key is listed under: its option's, or its family's. `None`
/// for a key that is neither, which the screen lists under "Other".
pub fn group_of(key: &str) -> Option<&'static str> {
    find(key)
        .map(|option| option.group)
        .or_else(|| family_of(key).map(|family| family.group))
}

/// The kind of value a key takes: its option's, or its family's, or text.
pub fn kind_of(key: &str) -> Kind {
    find(key)
        .map(|option| option.kind)
        .or_else(|| family_of(key).map(|family| family.kind))
        .unwrap_or(Kind::Text)
}

/// Whether `value` is acceptable for an option of this kind. Text is always
/// acceptable; an empty value means unset for any kind.
pub fn validate(kind: Kind, value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Ok(());
    }
    match kind {
        Kind::Bool if value == "true" || value == "false" => Ok(()),
        Kind::Bool => Err("true or false".to_string()),
        Kind::Choice(choices) if choices.contains(&value) => Ok(()),
        Kind::Choice(choices) => Err(format!("one of {}", choices.join(", "))),
        Kind::Integer => value
            .parse::<usize>()
            .map(|_| ())
            .map_err(|_| "a whole number, 0 or more".to_string()),
        Kind::Fraction => match value.parse::<f32>() {
            Ok(number) if (0.0..=1.0).contains(&number) => Ok(()),
            _ => Err("a number from 0.0 to 1.0".to_string()),
        },
        Kind::Text | Kind::Format => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// The options the configuration reference lists: name -> default as
    /// written there (`None` for "unset"), and the heading each is under.
    fn documented() -> BTreeMap<String, (Option<String>, String)> {
        let reference = include_str!("../../docs/configuration.md");
        let mut heading = String::new();
        let mut options = BTreeMap::new();
        for line in reference.lines() {
            if let Some(title) = line.strip_prefix("## ") {
                heading = title.to_string();
                continue;
            }
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            // "| `name` | default | what it does |" splits into five cells.
            if cells.len() < 5 || !cells[1].starts_with('`') {
                continue;
            }
            let name = cells[1].trim_matches('`').to_string();
            let default = match cells[2] {
                "unset" => None,
                written => Some(written.trim_matches('`').to_string()),
            };
            options.insert(name, (default, heading.clone()));
        }
        options
    }

    #[test]
    fn every_documented_option_is_in_the_table_and_no_other() {
        let documented: Vec<String> = documented().into_keys().collect();
        let mut in_code: Vec<String> = OPTIONS
            .iter()
            .map(|option| option.name.to_string())
            .chain(FAMILIES.iter().map(|family| family.pattern.to_string()))
            .chain(SCREEN_OPTIONS.iter().map(|name| name.to_string()))
            .collect();
        in_code.sort();
        assert_eq!(in_code, documented);
    }

    #[test]
    fn defaults_and_groups_match_the_reference() {
        let documented = documented();
        for option in OPTIONS {
            let (default, heading) = &documented[option.name];
            assert_eq!(
                option.default.map(str::to_string),
                *default,
                "default of {}",
                option.name
            );
            assert_eq!(option.group, heading, "group of {}", option.name);
        }
        for family in FAMILIES {
            let (_, heading) = &documented[family.pattern];
            assert_eq!(family.group, heading, "group of {}", family.pattern);
        }
    }

    #[test]
    fn groups_are_in_reference_order() {
        let reference = include_str!("../../docs/configuration.md");
        let headings: Vec<&str> = reference
            .lines()
            .filter_map(|line| line.strip_prefix("## "))
            .filter(|heading| GROUPS.contains(heading))
            .collect();
        assert_eq!(headings, GROUPS);
        for option in OPTIONS {
            assert!(GROUPS.contains(&option.group), "{}", option.name);
        }
        for family in FAMILIES {
            assert!(GROUPS.contains(&family.group), "{}", family.pattern);
        }
    }

    #[test]
    fn choice_defaults_are_among_their_choices() {
        for option in OPTIONS {
            if let (Kind::Choice(choices), Some(default)) = (option.kind, option.default) {
                assert!(choices.contains(&default), "{}", option.name);
            }
        }
    }

    #[test]
    fn keys_are_sorted_into_the_closest_family() {
        let pattern = |key| family_of(key).map(|family| family.pattern);
        assert_eq!(pattern("label_split_down"), Some("label_<id>"));
        assert_eq!(pattern("key_format_quit"), Some("key_format_<id>"));
        assert_eq!(pattern("color_blue"), Some("color_<name>"));
        assert_eq!(pattern("mode_format_locked"), Some("mode_format_<mode>"));
        assert_eq!(
            pattern("label_locked_mode_normal"),
            Some("label_<mode>_<id>")
        );
        assert_eq!(pattern("mode_format_bogus"), None);
        assert_eq!(pattern("label_"), None);
        assert_eq!(pattern("something_else"), None);
    }

    #[test]
    fn keys_outside_the_options_are_sorted_into_their_family_groups() {
        assert_eq!(group_of("color_blue"), Some("Colors"));
        assert_eq!(group_of("key_alias_enter"), Some("Key aliases"));
        assert_eq!(group_of("mod_alias_ctrl"), Some("Modifier aliases"));
        assert_eq!(group_of("chord_mods_leader"), Some("Modifier aliases"));
        assert_eq!(group_of("chord_alias_leader"), Some("Modifier aliases"));
        assert_eq!(group_of("label_split_down"), Some("Labels"));
        assert_eq!(group_of("label_locked_mode_normal"), Some("Labels"));
        assert_eq!(group_of("mode_format_locked"), Some("Mode prefix"));
        assert_eq!(group_of("key_format_quit"), Some("Styling"));
        assert_eq!(group_of("desc_format_quit"), Some("Styling"));
        assert_eq!(group_of("keys_go_to_tab"), Some("Styling"));
        assert_eq!(group_of("mode_format"), Some("Mode prefix"));
        assert_eq!(group_of("something_else"), None);
        assert_eq!(group_of("mode_format_bogus"), None);
    }

    #[test]
    fn format_strings_are_told_apart_from_plain_text() {
        assert_eq!(kind_of("key_format"), Kind::Format);
        assert_eq!(kind_of("key_format_quit"), Kind::Format);
        assert_eq!(kind_of("mode_format_locked"), Kind::Format);
        assert_eq!(kind_of("label_quit"), Kind::Text);
        assert_eq!(kind_of("overflow_str"), Kind::Text);
        assert_eq!(kind_of("max_length"), Kind::Integer);
        assert_eq!(kind_of("something_else"), Kind::Text);
        assert_eq!(
            FAMILIES
                .iter()
                .map(FamilySpec::prefix)
                .find(|prefix| prefix.starts_with("color")),
            Some("color_")
        );
    }

    #[test]
    fn values_are_checked_by_kind() {
        assert!(validate(Kind::Integer, "12").is_ok());
        assert!(validate(Kind::Integer, "-1").is_err());
        assert!(validate(Kind::Fraction, "0.25").is_ok());
        assert!(validate(Kind::Fraction, "1.5").is_err());
        assert!(validate(Kind::Bool, "yes").is_err());
        assert!(validate(Kind::Choice(&["tl", "lt"]), "lt").is_ok());
        assert!(validate(Kind::Choice(&["tl", "lt"]), "x").is_err());
        assert!(validate(Kind::Integer, "").is_ok());
    }
}
