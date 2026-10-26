//! Keys as the bar shows them: grouping a hint's keys by modifier, aliases
//! for keys and modifiers, and the order alternative keys are listed in.

use crate::{
    DirectionKeys, KeyOrder, CONFIG_CHORD_ALIAS_PREFIX, CONFIG_CHORD_MODS_PREFIX,
    CONFIG_KEY_ALIAS_PREFIX, CONFIG_MOD_ALIAS_PREFIX,
};
use ansi_term::{
    ANSIString,
    Colour::{Fixed, RGB},
    Style,
};
use std::collections::{BTreeMap, BTreeSet};
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;
use zellij_tile_utils::palette_match;

pub(crate) fn find_keys_for_actions(
    keymap: &[(KeyWithModifier, Vec<Action>)],
    target_actions: &[Action],
    exact_match: bool,
) -> Vec<KeyWithModifier> {
    keymap
        .iter()
        .filter_map(|(key, key_actions)| {
            if exact_match {
                let matching = key_actions
                    .iter()
                    .zip(target_actions)
                    .filter(|(a, b)| a.shallow_eq(b))
                    .count();
                if matching == key_actions.len() && matching == target_actions.len() {
                    Some(key.clone())
                } else {
                    None
                }
            } else if key_actions.iter().next() == target_actions.iter().next() {
                Some(key.clone())
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn find_keys_for_action_groups(
    keymap: &[(KeyWithModifier, Vec<Action>)],
    action_groups: &[&[Action]],
) -> Vec<KeyWithModifier> {
    action_groups
        .iter()
        .flat_map(|actions| find_keys_for_actions(keymap, actions, true))
        .collect()
}

/// A run of keys that share the exact same set of modifiers, so the modifier
/// can be factored out and shown once (e.g. `Ctrl+h` and `Ctrl+k` collapse to
/// `^h|k`). `modifiers` is in canonical order (Ctrl, Alt, Shift, Super) since
/// `key_modifiers` is a `BTreeSet`.
pub(crate) struct KeyGroup {
    pub(crate) modifiers: Vec<KeyModifier>,
    pub(crate) keys: Vec<String>,
}

/// Partition a hint's key bindings into groups sharing the same modifiers,
/// preserving first-seen order of both groups and keys. This is what lets
/// `Ctrl h|Alt <|Ctrl k|Alt l` collapse to `^h|k ⌥<|l`.
pub(crate) fn group_keys(
    key_bindings: &[KeyWithModifier],
    config: &BTreeMap<String, String>,
) -> Vec<KeyGroup> {
    let mut groups: Vec<KeyGroup> = vec![];
    for key in key_bindings {
        let modifiers: Vec<KeyModifier> = key.key_modifiers.iter().copied().collect();
        let display = format_bare_key(&key.bare_key, config);
        if let Some(group) = groups.iter_mut().find(|g| g.modifiers == modifiers) {
            group.keys.push(display);
        } else {
            groups.push(KeyGroup {
                modifiers,
                keys: vec![display],
            });
        }
    }
    groups
}

/// Render a modifier alias for display, e.g. `Ctrl` -> `^` when
/// `mod_alias_ctrl "^"` is configured. Without an alias, the modifier's normal
/// name is used (`Ctrl`, `Alt`, `Shift`, `Super`).
pub(crate) fn modifier_display(modifier: KeyModifier, config: &BTreeMap<String, String>) -> String {
    let name = modifier.to_string();
    config
        .get(&format!(
            "{}{}",
            CONFIG_MOD_ALIAS_PREFIX,
            name.to_lowercase()
        ))
        .cloned()
        .unwrap_or(name)
}

/// The prefix shown before a group's keys, including its trailing separator. A
/// word-like modifier (`Ctrl`) is followed by a space (`Ctrl h`); a symbol
/// alias (`^`) hugs the keys (`^h`). An empty modifier set yields no prefix.
/// A group whose modifiers exactly match a configured chord (see
/// `chord_alias`) uses that alias in place of the per-modifier names.
pub(crate) fn group_prefix(modifiers: &[KeyModifier], config: &BTreeMap<String, String>) -> String {
    if modifiers.is_empty() {
        return String::new();
    }
    let joined = chord_alias(modifiers, config).unwrap_or_else(|| {
        modifiers
            .iter()
            .map(|m| modifier_display(*m, config))
            .collect()
    });
    if joined
        .chars()
        .next_back()
        .is_some_and(|c| c.is_alphanumeric())
    {
        format!("{} ", joined)
    } else {
        joined
    }
}

/// Parse one of the four modifier names accepted by `mod_alias_<name>`
/// (`ctrl`, `alt`, `shift`, `super`) back into a `KeyModifier`.
pub(crate) fn parse_modifier_name(name: &str) -> Option<KeyModifier> {
    match name.trim().to_lowercase().as_str() {
        "ctrl" => Some(KeyModifier::Ctrl),
        "alt" => Some(KeyModifier::Alt),
        "shift" => Some(KeyModifier::Shift),
        "super" => Some(KeyModifier::Super),
        _ => None,
    }
}

/// Parse a `chord_mods_<name>` value, e.g. `"ctrl+alt+super+shift"`, into the
/// set of modifiers it names. `None` if any token is unrecognized, so a typo
/// silently disables that one chord rather than matching the wrong
/// combination.
pub(crate) fn parse_chord_mods(value: &str) -> Option<BTreeSet<KeyModifier>> {
    value.split('+').map(parse_modifier_name).collect()
}

/// Look up a configured chord alias for an exact modifier combination. Named
/// pairs are set via `chord_mods_<name>` (the combination) and
/// `chord_alias_<name>` (the symbol shown instead), letting a custom leader
/// key's full chord collapse to one glyph everywhere it appears, e.g. mapping
/// `Ctrl+Alt+Super+Shift` to `&`. A `chord_mods_<name>` with no matching
/// `chord_alias_<name>` has no effect. Matches only the exact set — a hint
/// bound to part of the chord is unaffected.
pub(crate) fn chord_alias(
    modifiers: &[KeyModifier],
    config: &BTreeMap<String, String>,
) -> Option<String> {
    if modifiers.is_empty() {
        return None;
    }
    let wanted: BTreeSet<KeyModifier> = modifiers.iter().copied().collect();
    config
        .iter()
        .filter_map(|(k, v)| Some((k.strip_prefix(CONFIG_CHORD_MODS_PREFIX)?, v)))
        .find(|(_, mods)| parse_chord_mods(mods).as_ref() == Some(&wanted))
        .and_then(|(name, _)| {
            config
                .get(&format!("{}{}", CONFIG_CHORD_ALIAS_PREFIX, name))
                .cloned()
        })
}

/// Render a bare key, substituting a configured alias symbol when one is set.
/// Without an alias this is exactly the key's normal Zellij representation
/// (e.g. `ENTER`, `ESC`, `←`), so behavior is unchanged unless configured.
pub(crate) fn format_bare_key(bare_key: &BareKey, config: &BTreeMap<String, String>) -> String {
    key_alias(bare_key, config).unwrap_or_else(|| bare_key.to_string())
}

/// Look up a configured symbol for `bare_key`. Aliases are set via
/// `key_alias_<name>` options (mirroring zjstatus's `color_<name>` aliases),
/// where `<name>` is a lowercase key name, e.g. `key_alias_enter "↵"`. Several
/// common keys accept a short synonym (e.g. `esc`/`escape`); the first name
/// with a configured value wins.
pub(crate) fn key_alias(bare_key: &BareKey, config: &BTreeMap<String, String>) -> Option<String> {
    key_alias_names(bare_key).into_iter().find_map(|name| {
        config
            .get(&format!("{}{}", CONFIG_KEY_ALIAS_PREFIX, name))
            .cloned()
    })
}

/// The accepted `key_alias_<name>` names for a bare key, in priority order.
pub(crate) fn key_alias_names(bare_key: &BareKey) -> Vec<String> {
    let names: &[&str] = match bare_key {
        BareKey::Enter => &["enter", "return"],
        BareKey::Esc => &["esc", "escape"],
        BareKey::Tab => &["tab"],
        BareKey::Char(' ') => &["space"],
        BareKey::Backspace => &["backspace"],
        BareKey::Delete => &["delete", "del"],
        BareKey::Insert => &["insert", "ins"],
        BareKey::Home => &["home"],
        BareKey::End => &["end"],
        BareKey::PageUp => &["pageup", "pgup"],
        BareKey::PageDown => &["pagedown", "pgdn"],
        BareKey::Up => &["up"],
        BareKey::Down => &["down"],
        BareKey::Left => &["left"],
        BareKey::Right => &["right"],
        BareKey::CapsLock => &["capslock"],
        BareKey::ScrollLock => &["scrolllock"],
        BareKey::NumLock => &["numlock"],
        BareKey::PrintScreen => &["printscreen"],
        BareKey::Pause => &["pause"],
        BareKey::Menu => &["menu"],
        BareKey::F(n) => return vec![format!("f{}", n)],
        // Any other character can be aliased by the character itself.
        BareKey::Char(c) => return vec![c.to_lowercase().to_string()],
    };
    names.iter().map(|s| s.to_string()).collect()
}

pub(crate) fn style_key_with_modifier(
    key_bindings: &[KeyWithModifier],
    palette: &Styling,
    config: &BTreeMap<String, String>,
) -> Vec<ANSIString<'static>> {
    if key_bindings.is_empty() {
        return vec![];
    }

    let saturated_bg = palette_match!(palette.ribbon_unselected.background);
    let contrasting_fg = palette_match!(palette.ribbon_unselected.base);
    let ribbon = || Style::new().fg(contrasting_fg).on(saturated_bg);

    let mut styled_parts = vec![Style::new().paint(" "), ribbon().paint(" ")];

    for (group_idx, group) in group_keys(key_bindings, config).iter().enumerate() {
        if group_idx > 0 {
            styled_parts.push(ribbon().paint(" "));
        }

        let prefix = group_prefix(&group.modifiers, config);
        if !prefix.is_empty() {
            styled_parts.push(ribbon().bold().paint(prefix));
        }

        for key in &group.keys {
            styled_parts.push(ribbon().bold().paint(key.clone()));
        }
    }

    styled_parts.push(ribbon().paint(" "));

    styled_parts
}

/// Style arbitrary text as a key, for hints whose keys have been replaced by a
/// fixed string. Mirrors `style_key_with_modifier` so a replaced key sits in
/// the same ribbon as a real one.
pub(crate) fn style_key_text(text: &str, palette: &Styling) -> Vec<ANSIString<'static>> {
    if text.is_empty() {
        return vec![];
    }
    let saturated_bg = palette_match!(palette.ribbon_unselected.background);
    let contrasting_fg = palette_match!(palette.ribbon_unselected.base);
    let ribbon = Style::new().fg(contrasting_fg).on(saturated_bg);
    vec![
        Style::new().paint(" "),
        ribbon.paint(" ".to_string()),
        ribbon.bold().paint(text.to_string()),
        ribbon.paint(" ".to_string()),
    ]
}

/// Compose the plain-text representation of a key binding as it appears in a
/// hint, e.g. `^p`, `h|j|k|l`, or `←↓↑→`. This is the value substituted for the
/// `{key}` placeholder when a custom `key_format` is configured; the styling
/// itself is left to the format string. Keys sharing a modifier are grouped so
/// the modifier is shown once per group (see `group_keys`).
pub(crate) fn compose_key_text(
    key_bindings: &[KeyWithModifier],
    config: &BTreeMap<String, String>,
) -> String {
    group_keys(key_bindings, config)
        .iter()
        .map(|group| {
            format!(
                "{}{}",
                group_prefix(&group.modifiers, config),
                group.keys.concat()
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Named keys that are not letters, digits or arrows, in rough physical order
/// around the main block. Anything absent sorts after everything listed.
pub(crate) const NAMED_KEY_ORDER: &[BareKey] = &[
    BareKey::Esc,
    BareKey::Tab,
    BareKey::CapsLock,
    BareKey::Enter,
    BareKey::Backspace,
    BareKey::Insert,
    BareKey::Delete,
    BareKey::Home,
    BareKey::End,
    BareKey::PageUp,
    BareKey::PageDown,
    BareKey::PrintScreen,
    BareKey::ScrollLock,
    BareKey::Pause,
    BareKey::NumLock,
    BareKey::Menu,
];

/// Sort a hint's keys into a predictable reading order.
///
/// Modifiers lead the sort so that keys sharing one stay contiguous —
/// `group_keys` renders each modifier group as a single run (`^hjkl`), and
/// interleaving would shatter that into `^h h ^j j`.
pub(crate) fn sort_keys(keys: &mut [KeyWithModifier], order: KeyOrder) {
    if order == KeyOrder::Unsorted {
        return;
    }
    keys.sort_by_key(|key| {
        (
            modifier_rank(&key.key_modifiers),
            modifier_tiebreak(&key.key_modifiers),
            key_rank(&key.bare_key, order),
        )
    });
}

/// Which modifier group a key belongs to: unmodified first, then `Ctrl`,
/// `Super`, `Alt`, `Shift`. A key carrying several modifiers sorts with the
/// strongest one it has, so `Ctrl Shift p` groups under `Ctrl`.
///
/// Unmodified keys lead because they are the plainest way to reach an action,
/// and because a hint too wide to fit is cut from the right — so whatever sorts
/// first is what survives.
pub(crate) fn modifier_rank(modifiers: &BTreeSet<KeyModifier>) -> u8 {
    modifiers
        .iter()
        .map(|modifier| match modifier {
            KeyModifier::Ctrl => 1,
            KeyModifier::Super => 2,
            KeyModifier::Alt => 3,
            KeyModifier::Shift => 4,
        })
        .min()
        .unwrap_or(0)
}

/// Separates combinations that share a `modifier_rank` (`Ctrl` from
/// `Ctrl Shift`) so their grouping stays stable rather than depending on the
/// order Zellij reported them.
pub(crate) fn modifier_tiebreak(modifiers: &BTreeSet<KeyModifier>) -> (usize, u8) {
    (modifiers.len(), modifiers.iter().map(modifier_bit).sum())
}

pub(crate) fn modifier_bit(modifier: &KeyModifier) -> u8 {
    match modifier {
        KeyModifier::Ctrl => 1,
        KeyModifier::Super => 2,
        KeyModifier::Alt => 4,
        KeyModifier::Shift => 8,
    }
}

/// Where a key sits in the reading order, as `(category, row, column)`.
///
/// Categories keep unlike keys from interleaving, so letters gather before
/// punctuation rather than mixing at row boundaries — `opas\`, not `op\as`.
/// Within a category, position on the layout does the rest: function keys and
/// digits by number, letters and punctuation by row then column, arrows in
/// `hjkl` order rather than the `Left, Down, Up, Right` the enum declares.
pub(crate) fn key_rank(key: &BareKey, order: KeyOrder) -> (u8, u32, u32) {
    match key {
        BareKey::F(n) => (0, *n as u32, 0),
        BareKey::Char(c) => {
            let lowered = c.to_ascii_lowercase();
            let category = if lowered.is_ascii_digit() {
                1
            } else if lowered.is_ascii_alphabetic() {
                2
            } else {
                3
            };
            // No layout to consult: order by the character itself, which for
            // ASCII is `0-9` then `a-z`.
            if order == KeyOrder::Alphabetical {
                return (category, lowered as u32, 0);
            }
            for (row, keys) in order.rows().iter().enumerate() {
                if let Some(column) = keys.chars().position(|k| k == lowered) {
                    return (category, row as u32, column as u32);
                }
            }
            // Off-layout characters (space, non-ASCII) keep a stable order of
            // their own rather than landing at row 0 alongside the digits.
            (4, lowered as u32, 0)
        }
        BareKey::Left => (5, 0, 0),
        BareKey::Down => (5, 1, 0),
        BareKey::Up => (5, 2, 0),
        BareKey::Right => (5, 3, 0),
        other => {
            let position = NAMED_KEY_ORDER.iter().position(|k| k == other);
            (6, position.unwrap_or(NAMED_KEY_ORDER.len()) as u32, 0)
        }
    }
}

/// Drop one directional family (hjkl or arrows) when a hint is bound to both,
/// per the `direction_keys` setting. Only reduces when both families are
/// present, so a hint bound to just one is never emptied.
pub(crate) fn filter_direction_keys(
    key_bindings: &[KeyWithModifier],
    preference: DirectionKeys,
) -> Vec<KeyWithModifier> {
    if preference == DirectionKeys::Both {
        return key_bindings.to_vec();
    }

    let has_arrows = key_bindings.iter().any(|k| is_arrow_key(&k.bare_key));
    let has_letters = key_bindings.iter().any(|k| is_hjkl_key(&k.bare_key));
    if !(has_arrows && has_letters) {
        return key_bindings.to_vec();
    }

    key_bindings
        .iter()
        .filter(|k| match preference {
            DirectionKeys::Arrows => !is_hjkl_key(&k.bare_key),
            DirectionKeys::Letters => !is_arrow_key(&k.bare_key),
            DirectionKeys::Both => true,
        })
        .cloned()
        .collect()
}

pub(crate) fn is_arrow_key(bare_key: &BareKey) -> bool {
    matches!(
        bare_key,
        BareKey::Left | BareKey::Right | BareKey::Up | BareKey::Down
    )
}

pub(crate) fn is_hjkl_key(bare_key: &BareKey) -> bool {
    matches!(bare_key, BareKey::Char(c) if matches!(c.to_ascii_lowercase(), 'h' | 'j' | 'k' | 'l'))
}
