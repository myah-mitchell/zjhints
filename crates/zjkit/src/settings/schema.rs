//! What a plugin tells the settings screen about itself: the options it
//! reads, how they are grouped, and the few things only it can do, such as
//! drawing its bar for the preview.

use std::collections::BTreeMap;
use zellij_tile::prelude::{ModeInfo, PermissionType, Styling};

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
        pattern_prefix(self.pattern)
    }

    /// Whether the family's keys are fields of numbered entries: its first
    /// placeholder is `<n>`, as in `launch_<n>_name`, so each key names the
    /// entry it belongs to by number. The families sharing the prefix are
    /// the entry's fields.
    pub fn is_numbered(&self) -> bool {
        self.pattern[self.prefix().len()..].starts_with("<n>")
    }
}

/// The part of a family's `pattern` before its first placeholder, such as
/// `color_` of `color_<name>`: what every key of the family starts with.
pub fn pattern_prefix(pattern: &str) -> &str {
    pattern
        .split_once('<')
        .map_or(pattern, |(prefix, _)| prefix)
}

/// How a key of a numbered family's `pattern` starts for the entry
/// `number`: the pattern with its `<n>` filled in, up to the next
/// placeholder. `launch_3_command` for `launch_<n>_command`, a whole key, and
/// `launch_3_config_` for `launch_<n>_config_<key>`, which still needs its
/// `<key>`.
pub fn entry_key_start(pattern: &str, number: u64) -> String {
    let filled = pattern.replacen("<n>", &number.to_string(), 1);
    pattern_prefix(&filled).to_string()
}

/// Whether `key` is `pattern` typed up to one of its placeholders and no
/// further, so a key typed that far is not finished: `color_` for
/// `color_<name>`, or `launch_1_config_` for `launch_<n>_config_<key>`.
pub fn is_unfinished(pattern: &str, key: &str) -> bool {
    pattern
        .match_indices('<')
        .any(|(index, _)| placeholder_lengths(&pattern[..index], key).is_some())
}

pub const fn family(
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

pub const fn option(
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

/// The mode names per-mode keys are suffixed with, as `mode_config_suffix`
/// spells them. A family whose pattern has a `<mode>` placeholder only takes
/// keys naming one of these.
pub const MODE_NAMES: &[&str] = &[
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
    "rename_session",
    "session",
    "move",
    "prompt",
    "tmux",
];

/// A plugin's configuration: option names to their values.
pub type Configuration = BTreeMap<String, String>;

/// Everything the settings screen knows about one plugin.
pub struct Schema {
    /// The plugin's name, which the screen's messages name it by. A bar's
    /// entry on Zellij's settings screen and its log lines use it too; see
    /// `bar::BarSpec`.
    pub plugin: &'static str,
    /// The headings the options are grouped under, in the order they are
    /// shown. A group may hold fixed options, families, or both.
    pub groups: &'static [&'static str],
    pub options: &'static [OptionSpec],
    pub families: &'static [FamilySpec],
    /// Options of the settings screen itself, read only when it is opened.
    /// They configure the screen rather than the bar, so they are not listed
    /// on it.
    pub screen_options: &'static [&'static str],
    /// Keys only the plugin itself sets, such as a popup's, which the screen
    /// refuses to add.
    pub internal_options: &'static [&'static str],
    /// The name of the pipe message a preview of the edits is sent to the
    /// bars as.
    pub live_message: &'static str,
    /// The group whose list has a preview of the bar under it, if any.
    pub preview_group: Option<&'static str>,
    /// Every permission a bar of the plugin may use. The screen runs from
    /// the same location as the bars, so Zellij caches one grant for both,
    /// and a grant replaces the cached one rather than adding to it.
    pub permissions: fn() -> Vec<PermissionType>,
    /// The configuration a bar reads, with the selected themes laid under
    /// the user's own values. `Styling` is the running Zellij theme.
    pub layer: fn(&Configuration, Option<&Styling>) -> Configuration,
    /// The placeholders the bar fills in a format string under a key, with
    /// the sample value each gets in the list.
    pub sample_values: fn(&str, &Configuration) -> Vec<(&'static str, String)>,
    /// The bar as a configuration draws it, a number of columns wide, in the
    /// mode, keybindings and Zellij theme of the `ModeInfo`.
    pub draw_bar: fn(Configuration, &ModeInfo, usize) -> String,
}

impl Schema {
    /// The option named `name`, if it has a fixed name.
    pub fn find(&self, name: &str) -> Option<&'static OptionSpec> {
        self.options.iter().find(|option| option.name == name)
    }

    /// What a key outside the fixed options is, by the family it fits. See
    /// `fits_pattern` for what fitting a pattern takes.
    ///
    /// A key that fits more than one family is taken as the one whose pattern
    /// spells out the most of it, the most specific: `label_locked_quit` fits
    /// both `label_<mode>_<id>` and `label_<id>` and is taken as the
    /// mode-scoped one, the way the bar reads it.
    ///
    /// Between patterns that spell out as much, the key is read left to
    /// right, as the bars read their keys: the family whose first
    /// placeholder takes the fewest characters wins, then the one whose
    /// second does, and so on. `item_1_config_command` fits `item_<id>_command`
    /// with `1_config` for `<id>` and `item_<id>_config_<key>` with `1`, so it
    /// is taken as the second. Patterns that still tie differ only in their
    /// placeholders' names, and the first in the alphabet is taken, so the
    /// order the families are declared in never decides.
    pub fn family_of(&self, key: &str) -> Option<&'static FamilySpec> {
        self.families
            .iter()
            .filter_map(|family| Some((family, placeholder_lengths(family.pattern, key)?)))
            .max_by(|(family, lengths), (other_family, other_lengths)| {
                literal_length(family.pattern)
                    .cmp(&literal_length(other_family.pattern))
                    .then_with(|| other_lengths.cmp(lengths))
                    .then_with(|| other_family.pattern.cmp(family.pattern))
            })
            .map(|(family, _)| family)
    }

    /// The family a key that fits none was most likely meant for: the one
    /// whose prefix the key starts with, the longest prefix when several
    /// do, and the first declared of those sharing it. `launch_1_nmae`,
    /// `launch_x_name` and `launch_1` all start with `launch_`, the prefix
    /// of every field of a launcher entry. The bar reads no such key, so the
    /// screen lists it in the family's group with a warning, next to the
    /// keys it was meant to be one of.
    ///
    /// `None` for a key the plugin reads, a fixed option, a key fitting a
    /// family, or an option of the screen or the plugin itself, and for a
    /// key starting with no family's prefix, which the screen lists under
    /// "Other".
    pub fn misfit_family(&self, key: &str) -> Option<&'static FamilySpec> {
        let read = self.find(key).is_some()
            || self.family_of(key).is_some()
            || self.screen_options.contains(&key)
            || self.internal_options.contains(&key);
        if read {
            return None;
        }
        self.families
            .iter()
            .filter(|family| !family.prefix().is_empty() && key.starts_with(family.prefix()))
            .fold(
                None,
                |longest: Option<&'static FamilySpec>, family| match longest {
                    Some(longest) if longest.prefix().len() >= family.prefix().len() => {
                        Some(longest)
                    }
                    _ => Some(family),
                },
            )
    }

    /// Every family with `prefix` named at once, as a warning about a key
    /// fitting none of them names them: the family's own pattern when no
    /// other has the prefix, or what the patterns spell out in common
    /// followed by `*`. The six fields of a launcher entry are
    /// `launch_<n>_*`. A placeholder the patterns share only part of is left
    /// out, so `label_<id>` and `label_<mode>_<id>` are `label_*`.
    pub fn patterns_with_prefix(&self, prefix: &str) -> String {
        let mut patterns = self
            .families
            .iter()
            .filter(|family| family.prefix() == prefix)
            .map(|family| family.pattern);
        let Some(first) = patterns.next() else {
            return format!("{prefix}*");
        };
        let mut common = first;
        let mut several = false;
        for pattern in patterns {
            several = true;
            let shared = common
                .char_indices()
                .zip(pattern.chars())
                .find(|((_, character), other)| character != other)
                .map_or(common.len().min(pattern.len()), |((index, _), _)| index);
            common = &common[..shared];
        }
        if !several {
            return first.to_string();
        }
        // Cut a placeholder the patterns share only the start of.
        let opened = common.rfind('<');
        let closed = common.rfind('>');
        if opened > closed {
            common = &common[..opened.unwrap_or(0)];
        }
        format!("{common}*")
    }

    /// The number of the entry `key` is a field of: what the `<n>` of its
    /// family stands for, when that family is numbered and has `prefix`.
    /// `Some(12)` for `launch_12_name` and `launch_`.
    pub fn entry_number(&self, prefix: &str, key: &str) -> Option<u64> {
        let family = self
            .family_of(key)
            .filter(|family| family.is_numbered() && family.prefix() == prefix)?;
        let length = *placeholder_lengths(family.pattern, key)?.first()?;
        key[prefix.len()..prefix.len() + length].parse().ok()
    }

    /// The group a key is listed under: its option's, its family's, or that
    /// of the family it was likely meant for (see `misfit_family`). `None`
    /// for a key that is none of these, which the screen lists under
    /// "Other".
    pub fn group_of(&self, key: &str) -> Option<&'static str> {
        self.find(key)
            .map(|option| option.group)
            .or_else(|| self.family_of(key).map(|family| family.group))
            .or_else(|| self.misfit_family(key).map(|family| family.group))
    }

    /// The kind of value a key takes: its option's, or its family's, or text.
    pub fn kind_of(&self, key: &str) -> Kind {
        self.find(key)
            .map(|option| option.kind)
            .or_else(|| self.family_of(key).map(|family| family.kind))
            .unwrap_or(Kind::Text)
    }
}

/// Whether `key` fits a family's `pattern`: the text outside the
/// placeholders as written, and each placeholder standing for at least one
/// character. `<mode>` stands for one of `MODE_NAMES` only, and `<n>` for a
/// whole number as zjtaskbar's launcher reads one: digits alone, with no
/// leading zero unless the number is `0`, and small enough for a `u64`. Any
/// other placeholder stands for anything.
#[cfg(test)]
fn fits_pattern(pattern: &str, key: &str) -> bool {
    placeholder_lengths(pattern, key).is_some()
}

/// How many bytes of `key` each placeholder of `pattern` stands for, in
/// order, when `key` fits it (see `fits_pattern`), or `None` when it does
/// not. Where a key fits in more than one way, each placeholder takes as
/// little as it can, the first before the second and so on: `a_b_c` fits
/// `<x>_<y>` as `a` and `b_c`.
fn placeholder_lengths(pattern: &str, key: &str) -> Option<Vec<usize>> {
    let Some((literal, rest)) = pattern.split_once('<') else {
        return (key == pattern).then(Vec::new);
    };
    let key = key.strip_prefix(literal)?;
    // An unclosed placeholder is a typo in the plugin's own schema.
    let (placeholder, pattern_after) = rest.split_once('>')?;
    // Every length that leaves the placeholder at least one character,
    // shortest first, so the text after it can be found wherever it falls.
    // No mode name begins another, so a `<mode>` placeholder takes at most
    // one of them.
    let lengths: Vec<usize> = if placeholder == "mode" {
        MODE_NAMES
            .iter()
            .filter(|mode| key.starts_with(*mode))
            .map(|mode| mode.len())
            .collect()
    } else if placeholder == "n" {
        let digits = key.bytes().take_while(u8::is_ascii_digit).count();
        (1..=digits)
            .filter(|&length| is_whole_number(&key[..length]))
            .collect()
    } else {
        key.char_indices()
            .skip(1)
            .map(|(index, _)| index)
            .chain([key.len()])
            .filter(|index| *index > 0)
            .collect()
    };
    lengths.into_iter().find_map(|length| {
        let mut lengths = vec![length];
        lengths.extend(placeholder_lengths(pattern_after, &key[length..])?);
        Some(lengths)
    })
}

/// Whether `digits`, all ASCII digits, spell a whole number the way
/// zjtaskbar's launcher takes an entry's number: no leading zero unless the
/// number is `0` itself, and no larger than a `u64` holds.
fn is_whole_number(digits: &str) -> bool {
    let leading_zero = digits.len() > 1 && digits.starts_with('0');
    !leading_zero && digits.parse::<u64>().is_ok()
}

/// How many characters of `pattern` are written out rather than
/// placeholders, which is how much of a key fitting it the pattern names.
fn literal_length(pattern: &str) -> usize {
    let mut length = 0;
    let mut in_placeholder = false;
    for character in pattern.chars() {
        match character {
            '<' => in_placeholder = true,
            '>' => in_placeholder = false,
            _ if !in_placeholder => length += 1,
            _ => {}
        }
    }
    length
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

    #[test]
    fn a_key_fits_a_pattern_with_its_text_as_written_and_a_placeholder_for_the_rest() {
        assert!(fits_pattern("color_<name>", "color_blue"));
        assert!(!fits_pattern("color_<name>", "color_"));
        assert!(!fits_pattern("color_<name>", "colour_blue"));
        assert!(fits_pattern("launch_<n>_name", "launch_12_name"));
        assert!(!fits_pattern("launch_<n>_name", "launch__name"));
        assert!(!fits_pattern("launch_<n>_name", "launch_1_names"));
        assert!(fits_pattern("launch_<n>_name", "launch_0_name"));
        assert!(fits_pattern(
            "launch_<n>_config_<key>",
            "launch_1_config_a_b"
        ));
        assert!(!fits_pattern("launch_<n>_config_<key>", "launch_1_config_"));
        assert!(fits_pattern("plain", "plain"));
        assert!(!fits_pattern("plain", "plainer"));
    }

    #[test]
    fn a_mode_placeholder_takes_only_a_mode_name() {
        assert!(fits_pattern("mode_format_<mode>", "mode_format_locked"));
        assert!(fits_pattern(
            "mode_format_<mode>",
            "mode_format_enter_search"
        ));
        assert!(!fits_pattern("mode_format_<mode>", "mode_format_bogus"));
        assert!(!fits_pattern("mode_format_<mode>", "mode_format_locked_x"));
        assert!(fits_pattern(
            "label_<mode>_<id>",
            "label_locked_mode_normal"
        ));
        assert!(!fits_pattern("label_<mode>_<id>", "label_locked_"));
        assert!(!fits_pattern("label_<mode>_<id>", "label_split_down"));
    }

    #[test]
    fn a_number_placeholder_takes_only_a_whole_number_without_leading_zeros() {
        for key in [
            "launch_01_name",
            "launch_00_name",
            "launch_x_name",
            "launch_1x_name",
            "launch_-1_name",
            "launch_+1_name",
            "launch_1_2_name",
            "launch_\u{0661}_name",
            // One more than `u64::MAX`.
            "launch_18446744073709551616_name",
        ] {
            assert!(!fits_pattern("launch_<n>_name", key), "{key}");
        }
        assert!(fits_pattern(
            "launch_<n>_name",
            "launch_18446744073709551615_name"
        ));
        assert!(fits_pattern("item_<n>", "item_10"));
        assert!(!fits_pattern("item_<n>", "item_10a"));
    }

    fn no_values(_: &str, _: &Configuration) -> Vec<(&'static str, String)> {
        vec![]
    }

    fn no_bar(_: Configuration, _: &ModeInfo, _: usize) -> String {
        String::new()
    }

    static FAMILIES: [FamilySpec; 4] = [
        family("label_<id>", "Labels", Kind::Text, "label"),
        family("label_<mode>_<id>", "Labels", Kind::Text, "mode label"),
        family("launch_<n>_name", "Launcher", Kind::Text, "name"),
        family(
            "launch_<n>_config_<key>",
            "Launcher",
            Kind::Format,
            "config",
        ),
    ];

    static FAMILY_SCHEMA: Schema = Schema {
        plugin: "test",
        groups: &["Labels", "Launcher"],
        options: &[],
        families: &FAMILIES,
        screen_options: &[],
        internal_options: &[],
        live_message: "test::configure",
        preview_group: None,
        permissions: Vec::new,
        layer: |configuration, _| configuration.clone(),
        sample_values: no_values,
        draw_bar: no_bar,
    };

    #[test]
    fn a_key_fitting_several_families_is_taken_as_the_most_specific() {
        let description = |key: &str| FAMILY_SCHEMA.family_of(key).map(|f| f.description);
        assert_eq!(description("label_quit"), Some("label"));
        assert_eq!(description("label_locked_quit"), Some("mode label"));
        assert_eq!(description("label_locked"), Some("label"));
        assert_eq!(description("launch_1_name"), Some("name"));
        assert_eq!(description("launch_1_config_name"), Some("config"));
        assert_eq!(FAMILY_SCHEMA.kind_of("launch_1_config_name"), Kind::Format);
        assert_eq!(description("launch_1_other"), None);
        assert_eq!(description("launch_01_name"), None);
        assert_eq!(description("launch_a_config_name"), None);
    }

    #[test]
    fn a_key_starting_like_a_family_s_but_fitting_none_is_listed_with_it() {
        let misfit = |key: &str| FAMILY_SCHEMA.misfit_family(key).map(|f| f.description);
        // Each starts with `launch_`, the prefix of both launcher families,
        // and the first declared is taken.
        for key in ["launch_1_other", "launch_x_name", "launch_1", "launch_"] {
            assert_eq!(misfit(key), Some("name"), "{key}");
            assert_eq!(FAMILY_SCHEMA.group_of(key), Some("Launcher"), "{key}");
        }
        assert_eq!(misfit("label_"), Some("label"));
        // A key fitting a family, or starting with no family's prefix, is
        // no misfit.
        assert_eq!(misfit("launch_1_name"), None);
        assert_eq!(misfit("launcher"), None);
        assert_eq!(misfit("other"), None);
        assert_eq!(FAMILY_SCHEMA.group_of("other"), None);
    }

    static NESTED_PREFIXES: [FamilySpec; 2] = [
        family("item_<n>", "Items", Kind::Text, "item"),
        family("item_extra_<id>", "Items", Kind::Text, "extra"),
    ];

    #[test]
    fn a_misfit_is_taken_for_the_family_with_the_longest_prefix_it_starts_with() {
        let schema = schema_of(&NESTED_PREFIXES);
        let misfit = |key: &str| schema.misfit_family(key).map(|f| f.description);
        assert_eq!(misfit("item_extra_"), Some("extra"));
        assert_eq!(misfit("item_extra"), Some("item"));
        assert_eq!(misfit("item_x"), Some("item"));
        assert_eq!(misfit("item_1"), None);
        assert_eq!(misfit("item_extra_x"), None);
    }

    #[test]
    fn the_families_sharing_a_prefix_are_named_by_what_their_patterns_share() {
        assert_eq!(
            FAMILY_SCHEMA.patterns_with_prefix("launch_"),
            "launch_<n>_*"
        );
        assert_eq!(FAMILY_SCHEMA.patterns_with_prefix("label_"), "label_*");
        let schema = schema_of(&NESTED_PREFIXES);
        assert_eq!(
            schema.patterns_with_prefix("item_extra_"),
            "item_extra_<id>"
        );
    }

    #[test]
    fn an_entry_s_number_is_what_its_family_s_number_placeholder_stands_for() {
        let number = |key: &str| FAMILY_SCHEMA.entry_number("launch_", key);
        assert_eq!(number("launch_12_name"), Some(12));
        assert_eq!(number("launch_0_config_name"), Some(0));
        assert_eq!(number("launch_01_name"), None);
        assert_eq!(number("launch_1_other"), None);
        assert_eq!(FAMILY_SCHEMA.entry_number("label_", "label_1"), None);
        assert!(FAMILIES[2].is_numbered());
        assert!(!FAMILIES[0].is_numbered());
    }

    #[test]
    fn a_numbered_key_starts_with_the_entry_s_number_filled_in() {
        assert_eq!(entry_key_start("launch_<n>_name", 3), "launch_3_name");
        assert_eq!(
            entry_key_start("launch_<n>_config_<key>", 12),
            "launch_12_config_"
        );
    }

    #[test]
    fn a_key_typed_up_to_a_placeholder_is_unfinished() {
        assert!(is_unfinished("color_<name>", "color_"));
        assert!(is_unfinished("launch_<n>_config_<key>", "launch_"));
        assert!(is_unfinished("launch_<n>_config_<key>", "launch_1_config_"));
        assert!(!is_unfinished(
            "launch_<n>_config_<key>",
            "launch_1_config_a"
        ));
        assert!(!is_unfinished("launch_<n>_config_<key>", "launch_1_conf"));
        assert!(!is_unfinished("launch_<n>_name", "launch_1_name"));
        assert!(!is_unfinished("plain", "plain"));
    }

    #[test]
    fn a_placeholder_takes_as_little_as_it_can_from_the_left() {
        assert_eq!(placeholder_lengths("<x>_<y>", "a_b_c"), Some(vec![1, 3]));
        assert_eq!(
            placeholder_lengths("item_<id>_command", "item_1_config_command"),
            Some(vec![8])
        );
        assert_eq!(
            placeholder_lengths("label_<mode>_<id>", "label_rename_tab_quit"),
            Some(vec![10, 4])
        );
        assert_eq!(placeholder_lengths("plain", "plain"), Some(vec![]));
        assert_eq!(placeholder_lengths("color_<name>", "color_"), None);
    }

    /// A schema of `families` alone, to sort keys with.
    fn schema_of(families: &'static [FamilySpec]) -> Schema {
        Schema {
            families,
            ..FAMILY_SCHEMA
        }
    }

    static COMMAND_THEN_CONFIG: [FamilySpec; 2] = [
        family("item_<id>_command", "Items", Kind::Text, "command"),
        family("item_<id>_config_<key>", "Items", Kind::Format, "config"),
    ];

    static CONFIG_THEN_COMMAND: [FamilySpec; 2] = [
        family("item_<id>_config_<key>", "Items", Kind::Format, "config"),
        family("item_<id>_command", "Items", Kind::Text, "command"),
    ];

    static TWO_NAMES_IN_ONE_ORDER: [FamilySpec; 2] = [
        family("thing_<a>", "Items", Kind::Text, "a"),
        family("thing_<b>", "Items", Kind::Text, "b"),
    ];

    static TWO_NAMES_IN_THE_OTHER: [FamilySpec; 2] = [
        family("thing_<b>", "Items", Kind::Text, "b"),
        family("thing_<a>", "Items", Kind::Text, "a"),
    ];

    #[test]
    fn families_that_spell_out_as_much_are_told_apart_the_same_in_either_order() {
        for families in [&COMMAND_THEN_CONFIG, &CONFIG_THEN_COMMAND] {
            let schema = schema_of(families);
            let description = |key: &str| schema.family_of(key).map(|f| f.description);
            // Both spell out 13 characters; `<id>` is `1` for the config
            // family and `1_config` for the command one.
            assert_eq!(description("item_1_config_command"), Some("config"));
            assert_eq!(description("item_1_command"), Some("command"));
            assert_eq!(description("item_1_config_name"), Some("config"));
        }
        for families in [&TWO_NAMES_IN_ONE_ORDER, &TWO_NAMES_IN_THE_OTHER] {
            let schema = schema_of(families);
            assert_eq!(
                schema.family_of("thing_x").map(|f| f.description),
                Some("a")
            );
        }
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
