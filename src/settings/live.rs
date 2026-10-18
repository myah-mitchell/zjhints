//! Previewing edits on the running bars.
//!
//! The settings screen broadcasts every change as a pipe message to all
//! plugins in the session; each bar keeps the ones meant for it. A broadcast
//! launches nothing, unlike a message addressed by plugin URL, which starts a
//! new instance when no running one matches its configuration exactly.
//!
//! A message names the alias it edits and carries, for each key it changes,
//! the value the alias had before and the value it has now. The first time a
//! key is previewed, a bar follows it only if its own value is the alias's
//! value before the change, so a key the layout sets for that bar (such as
//! `show_mode true` on one pane) is left alone, however often it is changed
//! afterwards.

use std::collections::{BTreeMap, BTreeSet};

/// The name of the pipe message a preview is sent as.
pub const MESSAGE_NAME: &str = "zjhints::configure";

const ALIAS_ARG: &str = "alias";
const WAS_PREFIX: &str = "was:";
const NOW_PREFIX: &str = "now:";

/// One key's change: its value before and after, `None` meaning unset.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub key: String,
    pub was: Option<String>,
    pub now: Option<String>,
}

/// The changes between two versions of an alias's configuration.
pub fn changes(was: &BTreeMap<String, String>, now: &BTreeMap<String, String>) -> Vec<Change> {
    let mut keys: Vec<&String> = was.keys().chain(now.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter(|key| was.get(*key) != now.get(*key))
        .map(|key| Change {
            key: key.clone(),
            was: was.get(key).cloned(),
            now: now.get(key).cloned(),
        })
        .collect()
}

/// The pipe arguments carrying `changes` to the bars of `alias`.
pub fn encode(alias: &str, changes: &[Change]) -> BTreeMap<String, String> {
    let mut args = BTreeMap::from([(ALIAS_ARG.to_string(), alias.to_string())]);
    for change in changes {
        // An unset value is a missing argument, so "was set to empty" and
        // "was unset" stay distinct.
        if let Some(was) = &change.was {
            args.insert(format!("{WAS_PREFIX}{}", change.key), was.clone());
        }
        if let Some(now) = &change.now {
            args.insert(format!("{NOW_PREFIX}{}", change.key), now.clone());
        }
    }
    args
}

/// The alias and changes `args` carry, or `None` when they are not a preview.
pub fn decode(args: &BTreeMap<String, String>) -> Option<(String, Vec<Change>)> {
    let alias = args.get(ALIAS_ARG)?.clone();
    let keys: BTreeSet<&str> = args
        .keys()
        .filter_map(|arg| {
            arg.strip_prefix(WAS_PREFIX)
                .or_else(|| arg.strip_prefix(NOW_PREFIX))
        })
        .collect();
    let changes = keys
        .into_iter()
        .filter(|key| !key.is_empty())
        .map(|key| Change {
            key: key.to_string(),
            was: args.get(&format!("{WAS_PREFIX}{key}")).cloned(),
            now: args.get(&format!("{NOW_PREFIX}{key}")).cloned(),
        })
        .collect();
    Some((alias, changes))
}

/// Apply to `configuration` the changes to keys it follows the alias in.
/// A key not seen before is followed when its value is the change's `was`;
/// one that is not goes into `ignored` and stays there. Returns whether
/// anything was applied.
pub fn apply(
    configuration: &mut BTreeMap<String, String>,
    ignored: &mut BTreeSet<String>,
    changes: &[Change],
) -> bool {
    let mut applied = false;
    for change in changes {
        if ignored.contains(&change.key) {
            continue;
        }
        if configuration.get(&change.key) != change.was.as_ref() {
            ignored.insert(change.key.clone());
            continue;
        }
        match &change.now {
            Some(now) => configuration.insert(change.key.clone(), now.clone()),
            None => configuration.remove(&change.key),
        };
        applied = true;
    }
    applied
}

/// Whether a bar running as `own_url` takes previews of `alias`. A bar
/// started from the alias reports the alias's name as its URL, whether its
/// pane is visible or, loaded to feed zjstatus, hidden. A bar whose URL is
/// not known yet, before Zellij first reports the panes, takes every preview.
pub fn is_for(alias: &str, own_url: Option<&str>) -> bool {
    own_url.is_none_or(|url| url == alias)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn changes_cover_set_changed_and_unset_keys() {
        let was = map(&[("a", "1"), ("b", "2"), ("c", "3")]);
        let now = map(&[("a", "1"), ("b", "20"), ("d", "")]);
        let changes = changes(&was, &now);
        let summary: Vec<(&str, Option<&str>, Option<&str>)> = changes
            .iter()
            .map(|change| {
                (
                    change.key.as_str(),
                    change.was.as_deref(),
                    change.now.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("b", Some("2"), Some("20")),
                ("c", Some("3"), None),
                ("d", None, Some("")),
            ]
        );
    }

    #[test]
    fn changes_survive_the_trip_through_pipe_arguments() {
        let changes = changes(
            &map(&[("show_mode", "true"), ("label_quit", "")]),
            &map(&[("label_quit", "bye"), ("mouse", "true")]),
        );
        let (alias, decoded) = decode(&encode("zjhints", &changes)).unwrap();
        assert_eq!(alias, "zjhints");
        assert_eq!(decoded, changes);
    }

    #[test]
    fn other_messages_are_not_previews() {
        assert_eq!(decode(&map(&[("now:mouse", "true")])), None);
    }

    #[test]
    fn a_change_applies_only_over_the_value_it_replaces() {
        // The layout gave this bar show_mode true; the alias leaves it unset.
        let mut configuration = map(&[("show_mode", "true"), ("mouse", "false")]);
        let changes = vec![
            Change {
                key: "show_mode".to_string(),
                was: None,
                now: Some("false".to_string()),
            },
            Change {
                key: "mouse".to_string(),
                was: Some("false".to_string()),
                now: Some("true".to_string()),
            },
        ];
        assert!(apply(&mut configuration, &mut BTreeSet::new(), &changes));
        assert_eq!(
            configuration,
            map(&[("show_mode", "true"), ("mouse", "true")])
        );
    }

    #[test]
    fn an_unset_change_removes_the_key() {
        let mut configuration = map(&[("mouse", "true")]);
        let changes = vec![Change {
            key: "mouse".to_string(),
            was: Some("true".to_string()),
            now: None,
        }];
        apply(&mut configuration, &mut BTreeSet::new(), &changes);
        assert!(configuration.is_empty());
    }

    #[test]
    fn a_key_the_bar_does_not_follow_stays_ignored_through_later_changes() {
        let change = |was: Option<&str>, now: Option<&str>| {
            vec![Change {
                key: "show_mode".to_string(),
                was: was.map(str::to_string),
                now: now.map(str::to_string),
            }]
        };
        let mut configuration = map(&[("show_mode", "true")]);
        let mut ignored = BTreeSet::new();
        assert!(!apply(
            &mut configuration,
            &mut ignored,
            &change(None, Some("true"))
        ));
        // The next step's `was` is the bar's own value, but the bar already
        // knows the layout owns this key.
        assert!(!apply(
            &mut configuration,
            &mut ignored,
            &change(Some("true"), Some("false"))
        ));
        assert!(!apply(
            &mut configuration,
            &mut ignored,
            &change(Some("false"), None)
        ));
        assert_eq!(configuration, map(&[("show_mode", "true")]));
    }

    #[test]
    fn a_bar_takes_previews_of_its_own_alias_only() {
        assert!(is_for("zjhints", Some("zjhints")));
        assert!(!is_for("zjhints", Some("zjhints-alt")));
        assert!(!is_for("zjhints", Some("file:/x/zjhints.wasm")));
        assert!(is_for("zjhints", None));
    }
}
