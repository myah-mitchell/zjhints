//! The hints themselves: which actions get one, what each is labeled, and
//! laying a mode's hints out on one line.

use crate::keys::{
    compose_key_text, filter_direction_keys, find_keys_for_action_groups, find_keys_for_actions,
    sort_keys, style_key_text, style_key_with_modifier,
};
use crate::{
    keypress, ClickBehavior, HintStyle, CONFIG_DESC_FORMAT_PREFIX, CONFIG_KEYS_PREFIX,
    CONFIG_KEY_FORMAT_PREFIX, CONFIG_LABEL_PREFIX, NORMAL_MODE_ACTIONS, PANE_MODE_ACTION_SEQUENCES,
    SESSION_PLUGINS, TAB_MODE_ACTION_SEQUENCES, TO_NORMAL,
};
use ansi_term::{
    ANSIString,
    Colour::{Fixed, RGB},
    Style,
};
use std::collections::{BTreeMap, BTreeSet};
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::actions::SearchDirection;
use zellij_tile::prelude::*;
use zellij_tile_utils::palette_match;
use zjkit::ansi::visible_width;
use zjkit::fit::{drop_to_fit, Fittable, Group};
use zjkit::format;
use zjkit::mouse::{Choice, Item, LineSegment, SegmentContent};

pub(crate) fn style_description(description: &str, palette: &Styling) -> Vec<ANSIString<'static>> {
    let less_saturated_bg = palette_match!(palette.text_unselected.background);
    let contrasting_fg = palette_match!(palette.text_unselected.base);

    vec![Style::new()
        .fg(contrasting_fg)
        .on(less_saturated_bg)
        .paint(format!(" {} ", description))]
}

pub(crate) fn plugin_key(
    keymap: &[(KeyWithModifier, Vec<Action>)],
    plugin_name: &str,
) -> Option<KeyWithModifier> {
    keymap.iter().find_map(|(key, key_actions)| {
        if key_actions
            .iter()
            .any(|action| action.launches_plugin(plugin_name))
        {
            Some(key.clone())
        } else {
            None
        }
    })
}

/// Every key that leaves this mode for Normal.
///
/// Zellij binds Enter and Esc to the very same `SwitchToMode "Normal"`, so they
/// form one hint listing both keys rather than being split into a "select" and
/// an "exit" — a distinction Zellij does not make.
pub(crate) fn exit_keys(keymap: &[(KeyWithModifier, Vec<Action>)]) -> Vec<KeyWithModifier> {
    find_keys_for_actions(keymap, &[TO_NORMAL], true)
}

/// A hint queued for rendering: a concept `id`, the `label` shown for it, and
/// every key bound to it.
///
/// `id` is the hint's name in config (`label_<id>`, `key_format_<id>`,
/// `hint_order`), and it never changes with the label. Hints sharing a
/// `merge_key` become one. That key is the `id` unless the user set a label, so
/// unrelated hints can share a display label without fusing (Pane's `new` is
/// `new_pane`, Tab's is `new_tab`), and one concept gathers keys from several
/// sources (a curated `focus` on `hjkl` and a discovered `focus` on the arrows
/// become one hint). A label the user set is its own merge key instead; see
/// `merged_label_id`.
pub(crate) struct Hint {
    pub(crate) id: String,
    pub(crate) merge_key: String,
    pub(crate) label: String,
    pub(crate) keys: Vec<KeyWithModifier>,
}

/// Queue a hint under concept `id`, recording the keys it consumed in `used` so
/// later discovery does not repeat them.
pub(crate) fn add_hint(
    hints: &mut Vec<Hint>,
    keys: &[KeyWithModifier],
    merge_key: &str,
    id: &str,
    label: &str,
    used: &mut Vec<KeyWithModifier>,
) {
    if keys.is_empty() {
        return;
    }
    merge_hint(hints, merge_key, id, label, keys);
    used.extend(keys.iter().cloned());
}

/// Add `keys` to the hint with this `merge_key`, creating it if it does not
/// exist yet. Keys already present are skipped so a merged hint never repeats a
/// key. The first source to create a hint sets its id and label; later merges
/// only add keys.
pub(crate) fn merge_hint(
    hints: &mut Vec<Hint>,
    merge_key: &str,
    id: &str,
    label: &str,
    keys: &[KeyWithModifier],
) {
    match hints.iter_mut().find(|hint| hint.merge_key == merge_key) {
        Some(hint) => {
            for key in keys {
                if !hint.keys.contains(key) {
                    hint.keys.push(key.clone());
                }
            }
        }
        None => hints.push(Hint {
            id: id.to_string(),
            merge_key: merge_key.to_string(),
            label: label.to_string(),
            keys: keys.to_vec(),
        }),
    }
}

/// Render a hint's styled segments, without tracking consumed keys.
pub(crate) fn render_hint(parts: &mut Vec<ANSIString<'static>>, hint: &Hint, style: &HintStyle) {
    // Optionally collapse hjkl/arrow duplicates down to a single family.
    let mut keys = filter_direction_keys(&hint.keys, style.direction_keys);
    sort_keys(&mut keys, style.key_order);
    let keys = keys.as_slice();

    // A per-hint format overrides the global one; either overrides the theme
    // palette. An empty value falls back to the palette rather than rendering
    // nothing, matching how the global options treat empty.
    let key_format = hint_override(CONFIG_KEY_FORMAT_PREFIX, hint, style.mode, style.config);
    let key_format = match &key_format {
        Some(per_hint) => per_hint.as_deref(),
        None => style.key_format,
    };

    // Keys can be replaced outright by a fixed string, for hints better
    // described than enumerated — a chord, a mouse gesture, or a whole family
    // of keys standing in as `1-9`. An empty value renders no key part at all,
    // leaving the label to speak for itself.
    let replacement = hint_override(CONFIG_KEYS_PREFIX, hint, style.mode, style.config);
    let key_text = match &replacement {
        Some(fixed) => fixed.clone().unwrap_or_default(),
        None => compose_key_text(keys, style.config),
    };

    match key_format {
        Some(fmt) if !key_text.is_empty() => parts.extend(format::render_template(
            fmt,
            &[("key", &key_text)],
            style.config,
            style.dim,
        )),
        Some(_) => {}
        // Real keys keep the modifier grouping the palette styling does;
        // a replacement is already a finished string.
        None if replacement.is_some() => parts.extend(style_key_text(&key_text, style.colors)),
        None => parts.extend(style_key_with_modifier(keys, style.colors, style.config)),
    }

    // Description part, resolved the same way.
    let desc_format = hint_override(CONFIG_DESC_FORMAT_PREFIX, hint, style.mode, style.config);
    let desc_format = match &desc_format {
        Some(per_hint) => per_hint.as_deref(),
        None => style.desc_format,
    };

    match desc_format {
        Some(fmt) => parts.extend(format::render_template(
            fmt,
            &[("desc", &hint.label)],
            style.config,
            style.dim,
        )),
        None => parts.extend(style_description(&hint.label, style.colors)),
    }
}

/// The styled hint line for a mode, and the same line as `LineSegment`s
/// saying which columns hold which hint, for mouse clicks.
pub(crate) fn lay_out_hints_for_mode(
    mode: InputMode,
    keymap: &[(KeyWithModifier, Vec<Action>)],
    style: &HintStyle,
) -> (Vec<ANSIString<'static>>, Vec<LineSegment>) {
    let mut hints: Vec<Hint> = vec![];
    // Keys consumed by curated hints, so discovery does not repeat them.
    let mut used: Vec<KeyWithModifier> = vec![];
    let exit = exit_keys(keymap);

    match mode {
        InputMode::Normal => {
            for (action, label) in NORMAL_MODE_ACTIONS {
                let keys = find_keys_for_actions(keymap, std::slice::from_ref(action), true);
                add_curated_hint(&mut hints, &keys, action, label, style, &mut used);
            }
        }
        InputMode::Pane => {
            for (actions, label) in PANE_MODE_ACTION_SEQUENCES {
                let keys = find_keys_for_actions(keymap, actions, false);
                if let (false, Some(action)) = (keys.is_empty(), actions.first()) {
                    add_curated_hint(&mut hints, &keys, action, label, style, &mut used);
                }
            }

            let rename_keys = find_keys_for_actions(
                keymap,
                &[
                    Action::SwitchToMode {
                        input_mode: InputMode::RenamePane,
                    },
                    Action::PaneNameInput { input: vec![0] },
                ],
                false,
            );
            if !rename_keys.is_empty() {
                add_group_hint(
                    &mut hints,
                    &rename_keys,
                    "rename",
                    "rename",
                    style,
                    &mut used,
                );
            }

            let focus_keys = find_keys_for_action_groups(
                keymap,
                &[
                    &[Action::MoveFocus {
                        direction: Direction::Left,
                    }],
                    &[Action::MoveFocus {
                        direction: Direction::Down,
                    }],
                    &[Action::MoveFocus {
                        direction: Direction::Up,
                    }],
                    &[Action::MoveFocus {
                        direction: Direction::Right,
                    }],
                ],
            );
            add_group_hint(&mut hints, &focus_keys, "focus", "focus", style, &mut used);
            add_exit_hint(&mut hints, &exit, style, &mut used);
        }
        InputMode::Tab => {
            for (actions, label) in TAB_MODE_ACTION_SEQUENCES {
                let keys = find_keys_for_actions(keymap, actions, false);
                if let (false, Some(action)) = (keys.is_empty(), actions.first()) {
                    add_curated_hint(&mut hints, &keys, action, label, style, &mut used);
                }
            }

            let rename_keys = find_keys_for_actions(
                keymap,
                &[
                    Action::SwitchToMode {
                        input_mode: InputMode::RenameTab,
                    },
                    Action::TabNameInput { input: vec![0] },
                ],
                false,
            );
            if !rename_keys.is_empty() {
                add_group_hint(
                    &mut hints,
                    &rename_keys,
                    "rename",
                    "rename",
                    style,
                    &mut used,
                );
            }

            // Every key bound to tab navigation, both families. Narrowing to one
            // is `direction_keys`' job, done at render time — deciding it here
            // would both ignore that setting and leave the keys it dropped
            // unclaimed, for discovery to resurface as separate hints.
            let focus_keys = find_keys_for_action_groups(
                keymap,
                &[&[Action::GoToPreviousTab], &[Action::GoToNextTab]],
            );
            add_group_hint(&mut hints, &focus_keys, "focus", "focus", style, &mut used);
            add_exit_hint(&mut hints, &exit, style, &mut used);
        }
        InputMode::Resize => {
            let resize_keys = find_keys_for_action_groups(
                keymap,
                &[
                    &[Action::Resize {
                        resize: Resize::Increase,
                        direction: None,
                    }],
                    &[Action::Resize {
                        resize: Resize::Decrease,
                        direction: None,
                    }],
                ],
            );
            add_group_hint(
                &mut hints,
                &resize_keys,
                "resize",
                "resize",
                style,
                &mut used,
            );

            let increase_keys = find_keys_for_action_groups(
                keymap,
                &[
                    &[Action::Resize {
                        resize: Resize::Increase,
                        direction: Some(Direction::Left),
                    }],
                    &[Action::Resize {
                        resize: Resize::Increase,
                        direction: Some(Direction::Down),
                    }],
                    &[Action::Resize {
                        resize: Resize::Increase,
                        direction: Some(Direction::Up),
                    }],
                    &[Action::Resize {
                        resize: Resize::Increase,
                        direction: Some(Direction::Right),
                    }],
                ],
            );
            add_group_hint(
                &mut hints,
                &increase_keys,
                "increase",
                "increase",
                style,
                &mut used,
            );

            let decrease_keys = find_keys_for_action_groups(
                keymap,
                &[
                    &[Action::Resize {
                        resize: Resize::Decrease,
                        direction: Some(Direction::Left),
                    }],
                    &[Action::Resize {
                        resize: Resize::Decrease,
                        direction: Some(Direction::Down),
                    }],
                    &[Action::Resize {
                        resize: Resize::Decrease,
                        direction: Some(Direction::Up),
                    }],
                    &[Action::Resize {
                        resize: Resize::Decrease,
                        direction: Some(Direction::Right),
                    }],
                ],
            );
            add_group_hint(
                &mut hints,
                &decrease_keys,
                "decrease",
                "decrease",
                style,
                &mut used,
            );
            add_exit_hint(&mut hints, &exit, style, &mut used);
        }
        InputMode::Move => {
            let move_keys = find_keys_for_action_groups(
                keymap,
                &[
                    &[Action::MovePane {
                        direction: Some(Direction::Left),
                    }],
                    &[Action::MovePane {
                        direction: Some(Direction::Down),
                    }],
                    &[Action::MovePane {
                        direction: Some(Direction::Up),
                    }],
                    &[Action::MovePane {
                        direction: Some(Direction::Right),
                    }],
                ],
            );
            add_group_hint(
                &mut hints,
                &move_keys,
                "move_pane",
                "move",
                style,
                &mut used,
            );
            add_exit_hint(&mut hints, &exit, style, &mut used);
        }
        InputMode::Scroll => {
            let search_keys = find_keys_for_actions(
                keymap,
                &[
                    Action::SwitchToMode {
                        input_mode: InputMode::EnterSearch,
                    },
                    Action::SearchInput { input: vec![0] },
                ],
                true,
            );
            add_group_hint(
                &mut hints,
                &search_keys,
                "mode_search",
                "search",
                style,
                &mut used,
            );

            let scroll_keys =
                find_keys_for_action_groups(keymap, &[&[Action::ScrollDown], &[Action::ScrollUp]]);
            add_group_hint(
                &mut hints,
                &scroll_keys,
                "scroll",
                "scroll",
                style,
                &mut used,
            );

            let page_scroll_keys = find_keys_for_action_groups(
                keymap,
                &[&[Action::PageScrollDown], &[Action::PageScrollUp]],
            );
            add_group_hint(
                &mut hints,
                &page_scroll_keys,
                "page",
                "page",
                style,
                &mut used,
            );

            let half_page_scroll_keys = find_keys_for_action_groups(
                keymap,
                &[&[Action::HalfPageScrollDown], &[Action::HalfPageScrollUp]],
            );
            add_group_hint(
                &mut hints,
                &half_page_scroll_keys,
                "half_page",
                "half page",
                style,
                &mut used,
            );

            let edit_keys = find_keys_for_actions(
                keymap,
                &[Action::EditScrollback { ansi: false }, TO_NORMAL],
                false,
            );
            if !edit_keys.is_empty() {
                add_group_hint(&mut hints, &edit_keys, "edit", "edit", style, &mut used);
            }
            add_exit_hint(&mut hints, &exit, style, &mut used);
        }
        InputMode::Search => {
            let search_keys = find_keys_for_actions(
                keymap,
                &[
                    Action::SwitchToMode {
                        input_mode: InputMode::EnterSearch,
                    },
                    Action::SearchInput { input: vec![0] },
                ],
                true,
            );
            add_group_hint(
                &mut hints,
                &search_keys,
                "mode_search",
                "search",
                style,
                &mut used,
            );

            let scroll_keys =
                find_keys_for_action_groups(keymap, &[&[Action::ScrollDown], &[Action::ScrollUp]]);
            add_group_hint(
                &mut hints,
                &scroll_keys,
                "scroll",
                "scroll",
                style,
                &mut used,
            );

            let page_scroll_keys = find_keys_for_action_groups(
                keymap,
                &[&[Action::PageScrollDown], &[Action::PageScrollUp]],
            );
            add_group_hint(
                &mut hints,
                &page_scroll_keys,
                "page",
                "page",
                style,
                &mut used,
            );

            let half_page_scroll_keys = find_keys_for_action_groups(
                keymap,
                &[&[Action::HalfPageScrollDown], &[Action::HalfPageScrollUp]],
            );
            add_group_hint(
                &mut hints,
                &half_page_scroll_keys,
                "half_page",
                "half page",
                style,
                &mut used,
            );

            let down_keys = find_keys_for_actions(
                keymap,
                &[Action::Search {
                    direction: SearchDirection::Down,
                }],
                true,
            );
            add_group_hint(
                &mut hints,
                &down_keys,
                "search_down",
                "down",
                style,
                &mut used,
            );

            let up_keys = find_keys_for_actions(
                keymap,
                &[Action::Search {
                    direction: SearchDirection::Up,
                }],
                true,
            );
            add_group_hint(&mut hints, &up_keys, "search_up", "up", style, &mut used);

            add_exit_hint(&mut hints, &exit, style, &mut used);
        }
        InputMode::Session => {
            let detach_keys = find_keys_for_actions(keymap, &[Action::Detach], true);
            add_group_hint(
                &mut hints,
                &detach_keys,
                "detach",
                "detach",
                style,
                &mut used,
            );

            for (plugin, id, label) in SESSION_PLUGINS {
                if let Some(key) = plugin_key(keymap, plugin) {
                    add_group_hint(&mut hints, &[key], id, label, style, &mut used);
                }
            }

            add_exit_hint(&mut hints, &exit, style, &mut used);
        }
        _ => {
            let keys = find_keys_for_actions(
                keymap,
                &[Action::SwitchToMode {
                    input_mode: InputMode::Normal,
                }],
                true,
            );
            add_exit_hint(&mut hints, &keys, style, &mut used);
        }
    }

    // Append every other enabled keybinding in this mode that the curated
    // hints above didn't already cover.
    if style.discover {
        add_discovered_hints(&mut hints, keymap, &used, style);
    }

    // Stable, so hints the user didn't name keep the order this mode built them.
    if !style.hint_order.is_empty() {
        hints.sort_by_key(|hint| style.hint_order.rank(hint));
    }

    // Render each hint on its own, so whole hints can be dropped to fit rather
    // than the line being cut mid-hint.
    let mut pieces: Vec<HintPiece> = hints
        .iter()
        .enumerate()
        .map(|(index, hint)| {
            let mut parts = vec![];
            render_hint(&mut parts, hint, style);
            HintPiece {
                width: visible_width(&parts, style.wide_ambiguous),
                group: style.hint_order.group(hint),
                parts,
                index,
            }
        })
        .collect();

    let spacer: Vec<ANSIString<'static>> = style
        .spacer
        .map(|spacer| format::render_template(spacer, &[], style.config, style.dim))
        .unwrap_or_default();

    let indicator: Vec<ANSIString<'static>> = style
        .drop_indicator
        .map(|indicator| format::render_template(indicator, &[], style.config, style.dim))
        .unwrap_or_default();

    let spacer_width = visible_width(&spacer, style.wide_ambiguous);
    // The spacer after the mode prefix is there whatever is dropped, so it
    // comes off the budget up front.
    let leading_spacer = if style.spacer_after_prefix {
        spacer_width
    } else {
        0
    };
    let gap = style.limit.and_then(|limit| {
        drop_to_fit(
            &mut pieces,
            spacer_width,
            limit.saturating_sub(leading_spacer),
            visible_width(&indicator, style.wide_ambiguous),
            style.precedence,
        )
    });

    let targets: Vec<Item> = hints
        .iter()
        .map(|hint| hint_target(hint, keymap, style))
        .collect();
    let kept: BTreeSet<usize> = pieces.iter().map(|piece| piece.index).collect();
    let dropped: Vec<Item> = targets
        .iter()
        .enumerate()
        .filter(|(index, _)| !kept.contains(index))
        .map(|(_, target)| target.clone())
        .collect();

    let mut rendered: Vec<(Vec<ANSIString<'static>>, SegmentContent)> = pieces
        .into_iter()
        .map(|piece| {
            let target = targets[piece.index].clone();
            (piece.parts, SegmentContent::Item(target))
        })
        .collect();
    // The indicator stands in for the dropped run, spaced like a hint so it
    // reads as one.
    if let Some(gap) = gap.filter(|_| !indicator.is_empty()) {
        rendered.insert(
            gap.min(rendered.len()),
            (indicator, SegmentContent::Dropped(dropped)),
        );
    }

    let mut parts = vec![];
    let mut segments = vec![];
    for (idx, (piece, content)) in rendered.into_iter().enumerate() {
        if idx > 0 || style.spacer_after_prefix {
            parts.extend(spacer.iter().cloned());
            segments.push(LineSegment {
                width: spacer_width,
                content: SegmentContent::Spacer,
            });
        }
        segments.push(LineSegment {
            width: visible_width(&piece, style.wide_ambiguous),
            content,
        });
        parts.extend(piece);
    }
    (parts, segments)
}

/// A hint as a click target: its displayed keys, grouped by what each does in
/// this mode's keymap.
///
/// The keys are the ones the bar shows for it, after `direction_keys`, so a
/// click never runs a binding the hint was not showing.
pub(crate) fn hint_target(
    hint: &Hint,
    keymap: &[(KeyWithModifier, Vec<Action>)],
    style: &HintStyle,
) -> Item {
    let mut keys = filter_direction_keys(&hint.keys, style.direction_keys);
    sort_keys(&mut keys, style.key_order);

    let mut grouped: Vec<(Vec<KeyWithModifier>, &Vec<Action>)> = vec![];
    for key in keys {
        let Some((_, actions)) = keymap.iter().find(|(bound, _)| *bound == key) else {
            continue;
        };
        match grouped
            .iter_mut()
            .find(|(_, existing)| *existing == actions)
        {
            Some((group_keys, _)) => group_keys.push(key),
            None => grouped.push((vec![key], actions)),
        }
    }

    // What a hint's own keys look like on the bar, a `keys_<id>` replacement
    // included, so the menu entry for it reads the same as the hint would.
    let shown_keys = match hint_override(CONFIG_KEYS_PREFIX, hint, style.mode, style.config) {
        Some(fixed) => fixed.unwrap_or_default(),
        None => {
            let all: Vec<KeyWithModifier> = grouped
                .iter()
                .flat_map(|(group_keys, _)| group_keys.iter().cloned())
                .collect();
            compose_key_text(&all, style.config)
        }
    };

    let choices = grouped
        .iter()
        .map(|(group_keys, actions)| {
            // The hint's label already says what all of its keys have in
            // common. Telling its keys apart takes naming the action itself.
            let primary = actions
                .iter()
                .find(|action| !is_hidden_action(action))
                .or(actions.first());
            let description = primary.map_or_else(String::new, |action| {
                action_signature(action).replace('_', " ")
            });
            let actions = match style.click {
                ClickBehavior::RunActions => actions.to_vec(),
                // Any one of the group's keys does it; the first is the one
                // shown first.
                ClickBehavior::PressKeys => keypress::press_keys(&group_keys[..1]),
            };
            Choice {
                label: menu_label(&compose_key_text(group_keys, style.config), &description),
                actions,
            }
        })
        .collect();

    Item {
        label: menu_label(&shown_keys, &hint.label),
        choices,
    }
}

/// A menu entry's text: keys, then what they do, as on the bar.
pub(crate) fn menu_label(keys: &str, description: &str) -> String {
    if keys.is_empty() {
        description.to_string()
    } else {
        format!("{}  {}", keys, description)
    }
}

/// One rendered hint, kept separate so it can be dropped whole.
pub(crate) struct HintPiece {
    pub(crate) parts: Vec<ANSIString<'static>>,
    pub(crate) width: usize,
    pub(crate) group: Group,
    /// Position of the hint this was rendered from, so the hints dropped to
    /// fit can be told apart from the ones kept.
    pub(crate) index: usize,
}

impl Fittable for HintPiece {
    fn width(&self) -> usize {
        self.width
    }

    fn group(&self) -> Group {
        self.group
    }
}

/// Discover every enabled keybinding the curated hints didn't already show, and
/// merge it into `hints`. Bindings are keyed by their resolved label (so
/// families like the directional focus keys collapse into a single hint, and a
/// label a curated hint already produced gains the extra keys instead of being
/// shown a second time). Labels resolve from, in order: a `label_<action>`
/// config override, the built-in label table, or a name derived from the action.
///
/// Bindings inherited from the base mode (Zellij's `shared_except` groups) are
/// skipped, so entering Pane mode doesn't re-list the global mode switches that
/// are already advertised in the base mode.
pub(crate) fn add_discovered_hints(
    hints: &mut Vec<Hint>,
    keymap: &[(KeyWithModifier, Vec<Action>)],
    used: &[KeyWithModifier],
    style: &HintStyle,
) {
    for (key, actions) in keymap {
        if used.contains(key) {
            continue;
        }
        // A binding's first action is often plumbing — a pipe to a plugin, a
        // rename input — with the part worth advertising coming after it. Judge
        // the binding by its first action that is worth a hint, so a key like
        // `MessagePlugin "autolock"; SwitchToMode "Normal"` still shows as the
        // unlock it visibly is, rather than vanishing entirely.
        let Some(primary) = actions.iter().find(|action| !is_hidden_action(action)) else {
            continue;
        };
        if is_shared_binding(key, primary, style.shared) {
            continue;
        }
        let Some((merge_key, id, label)) = resolve_hint(primary, style) else {
            continue;
        };

        merge_hint(hints, &merge_key, &id, &label, std::slice::from_ref(key));
    }
}

/// Whether `key` is bound to the same action in the base mode, meaning it is a
/// global binding already shown there rather than one specific to this mode.
pub(crate) fn is_shared_binding(
    key: &KeyWithModifier,
    action: &Action,
    shared: &[(KeyWithModifier, Vec<Action>)],
) -> bool {
    shared.iter().any(|(shared_key, shared_actions)| {
        shared_key == key
            && shared_actions
                .first()
                .is_some_and(|shared_action| shared_action.shallow_eq(action))
    })
}

/// Resolve the label for a discovered keybinding's primary action. A user
/// `label_<action>` override wins; an empty override hides the hint. Otherwise
/// the built-in table is consulted, falling back to a name derived from the
/// action's own signature so nothing is ever left unlabeled.
///
/// Returns the hint's merge key, id and label.
pub(crate) fn resolve_hint(action: &Action, style: &HintStyle) -> Option<(String, String, String)> {
    let signature = action_signature(action);
    let (id, default) = match builtin_hint(action) {
        Some((id, label)) => (id.to_string(), label.to_string()),
        // No curated entry: the action names its own concept, and the label is
        // derived from that same name.
        None => (signature.clone(), signature.replace('_', " ")),
    };

    match label_override(&id, &signature, style.mode, style.config) {
        // An explicit label is its own merge key, so two actions given the same
        // label become one hint. See `merged_label_id`.
        Some(Some(label)) => Some((merged_label_id(&label), id, label)),
        // An empty override hides the hint.
        Some(None) => None,
        None => Some((id.clone(), id, default)),
    }
}

/// Look up a `label_<...>` override, accepting the hint's concept `id`
/// (`label_split_down`) or the raw action `signature` (`label_new_pane_down`),
/// each optionally scoped to the current mode (`label_locked_mode_normal`).
///
/// The outer `Option` distinguishes "no override set" from an override that is
/// present; the inner one is `None` for an empty value, meaning hide the hint.
pub(crate) fn label_override(
    id: &str,
    signature: &str,
    mode: &str,
    config: &BTreeMap<String, String>,
) -> Option<Option<String>> {
    // Most specific first: a label scoped to this mode beats a global one, and
    // the concept id beats the raw action signature. This is what lets
    // `label_locked_mode_normal "unlock"` retitle the unlock key without
    // touching every other mode's escape hatch.
    let value = [
        format!("{}{}_{}", CONFIG_LABEL_PREFIX, mode, id),
        format!("{}{}_{}", CONFIG_LABEL_PREFIX, mode, signature),
        format!("{}{}", CONFIG_LABEL_PREFIX, id),
        format!("{}{}", CONFIG_LABEL_PREFIX, signature),
    ]
    .iter()
    .find_map(|key| config.get(key))?;
    if value.is_empty() {
        Some(None)
    } else {
        Some(Some(value.clone()))
    }
}

/// Queue a curated hint, honoring a `label_<action>` override on `action`.
///
/// Curated hints carry hand-tuned labels rather than deriving them, so without
/// this they would silently ignore `label_<action>` entirely. An override that
/// hides the hint (an empty value) still marks its keys used, so discovery does
/// not resurface the same binding under the built-in label.
pub(crate) fn add_curated_hint(
    hints: &mut Vec<Hint>,
    keys: &[KeyWithModifier],
    action: &Action,
    default: &str,
    style: &HintStyle,
    used: &mut Vec<KeyWithModifier>,
) {
    let signature = action_signature(action);
    let id = builtin_hint(action)
        .map(|(id, _)| id.to_string())
        .unwrap_or_else(|| signature.clone());
    apply_label(hints, keys, &id, &signature, default, style, used);
}

/// Queue the hint for the keys that leave a mode for Normal.
///
/// Routed through `add_curated_hint` with the `SwitchToMode "Normal"` action —
/// not `add_group_hint` — so the override resolves by action name as well as by
/// id. Both `label_mode_normal` and `label_switch_to_mode_normal` (and their
/// mode-scoped forms) reach it. That the action form works matters: it is what
/// the built-in status bar used, so configs carried over from it keep working.
pub(crate) fn add_exit_hint(
    hints: &mut Vec<Hint>,
    keys: &[KeyWithModifier],
    style: &HintStyle,
    used: &mut Vec<KeyWithModifier>,
) {
    add_curated_hint(hints, keys, &TO_NORMAL, "normal", style, used);
}

/// Queue a curated hint that has no single backing action — one assembled from a
/// group of related bindings (the four resize directions), from alternatives
/// (`rename` matches whichever of two actions is bound), or discovered at
/// runtime (`select`, the plugin launchers). These declare their concept `id`
/// explicitly, since there is no action to derive it from.
pub(crate) fn add_group_hint(
    hints: &mut Vec<Hint>,
    keys: &[KeyWithModifier],
    id: &str,
    default: &str,
    style: &HintStyle,
    used: &mut Vec<KeyWithModifier>,
) {
    apply_label(hints, keys, id, id, default, style, used);
}

/// Queue a hint under `id`, applying a `label_<id>` / `label_<signature>`
/// override if one is set. An override that hides the hint still marks its keys
/// used, so discovery does not resurface it under the built-in label.
pub(crate) fn apply_label(
    hints: &mut Vec<Hint>,
    keys: &[KeyWithModifier],
    id: &str,
    signature: &str,
    default: &str,
    style: &HintStyle,
    used: &mut Vec<KeyWithModifier>,
) {
    match label_override(id, signature, style.mode, style.config) {
        Some(Some(label)) => add_hint(hints, keys, &merged_label_id(&label), id, &label, used),
        Some(None) => used.extend(keys.iter().cloned()),
        None => add_hint(hints, keys, id, id, default, used),
    }
}

/// Look up a per-hint setting, e.g. `key_format_split_down`.
///
/// Tries the most specific key first, so a mode-scoped setting beats a global
/// one, exactly as labels resolve. A hint can be named by its concept id or by
/// its label. The label matters for hints fused under a label you chose, whose
/// id is only that of whichever binding came first.
///
/// The outer `Option` separates "not set" from set; the inner one is `None`
/// for an empty value, which each caller reads for itself.
pub(crate) fn hint_override(
    prefix: &str,
    hint: &Hint,
    mode: &str,
    config: &BTreeMap<String, String>,
) -> Option<Option<String>> {
    let label = config_slug(&hint.label);
    let value = [
        format!("{}{}_{}", prefix, mode, hint.id),
        format!("{}{}_{}", prefix, mode, label),
        format!("{}{}", prefix, hint.id),
        format!("{}{}", prefix, label),
    ]
    .iter()
    .find_map(|key| config.get(key))?;
    Some(Some(value.clone()).filter(|value| !value.is_empty()))
}

/// A label as it can be written in a config key: `swap layout` -> `swap_layout`.
pub(crate) fn config_slug(label: &str) -> String {
    label
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
}

/// The merge key for a hint whose label the user set explicitly.
///
/// Deliberately relabeling two hints to the same text fuses them into one hint —
/// that is how you combine separate bindings you consider a single concept, e.g.
/// giving both swap-layout actions the label "swap layout". Built-in labels do
/// *not* merge this way, so hints that merely ship with the same label (Pane's
/// `new` and Tab's `new`) stay separate.
pub(crate) fn merged_label_id(label: &str) -> String {
    // `=` never appears in a concept id, so this cannot collide with one's key.
    format!("={}", label)
}

/// A stable, configuration-friendly identifier for an action, used both as the
/// `label_<name>` config key and as the derived fallback label. It is the
/// snake_case variant name, with the target mode appended for mode switches
/// (e.g. `switch_to_mode_locked`) since that distinction is meaningful.
pub(crate) fn action_signature(action: &Action) -> String {
    let debug = format!("{:?}", action);
    let variant = debug.split(['(', '{', ' ']).next().unwrap_or(&debug);
    let mut signature = to_snake_case(variant);
    match action {
        Action::SwitchToMode { input_mode: mode }
        | Action::SwitchModeForAllClients { input_mode: mode } => {
            signature.push('_');
            signature.push_str(&to_snake_case(&format!("{:?}", mode)));
        }
        // Directional variants are distinct hints with distinct labels (`split
        // down` vs `new`), so they need distinct signatures to be addressable:
        // `label_new_pane_down` rather than one `label_new_pane` for all five.
        Action::NewPane {
            direction: Some(direction),
            ..
        }
        | Action::MovePane {
            direction: Some(direction),
        }
        | Action::MoveFocus { direction }
        | Action::MoveFocusOrTab { direction }
        | Action::MoveTab { direction } => {
            signature.push('_');
            signature.push_str(&to_snake_case(&format!("{:?}", direction)));
        }
        Action::Resize { resize, direction } => {
            signature.push('_');
            signature.push_str(&to_snake_case(&format!("{:?}", resize)));
            if let Some(direction) = direction {
                signature.push('_');
                signature.push_str(&to_snake_case(&format!("{:?}", direction)));
            }
        }
        _ => {}
    }
    signature
}

pub(crate) fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Actions that are never useful as hints (text input, mouse, programmatic /
/// CLI-only actions). These are skipped during discovery.
pub(crate) fn is_hidden_action(action: &Action) -> bool {
    matches!(
        action,
        Action::Write { .. }
            | Action::WriteChars { .. }
            | Action::PaneNameInput { .. }
            | Action::TabNameInput { .. }
            | Action::SessionNameInput { .. }
            | Action::SearchInput { .. }
            | Action::RenameTab { .. }
            | Action::RenameTerminalPane { .. }
            | Action::RenamePluginPane { .. }
            | Action::NoOp
            | Action::MouseEvent { .. }
            | Action::ScrollUpAt { .. }
            | Action::ScrollDownAt { .. }
            | Action::CliPipe { .. }
            | Action::KeybindPipe { .. }
            | Action::DumpScreen { .. }
            | Action::DumpLayout
            | Action::ListClients
            | Action::QueryTabNames
            | Action::Run { .. }
            | Action::SkipConfirm { .. }
            | Action::StackPanes { .. }
            | Action::ChangeFloatingPaneCoordinates { .. }
    )
}

/// The built-in action -> (id, label) table. `id` is the concept the hint
/// belongs to — the merge key and the `label_<id>` config key — and `label` is
/// what gets displayed. Returns `None` for actions with no curated entry, in
/// which case the caller derives both from the action signature.
///
/// Ids must be unique per concept even where labels repeat: `new_pane` and
/// `new_tab` both display "new" but must never merge. Conversely, actions that
/// deliberately form one hint share an id (the four `MoveFocus` directions).
pub(crate) fn builtin_hint(action: &Action) -> Option<(&'static str, &'static str)> {
    if let Action::SwitchToMode { input_mode: mode }
    | Action::SwitchModeForAllClients { input_mode: mode } = action
    {
        return switch_mode_hint(mode);
    }

    Some(match action {
        Action::Quit => ("quit", "quit"),
        Action::Detach => ("detach", "detach"),

        // Panes
        Action::NewPane {
            direction: None, ..
        } => ("new_pane", "new"),
        Action::NewPane {
            direction: Some(Direction::Left),
            ..
        } => ("split_left", "split left"),
        Action::NewPane {
            direction: Some(Direction::Right),
            ..
        } => ("split_right", "split right"),
        Action::NewPane {
            direction: Some(Direction::Up),
            ..
        } => ("split_up", "split up"),
        Action::NewPane {
            direction: Some(Direction::Down),
            ..
        } => ("split_down", "split down"),
        Action::NewStackedPane { .. } => ("stacked_pane", "stacked"),
        Action::NewFloatingPane { .. } => ("floating_pane", "floating"),
        Action::NewInPlacePane { .. } => ("in_place_pane", "in place"),
        Action::CloseFocus => ("close_pane", "close"),
        Action::ToggleFocusFullscreen => ("fullscreen", "fullscreen"),
        Action::ToggleFloatingPanes => ("float", "float"),
        Action::TogglePaneEmbedOrFloating => ("embed", "embed"),
        Action::TogglePaneFrames => ("frames", "frames"),
        Action::TogglePanePinned => ("pin", "pin"),
        Action::TogglePaneInGroup => ("group_pane", "group"),
        Action::ToggleGroupMarking => ("group_marking", "mark"),
        // Directing focus is "focus"; only actions that relocate a pane are
        // "move". Distinct ids keep them apart from each other and from the
        // `SwitchToMode(Move)` hint, which means something else entirely.
        Action::MoveFocus { direction: _ } | Action::MoveFocusOrTab { direction: _ } => {
            ("focus", "focus")
        }
        Action::MovePane { direction: _ } => ("move_pane", "move"),
        Action::MovePaneBackwards => ("move_pane_back", "move back"),
        Action::FocusNextPane => ("next_pane", "next"),
        Action::FocusPreviousPane => ("prev_pane", "prev"),
        Action::SwitchFocus => ("toggle_focus", "toggle focus"),

        // Tabs
        Action::NewTab { .. } => ("new_tab", "new"),
        Action::CloseTab => ("close_tab", "close"),
        Action::GoToNextTab => ("next_tab", "next"),
        Action::GoToPreviousTab => ("prev_tab", "prev"),
        Action::GoToTab { index: _ } | Action::GoToTabName { .. } => ("go_to_tab", "tab"),
        Action::ToggleTab => ("toggle_tab", "toggle"),
        Action::ToggleActiveSyncTab => ("sync", "sync"),
        Action::MoveTab { direction: _ } => ("move_tab", "move tab"),
        Action::BreakPane => ("break_pane", "break pane"),
        Action::BreakPaneLeft => ("break_left", "break left"),
        Action::BreakPaneRight => ("break_right", "break right"),

        // Resize
        Action::Resize {
            resize: Resize::Increase,
            direction: None,
        }
        | Action::Resize {
            resize: Resize::Decrease,
            direction: None,
        } => ("resize", "resize"),
        Action::Resize {
            resize: Resize::Increase,
            direction: Some(_),
        } => ("increase", "increase"),
        Action::Resize {
            resize: Resize::Decrease,
            direction: Some(_),
        } => ("decrease", "decrease"),

        // Scroll / search
        Action::ScrollUp | Action::ScrollDown => ("scroll", "scroll"),
        Action::PageScrollUp | Action::PageScrollDown => ("page", "page"),
        Action::HalfPageScrollUp | Action::HalfPageScrollDown => ("half_page", "half page"),
        Action::ScrollToTop => ("top", "top"),
        Action::ScrollToBottom => ("bottom", "bottom"),
        Action::EditScrollback { .. } => ("edit", "edit"),
        Action::Search {
            direction: SearchDirection::Down,
        } => ("search_down", "down"),
        Action::Search {
            direction: SearchDirection::Up,
        } => ("search_up", "up"),
        Action::SearchToggleOption { option: _ } => ("search_toggle", "toggle"),

        // Misc
        Action::Copy => ("copy", "copy"),
        Action::ClearScreen => ("clear", "clear"),
        Action::ToggleMouseMode => ("mouse", "mouse"),
        Action::PreviousSwapLayout => ("prev_layout", "prev layout"),
        Action::NextSwapLayout => ("next_layout", "next layout"),
        Action::Confirm => ("confirm", "confirm"),
        Action::Deny => ("deny", "deny"),
        Action::RenameSession { name: _ } => ("rename_session", "rename"),
        Action::UndoRenamePane | Action::UndoRenameTab | Action::UndoRenameSession => {
            ("undo_rename", "undo")
        }

        _ => return None,
    })
}

/// Id and label for a `SwitchToMode` action, based on the target mode. Ids are
/// `mode_`-prefixed so that, e.g., switching to Move mode never merges with the
/// `MovePane` hint that shares its label.
pub(crate) fn switch_mode_hint(mode: &InputMode) -> Option<(&'static str, &'static str)> {
    Some(match mode {
        InputMode::Normal => ("mode_normal", "normal"),
        InputMode::Locked => ("mode_locked", "lock"),
        InputMode::Pane => ("mode_pane", "pane"),
        InputMode::Tab => ("mode_tab", "tab"),
        InputMode::Resize => ("mode_resize", "resize"),
        InputMode::Move => ("mode_move", "move"),
        InputMode::Scroll => ("mode_scroll", "scroll"),
        InputMode::Search | InputMode::EnterSearch => ("mode_search", "search"),
        InputMode::Session => ("mode_session", "session"),
        InputMode::RenameTab | InputMode::RenamePane | InputMode::RenameSession => {
            ("mode_rename", "rename")
        }
        InputMode::Tmux => ("mode_tmux", "tmux"),
        InputMode::Prompt => ("mode_prompt", "prompt"),
    })
}

pub(crate) fn get_keymap_for_mode(mode_info: &ModeInfo) -> Vec<(KeyWithModifier, Vec<Action>)> {
    match mode_info.mode {
        InputMode::Normal => mode_info.get_keybinds_for_mode(InputMode::Normal),
        InputMode::Pane => mode_info.get_keybinds_for_mode(InputMode::Pane),
        InputMode::Tab => mode_info.get_keybinds_for_mode(InputMode::Tab),
        InputMode::Resize => mode_info.get_keybinds_for_mode(InputMode::Resize),
        InputMode::Move => mode_info.get_keybinds_for_mode(InputMode::Move),
        InputMode::Scroll => mode_info.get_keybinds_for_mode(InputMode::Scroll),
        InputMode::Search => mode_info.get_keybinds_for_mode(InputMode::Search),
        InputMode::Session => mode_info.get_keybinds_for_mode(InputMode::Session),
        _ => mode_info.get_mode_keybinds(),
    }
}
