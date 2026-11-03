//! The launcher: a button at the bar's left end whose menu opens new
//! floating panes, the way a desktop taskbar's start button opens programs.
//!
//! A left click on the button opens a menu at the mouse, listing in order:
//!
//! 1. a new terminal, unless `launcher_new_terminal` is off;
//! 2. the user's own entries, each a command or a plugin, in the order of
//!    their numbers;
//! 3. Zellij's session manager, settings and plugin manager, unless
//!    `launcher_zellij_actions` is off.
//!
//! Choosing an item opens it; Escape or a click outside the menu closes it
//! and opens nothing. The menu is a Zellij prompt, like the pane menu, and
//! shares its slot in `State::pending_prompt`, so opening one closes the
//! other's wait.
//!
//! # The user's entries
//!
//! Zellij hands a plugin its configuration as a flat map of string keys to
//! string values, sorted by key, and keeps only the first value of a node.
//! A nested block arrives as one key holding its raw text. So each entry
//! is a set of flat keys sharing a number:
//!
//! - `launch_<n>_name`, what the menu shows;
//! - `launch_<n>_command`, the program to run, or `launch_<n>_plugin`, the
//!   plugin to open, by URL or alias. Exactly one of the two says what kind
//!   of entry it is;
//! - `launch_<n>_args`, the command's arguments in one string, split as
//!   `split_arguments` describes;
//! - `launch_<n>_cwd`, the directory the command runs in;
//! - `launch_<n>_config_<key>`, one key of the plugin's configuration.
//!
//! Every key is a plain string, so the settings screen lists and saves each
//! one as it does any other option. A new kind of entry later takes a new
//! key of its own in place of `command` or `plugin`, and the keys of the
//! kinds already here keep their names. `name` and `cwd` stand behind the
//! prefix because Zellij takes them out of a plugin's configuration for
//! itself.
//!
//! An entry missing its name, with neither or both of `command` and
//! `plugin`, or with arguments that do not split, is left off the menu
//! rather than failing the bar. A key the bar does not know is ignored, so
//! a newer bar's keys do no harm to an older one.

use crate::State;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::PathBuf;
use zellij_tile::prelude::*;
use zjkit::ansi::show_controls;
use zjkit::host::Host;
use zjkit::launch::{
    launch_or_focus_floating, new_floating_terminal, CONFIGURATION_ALIAS, PLUGIN_MANAGER_ALIAS,
    SESSION_MANAGER_ALIAS,
};
use zjkit::mouse::labeled_menu_request;

/// What every key of a user's entry starts with, before its number.
const LAUNCH_KEY_PREFIX: &str = "launch_";
/// What a key of the plugin configuration starts with, after the number.
const CONFIG_FIELD_PREFIX: &str = "config_";

/// One of the user's own entries, from its `launch_<n>_*` keys.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LaunchEntry {
    /// What the menu shows for it.
    pub name: String,
    pub target: LaunchTarget,
}

/// What a user's entry opens.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LaunchTarget {
    /// A command in a new floating pane, which Zellij keeps open once the
    /// command ends, so its output can still be read.
    Command {
        /// The program, found on the `PATH` as a shell would.
        program: String,
        arguments: Vec<String>,
        /// Where the command runs, or `None` for the working directory of
        /// the pane focused when it opens.
        cwd: Option<PathBuf>,
    },
    /// A plugin in a new floating pane.
    Plugin {
        /// A URL such as `zellij:strider` or `file:/path/to/plugin.wasm`,
        /// or an alias from Zellij's `plugins` block.
        url: String,
        configuration: BTreeMap<String, String>,
    },
}

/// One of Zellij's own plugins the menu offers, opened as its default
/// session-mode key opens it. See `zjkit::launch`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ZellijPlugin {
    /// What the menu shows for it.
    pub label: &'static str,
    /// The alias Zellij's default config gives it.
    pub alias: &'static str,
}

/// Zellij's own plugins, in the order the menu lists them: the session
/// manager, the settings screen and the plugin manager, in the order of
/// their keys in Zellij's own session mode.
pub(crate) const ZELLIJ_PLUGINS: [ZellijPlugin; 3] = [
    ZellijPlugin {
        label: "Session manager",
        alias: SESSION_MANAGER_ALIAS,
    },
    ZellijPlugin {
        label: "Settings",
        alias: CONFIGURATION_ALIAS,
    },
    ZellijPlugin {
        label: "Plugin manager",
        alias: PLUGIN_MANAGER_ALIAS,
    },
];

/// One item of the launcher's menu.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LauncherItem {
    /// A new floating terminal running the user's shell.
    NewTerminal,
    /// One of the user's own entries.
    Entry(LaunchEntry),
    /// One of Zellij's own plugins.
    ZellijPlugin(ZellijPlugin),
}

impl LauncherItem {
    /// How the item is named in the menu. A user's name is shown with its
    /// control characters escaped, as a pane's title is.
    pub(crate) fn label(&self) -> Cow<'static, str> {
        match self {
            LauncherItem::NewTerminal => Cow::Borrowed("New terminal"),
            LauncherItem::Entry(entry) => Cow::Owned(show_controls(&entry.name).into_owned()),
            LauncherItem::ZellijPlugin(plugin) => Cow::Borrowed(plugin.label),
        }
    }
}

/// The user's entries in `configuration`, in the order of their numbers,
/// leaving out each that is not whole. See the module documentation for
/// the keys.
///
/// The number is a whole number written without leading zeros, so `2`
/// comes before `10`, and a key such as `launch_01_name` is not an entry's
/// at all. Numbers need not run on from each other: a gap left by an entry
/// taken out changes nothing.
pub(crate) fn launch_entries(configuration: &BTreeMap<String, String>) -> Vec<LaunchEntry> {
    let mut fields_by_number: BTreeMap<u64, BTreeMap<&str, &str>> = BTreeMap::new();
    for (key, value) in configuration {
        if let Some((number, field)) = launch_key(key) {
            fields_by_number
                .entry(number)
                .or_default()
                .insert(field, value);
        }
    }
    fields_by_number.values().filter_map(entry_of).collect()
}

/// The number and the field of a user's entry that `key` names, as `1` and
/// `name` for `launch_1_name`, or `None` for any other key.
fn launch_key(key: &str) -> Option<(u64, &str)> {
    let rest = key.strip_prefix(LAUNCH_KEY_PREFIX)?;
    let (number, field) = rest.split_once('_')?;
    let all_digits = !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit());
    let leading_zero = number.len() > 1 && number.starts_with('0');
    if !all_digits || leading_zero || field.is_empty() {
        return None;
    }
    // A number too large for `u64` names no entry either.
    Some((number.parse().ok()?, field))
}

/// The entry `fields` make, keyed by field name, or `None` when they do not
/// make a whole one.
fn entry_of(fields: &BTreeMap<&str, &str>) -> Option<LaunchEntry> {
    let field = |name: &str| {
        fields
            .get(name)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
    };
    let name = fields
        .get("name")
        .filter(|name| !name.trim().is_empty())?
        .to_string();
    let target = match (field("command"), field("plugin")) {
        (Some(program), None) => LaunchTarget::Command {
            program: program.to_string(),
            arguments: split_arguments(fields.get("args").copied().unwrap_or_default())?,
            cwd: field("cwd").map(PathBuf::from),
        },
        (None, Some(url)) => LaunchTarget::Plugin {
            url: url.to_string(),
            configuration: fields
                .iter()
                .filter_map(|(field, value)| {
                    let key = field.strip_prefix(CONFIG_FIELD_PREFIX)?;
                    (!key.is_empty()).then(|| (key.to_string(), value.to_string()))
                })
                .collect(),
        },
        // Neither says what the entry opens, or both do and it is unclear
        // which was meant.
        _ => return None,
    };
    Some(LaunchEntry { name, target })
}

/// Which quotes the splitter is inside.
#[derive(Clone, Copy, PartialEq)]
enum Quote {
    Single,
    Double,
}

/// `text` split into a command's arguments the way a shell splits them,
/// without expanding anything, or `None` when a quote is never closed or the
/// text ends in a lone backslash.
///
/// - Whitespace outside quotes separates arguments, however much of it.
/// - Inside single quotes every character is taken as written.
/// - Inside double quotes a backslash escapes a `"` or another backslash,
///   and is taken as written before anything else.
/// - Outside quotes a backslash takes the character after it as written,
///   such as a space or a quote.
/// - Quoted text runs on into the text around it, so `--name="a b"` is one
///   argument, and `""` is an empty one.
///
/// There are no variables, globs, `~` or pipes: the program is started
/// directly, not through a shell. Run `sh -c '...'` to have one.
pub(crate) fn split_arguments(text: &str) -> Option<Vec<String>> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    // Whether an argument has started, which an empty pair of quotes does
    // without adding a character.
    let mut started = false;
    let mut quote = None;
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        match quote {
            Some(Quote::Single) => match character {
                '\'' => quote = None,
                _ => current.push(character),
            },
            Some(Quote::Double) => match character {
                '"' => quote = None,
                '\\' => match characters.next()? {
                    escaped @ ('"' | '\\') => current.push(escaped),
                    other => {
                        current.push('\\');
                        current.push(other);
                    }
                },
                _ => current.push(character),
            },
            None => match character {
                _ if character.is_whitespace() => {
                    if started {
                        arguments.push(std::mem::take(&mut current));
                        started = false;
                    }
                }
                '\'' => {
                    quote = Some(Quote::Single);
                    started = true;
                }
                '"' => {
                    quote = Some(Quote::Double);
                    started = true;
                }
                '\\' => {
                    current.push(characters.next()?);
                    started = true;
                }
                _ => {
                    current.push(character);
                    started = true;
                }
            },
        }
    }
    if quote.is_some() {
        return None;
    }
    if started {
        arguments.push(current);
    }
    Some(arguments)
}

impl State {
    /// The launcher's menu as the options stand, in the order it lists the
    /// items. See the module documentation.
    pub(crate) fn launcher_items(&self) -> Vec<LauncherItem> {
        let mut items = Vec::new();
        if self.launcher_new_terminal {
            items.push(LauncherItem::NewTerminal);
        }
        items.extend(self.launch_entries.iter().cloned().map(LauncherItem::Entry));
        if self.launcher_zellij_actions {
            items.extend(ZELLIJ_PLUGINS.map(LauncherItem::ZellijPlugin));
        }
        items
    }

    /// Whether the bar draws the launcher's button: `show_launcher` is on,
    /// the mouse is, since the button does nothing but take a click, and the
    /// menu would list something.
    pub(crate) fn launcher_shown(&self) -> bool {
        self.show_launcher
            && self.mouse
            && (self.launcher_new_terminal
                || self.launcher_zellij_actions
                || !self.launch_entries.is_empty())
    }

    /// Whether the launcher's menu may run one of the user's commands.
    pub(crate) fn launcher_runs_commands(&self) -> bool {
        self.launcher_shown()
            && self
                .launch_entries
                .iter()
                .any(|entry| matches!(entry.target, LaunchTarget::Command { .. }))
    }

    /// Whether the launcher's menu may run one of Zellij's own actions: a new
    /// terminal, or opening one of `ZELLIJ_PLUGINS`.
    pub(crate) fn launcher_runs_zellij_actions(&self) -> bool {
        self.launcher_shown() && (self.launcher_new_terminal || self.launcher_zellij_actions)
    }

    /// Open the launcher's menu at the mouse, and wait for the answer. See
    /// `answer_prompt`.
    pub(crate) fn open_launcher_menu(&mut self) {
        let items = self.launcher_items();
        if items.is_empty() {
            return;
        }
        let labels = items.iter().map(|item| item.label().into_owned());
        let request_id = self.host.prompt(labeled_menu_request(labels));
        self.pending_prompt
            .wait_for(request_id, crate::mouse::Prompted::Launcher { items });
    }

    /// Open what `item` names, as the menu showed it when it opened.
    ///
    /// A new pane opens floating and focused, wherever Zellij finds room in
    /// the tab the user is on. A command or a new terminal with no directory
    /// of its own starts in the one the focused pane is working in, as a pane
    /// from Zellij's own keys does.
    pub(crate) fn launch(&self, item: &LauncherItem) {
        let context = BTreeMap::new();
        match item {
            LauncherItem::NewTerminal => self.host.run_action(new_floating_terminal(), context),
            LauncherItem::Entry(entry) => match &entry.target {
                LaunchTarget::Command {
                    program,
                    arguments,
                    cwd,
                } => {
                    let command = CommandToRun {
                        path: PathBuf::from(program),
                        args: arguments.clone(),
                        cwd: cwd.clone(),
                        border_style: None,
                    };
                    self.host.open_command_pane_floating(command, None, context);
                }
                LaunchTarget::Plugin { url, configuration } => {
                    self.host
                        .open_plugin_pane_floating(url, configuration.clone(), None, context);
                }
            },
            LauncherItem::ZellijPlugin(plugin) => self
                .host
                .run_action(launch_or_focus_floating(plugin.alias), context),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configuration(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn names(entries: &[LaunchEntry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.name.as_str()).collect()
    }

    fn command(program: &str, arguments: &[&str], cwd: Option<&str>) -> LaunchTarget {
        LaunchTarget::Command {
            program: program.to_string(),
            arguments: arguments
                .iter()
                .map(|argument| argument.to_string())
                .collect(),
            cwd: cwd.map(PathBuf::from),
        }
    }

    #[test]
    fn entries_are_listed_in_the_order_of_their_numbers_gaps_and_all() {
        // Zellij sorts the keys as text, so 10 comes before 2 there.
        let entries = launch_entries(&configuration(&[
            ("launch_10_name", "ten"),
            ("launch_10_command", "top"),
            ("launch_2_name", "two"),
            ("launch_2_plugin", "zellij:strider"),
            ("launch_7_name", "seven"),
            ("launch_7_command", "htop"),
            ("launch_0_name", "zero"),
            ("launch_0_command", "true"),
        ]));
        assert_eq!(names(&entries), ["zero", "two", "seven", "ten"]);
    }

    #[test]
    fn an_entry_that_is_not_whole_is_left_off_and_the_rest_are_kept() {
        let entries = launch_entries(&configuration(&[
            ("launch_1_name", "kept"),
            ("launch_1_command", "htop"),
            // No name.
            ("launch_2_command", "htop"),
            // A name of only spaces.
            ("launch_3_name", "  "),
            ("launch_3_command", "htop"),
            // Neither a command nor a plugin.
            ("launch_4_name", "nothing"),
            ("launch_4_args", "-x"),
            // Both, so which is meant is unclear.
            ("launch_5_name", "both"),
            ("launch_5_command", "htop"),
            ("launch_5_plugin", "zellij:strider"),
            // A command of only spaces is none.
            ("launch_6_name", "blank"),
            ("launch_6_command", " "),
            // Arguments that do not split.
            ("launch_7_name", "unclosed"),
            ("launch_7_command", "echo"),
            ("launch_7_args", "'oops"),
            // Keys that name no entry: a leading zero, a number that is not
            // one, a number too large, and no field.
            ("launch_01_name", "leading zero"),
            ("launch_01_command", "htop"),
            ("launch_x_name", "letters"),
            ("launch_x_command", "htop"),
            ("launch_99999999999999999999_name", "too large"),
            ("launch_99999999999999999999_command", "htop"),
            ("launch_8_", "no field"),
            ("launch__name", "no number"),
            ("launch_9_name", "also kept"),
            ("launch_9_plugin", "zellij:strider"),
            // A field the bar does not know is ignored.
            ("launch_9_icon", "folder"),
        ]));
        assert_eq!(names(&entries), ["kept", "also kept"]);
    }

    #[test]
    fn a_command_entry_takes_its_program_arguments_and_directory() {
        let entries = launch_entries(&configuration(&[
            ("launch_1_name", "tests"),
            ("launch_1_command", " cargo "),
            ("launch_1_args", "test --workspace -- \"a b\""),
            ("launch_1_cwd", "/home/user/project"),
            ("launch_2_name", "top"),
            ("launch_2_command", "htop"),
            // An empty directory is none, the focused pane's.
            ("launch_2_cwd", ""),
            ("launch_2_args", ""),
        ]));
        assert_eq!(
            entries,
            [
                LaunchEntry {
                    name: "tests".to_string(),
                    target: command(
                        "cargo",
                        &["test", "--workspace", "--", "a b"],
                        Some("/home/user/project"),
                    ),
                },
                LaunchEntry {
                    name: "top".to_string(),
                    target: command("htop", &[], None),
                },
            ]
        );
    }

    #[test]
    fn a_plugin_entry_takes_its_config_keys_as_its_configuration() {
        let entries = launch_entries(&configuration(&[
            ("launch_1_name", "sessions"),
            ("launch_1_plugin", "zellij:session-manager"),
            ("launch_1_config_welcome_screen", "true"),
            // A plugin key the bar would read as an entry's field is still
            // the plugin's under `config_`.
            ("launch_1_config_name", "kept"),
            // No key after the prefix.
            ("launch_1_config_", "dropped"),
            // A command's fields mean nothing to a plugin.
            ("launch_1_args", "-x"),
        ]));
        assert_eq!(
            entries,
            [LaunchEntry {
                name: "sessions".to_string(),
                target: LaunchTarget::Plugin {
                    url: "zellij:session-manager".to_string(),
                    configuration: configuration(&[("name", "kept"), ("welcome_screen", "true"),]),
                },
            }]
        );
    }

    #[test]
    fn a_name_is_shown_with_its_control_characters_escaped() {
        let entry = LaunchEntry {
            name: "a\u{1b}[31mb".to_string(),
            target: command("htop", &[], None),
        };
        assert_eq!(LauncherItem::Entry(entry).label(), "a\\u{1b}[31mb");
    }

    #[test]
    fn arguments_split_as_a_shell_splits_them_with_nothing_expanded() {
        for (text, expected) in [
            ("", &[][..]),
            ("   ", &[]),
            ("a", &["a"]),
            ("  a \t b\n", &["a", "b"]),
            ("'a b' c", &["a b", "c"]),
            ("\"a b\" c", &["a b", "c"]),
            ("--name=\"a b\"", &["--name=a b"]),
            ("'it''s'", &["its"]),
            ("\"it's\"", &["it's"]),
            ("'say \"hi\"'", &["say \"hi\""]),
            ("\"\" ''", &["", ""]),
            ("a\\ b", &["a b"]),
            ("\\'a", &["'a"]),
            ("\"a\\\"b\"", &["a\"b"]),
            ("\"a\\\\b\"", &["a\\b"]),
            ("\"a\\nb\"", &["a\\nb"]),
            ("'a\\b'", &["a\\b"]),
            ("$HOME ~ *.rs", &["$HOME", "~", "*.rs"]),
            ("sh -c 'ls | wc -l'", &["sh", "-c", "ls | wc -l"]),
        ] {
            let expected: Vec<String> = expected
                .iter()
                .map(|argument| argument.to_string())
                .collect();
            assert_eq!(split_arguments(text), Some(expected), "{text:?}");
        }
    }

    #[test]
    fn arguments_with_a_quote_left_open_or_a_lone_backslash_at_the_end_do_not_split() {
        for text in ["'a", "\"a", "a 'b c", "a\\", "\"a\\"] {
            assert_eq!(split_arguments(text), None, "{text:?}");
        }
    }
}
