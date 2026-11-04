use super::*;
use std::cell::RefCell;
use std::collections::HashMap;
use zjkit::ansi::{AnsiParser, AnsiSegment};
use zjkit::host::{Call, Recorder};
use zjkit::mode::render_mode_prefix;
use zjkit::popup::PreviewPopup;
use zjkit::settings::live;
use zjkit::settings::screen::SettingsScreen;
use zjkit::visibility::{self, Request};

/// Sort a string of characters as if they were the keys of one hint, so an
/// expected ordering can be written as a plain string literal.
fn sorted(chars: &str, order: KeyOrder) -> String {
    let mut keys: Vec<KeyWithModifier> = chars
        .chars()
        .map(|c| KeyWithModifier::new(BareKey::Char(c)))
        .collect();
    sort_keys(&mut keys, order);
    keys.iter()
        .map(|key| match key.bare_key {
            BareKey::Char(c) => c,
            _ => '?',
        })
        .collect()
}

#[test]
fn every_layout_covers_the_alphabet_and_digits_exactly_once() {
    for order in [KeyOrder::Qwerty, KeyOrder::Dvorak, KeyOrder::Colemak] {
        let all: String = order.rows().concat();
        for expected in "abcdefghijklmnopqrstuvwxyz0123456789".chars() {
            assert_eq!(
                all.matches(expected).count(),
                1,
                "{:?} should place {:?} exactly once",
                order.rows(),
                expected
            );
        }
    }
}

#[test]
fn digits_follow_the_digit_row_so_zero_comes_last() {
    assert_eq!(sorted("271543689", KeyOrder::Qwerty), "123456789");
    assert_eq!(sorted("0159", KeyOrder::Qwerty), "1590");
}

#[test]
fn letters_sort_by_row_then_column() {
    assert_eq!(sorted("kh", KeyOrder::Qwerty), "hk");
    assert_eq!(sorted("lkjh", KeyOrder::Qwerty), "hjkl");
    // Rows run top to bottom: q above a above z.
    assert_eq!(sorted("zaq", KeyOrder::Qwerty), "qaz");
}

#[test]
fn letters_group_ahead_of_punctuation_rather_than_interleaving() {
    assert_eq!(sorted("op\\as", KeyOrder::Qwerty), "opas\\");
}

#[test]
fn digits_letters_and_punctuation_stay_in_separate_groups() {
    assert_eq!(sorted("a1/", KeyOrder::Qwerty), "1a/");
    assert_eq!(sorted("/1a", KeyOrder::Qwerty), "1a/");
}

#[test]
fn layout_changes_where_letters_land() {
    // hjkl only reads in order on the layout it was designed for. Dvorak
    // scatters it across all three rows — l on the top, h on the home row,
    // j and k down on the bottom — so the vim ordering is lost.
    assert_eq!(sorted("hjkl", KeyOrder::Qwerty), "hjkl");
    assert_eq!(sorted("hjkl", KeyOrder::Dvorak), "lhjk");
    assert_eq!(sorted("arst", KeyOrder::Colemak), "arst");
}

#[test]
fn alphabetical_ignores_the_keyboard_layout() {
    assert_eq!(sorted("dbca", KeyOrder::Alphabetical), "abcd");
    // The layout modes would order these by position instead.
    assert_eq!(sorted("lkjh", KeyOrder::Alphabetical), "hjkl");
    assert_eq!(sorted("zaq", KeyOrder::Alphabetical), "aqz");
    assert_eq!(sorted("qaz", KeyOrder::Qwerty), "qaz");
}

#[test]
fn alphabetical_puts_zero_first_unlike_the_digit_row() {
    // Plain ascending order, so `0` leads rather than trailing `9`.
    assert_eq!(sorted("0159", KeyOrder::Alphabetical), "0159");
    assert_eq!(sorted("0159", KeyOrder::Qwerty), "1590");
}

#[test]
fn alphabetical_still_groups_digits_letters_and_punctuation() {
    assert_eq!(sorted("/a1", KeyOrder::Alphabetical), "1a/");
}

#[test]
fn unsorted_preserves_the_order_zellij_reported() {
    assert_eq!(sorted("271543689", KeyOrder::Unsorted), "271543689");
    assert_eq!(sorted("op\\as", KeyOrder::Unsorted), "op\\as");
}

#[test]
fn arrows_follow_hjkl_order_not_declaration_order() {
    let mut keys: Vec<KeyWithModifier> =
        [BareKey::Right, BareKey::Up, BareKey::Left, BareKey::Down]
            .into_iter()
            .map(KeyWithModifier::new)
            .collect();
    sort_keys(&mut keys, KeyOrder::Qwerty);
    let bare: Vec<BareKey> = keys.iter().map(|k| k.bare_key.clone()).collect();
    assert_eq!(
        bare,
        vec![BareKey::Left, BareKey::Down, BareKey::Up, BareKey::Right]
    );
}

#[test]
fn function_keys_sort_numerically_not_lexically() {
    let mut keys: Vec<KeyWithModifier> = [10u8, 2, 1]
        .into_iter()
        .map(|n| KeyWithModifier::new(BareKey::F(n)))
        .collect();
    sort_keys(&mut keys, KeyOrder::Qwerty);
    let bare: Vec<BareKey> = keys.iter().map(|k| k.bare_key.clone()).collect();
    assert_eq!(bare, vec![BareKey::F(1), BareKey::F(2), BareKey::F(10)]);
}

#[test]
fn modifier_groups_run_unmodified_then_ctrl_super_alt_shift() {
    let plain = KeyWithModifier::new(BareKey::Char('a'));
    let mut keys = vec![
        plain.clone().with_shift_modifier(),
        plain.clone().with_alt_modifier(),
        plain.clone(),
        plain.clone().with_super_modifier(),
        plain.clone().with_ctrl_modifier(),
    ];
    sort_keys(&mut keys, KeyOrder::Qwerty);
    // Assert the modifiers themselves, not just that the ranks ascend —
    // ascending ranks would hold for any ordering.
    let groups: Vec<Vec<KeyModifier>> = keys
        .iter()
        .map(|key| key.key_modifiers.iter().copied().collect())
        .collect();
    assert_eq!(
        groups,
        vec![
            vec![],
            vec![KeyModifier::Ctrl],
            vec![KeyModifier::Super],
            vec![KeyModifier::Alt],
            vec![KeyModifier::Shift],
        ]
    );
}

#[test]
fn a_key_sorts_with_the_strongest_modifier_it_carries() {
    let plain = KeyWithModifier::new(BareKey::Char('p'));
    let ctrl_shift = plain.clone().with_ctrl_modifier().with_shift_modifier();
    assert_eq!(modifier_rank(&ctrl_shift.key_modifiers), 1);
    // Same group as plain Ctrl, but still ordered after it.
    let ctrl = plain.with_ctrl_modifier();
    assert_eq!(modifier_rank(&ctrl.key_modifiers), 1);
    assert!(modifier_tiebreak(&ctrl.key_modifiers) < modifier_tiebreak(&ctrl_shift.key_modifiers));
}

/// Build hints with the given ids (label mirrors the id unless it contains
/// `/`, written as `id/label`), order them, and read back the resulting ids.
fn ordered(ids: &[&str], config: &str) -> Vec<String> {
    let mut hints: Vec<Hint> = ids
        .iter()
        .map(|spec| {
            let (id, label) = spec.split_once('/').unwrap_or((spec, spec));
            Hint {
                id: id.to_string(),
                merge_key: id.to_string(),
                label: label.to_string(),
                keys: vec![],
            }
        })
        .collect();
    let order = HintOrder::from_config(config);
    if !order.is_empty() {
        hints.sort_by_key(|hint| order.rank(hint));
    }
    hints.into_iter().map(|hint| hint.id).collect()
}

#[test]
fn wildcard_splits_pinned_front_from_pinned_back() {
    assert_eq!(
        ordered(&["a", "b", "c", "d"], "d, *, a"),
        vec!["d", "b", "c", "a"]
    );
}

#[test]
fn entries_before_the_wildcard_lead_in_the_order_given() {
    assert_eq!(ordered(&["a", "b", "c"], "c, b, *"), vec!["c", "b", "a"]);
}

#[test]
fn entries_after_the_wildcard_trail_in_the_order_given() {
    assert_eq!(ordered(&["a", "b", "c"], "*, b, a"), vec!["c", "b", "a"]);
}

#[test]
fn a_list_without_a_wildcard_leads_and_the_rest_follow() {
    assert_eq!(ordered(&["a", "b", "c"], "c, a"), vec!["c", "a", "b"]);
}

#[test]
fn unlisted_hints_keep_the_order_the_mode_built_them() {
    assert_eq!(
        ordered(&["a", "b", "c", "d", "e"], "e, *"),
        vec!["e", "a", "b", "c", "d"]
    );
}

#[test]
fn an_empty_or_absent_order_changes_nothing() {
    assert_eq!(ordered(&["b", "a"], ""), vec!["b", "a"]);
    assert_eq!(ordered(&["b", "a"], "  ,  "), vec!["b", "a"]);
}

#[test]
fn naming_an_absent_hint_is_harmless() {
    assert_eq!(ordered(&["a", "b"], "nope, *, alsonope"), vec!["a", "b"]);
}

#[test]
fn hints_can_be_named_by_label_as_well_as_id() {
    assert_eq!(
        ordered(&["a", "next_layout/swap layout"], "swap layout, *"),
        vec!["next_layout", "a"]
    );
}

#[test]
fn matching_ignores_case_and_surrounding_space() {
    assert_eq!(ordered(&["a", "Quit"], "  QUIT  , *"), vec!["Quit", "a"]);
}

/// Render a mode from a synthetic keymap and return the visible text, so a
/// whole hint line can be asserted as a plain string.
fn rendered(
    mode: InputMode,
    keymap: &[(KeyWithModifier, Vec<Action>)],
    config: &[(&str, &str)],
) -> String {
    let colors = Styling::default();
    let config = label_config(config);
    let hint_order = HintOrder::from_config(config.get("hint_order").map_or("", |v| v));
    let mode_key = format!("{:?}", mode).to_lowercase();
    let style = HintStyle {
        colors: &colors,
        dim: config
            .get("test_dim_strength")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.0),
        // Minimal formats keep assertions readable; a test can override
        // them through config to exercise the real precedence.
        key_format: config
            .get("key_format")
            .map(String::as_str)
            .or(Some("{key} ")),
        desc_format: config
            .get("desc_format")
            .map(String::as_str)
            .or(Some("{desc}")),
        spacer: Some("|"),
        spacer_after_prefix: false,
        discover: config
            .get("discover_hints")
            .map_or(DEFAULT_DISCOVER_HINTS, |v| v == "true"),
        direction_keys: DirectionKeys::from_config(
            config.get("direction_keys").map_or("both", |v| v),
        ),
        key_order: KeyOrder::default(),
        mode: &mode_key,
        hint_order: &hint_order,
        limit: config.get("limit").and_then(|v| v.parse::<usize>().ok()),
        drop_indicator: config.get("drop_indicator").map(|v| v.as_str()),
        precedence: Precedence::from_config(config.get("hint_precedence").map_or("", |v| v)),
        shared: &[],
        config: &config,
        click: ClickBehavior::RunActions,
    };
    let (parts, _) = lay_out_hints_for_mode(mode, keymap, &style);
    let text = format!("{}", ANSIStrings(&parts));
    let mut parser = AnsiParser::new(&text);
    let mut visible = String::new();
    while let Some(segment) = parser.next_segment() {
        if let AnsiSegment::VisibleChar(ch) = segment {
            visible.push(ch);
        }
    }
    visible
}

fn key(bare: BareKey) -> KeyWithModifier {
    KeyWithModifier::new(bare)
}

fn ctrl(c: char) -> KeyWithModifier {
    KeyWithModifier::new(BareKey::Char(c)).with_ctrl_modifier()
}

/// `MessagePlugin` in KDL, which parses to this plumbing action.
fn message_plugin() -> Action {
    Action::KeybindPipe {
        name: None,
        payload: Some("enable".to_string()),
        args: None,
        plugin: Some("autolock".to_string()),
        plugin_id: None,
        configuration: None,
        launch_new: false,
        skip_cache: false,
        floating: None,
        in_place: None,
        cwd: None,
        pane_title: None,
    }
}

#[test]
fn a_binding_led_by_a_plumbing_action_still_shows_what_it_does() {
    // Locked mode's only binding is `MessagePlugin ...; SwitchToMode
    // "Normal"`. Judging it by its first action alone hid the mode entirely.
    let keymap = vec![(ctrl('g'), vec![message_plugin(), TO_NORMAL])];
    assert_eq!(rendered(InputMode::Locked, &keymap, &[]), "Ctrl g normal");
    assert_eq!(
        rendered(
            InputMode::Locked,
            &keymap,
            &[("label_locked_mode_normal", "unlock")]
        ),
        "Ctrl g unlock"
    );
}

#[test]
fn a_binding_of_pure_plumbing_stays_hidden() {
    let keymap = vec![(ctrl('g'), vec![message_plugin()])];
    assert_eq!(rendered(InputMode::Locked, &keymap, &[]), "");
}

/// Tab navigation as Zellij binds it by default: both families, both ways.
fn tab_nav_keymap() -> Vec<(KeyWithModifier, Vec<Action>)> {
    vec![
        (key(BareKey::Char('h')), vec![Action::GoToPreviousTab]),
        (key(BareKey::Left), vec![Action::GoToPreviousTab]),
        (key(BareKey::Char('l')), vec![Action::GoToNextTab]),
        (key(BareKey::Right), vec![Action::GoToNextTab]),
    ]
}

#[test]
fn tab_focus_honors_direction_keys_and_claims_every_key_it_shows() {
    // With "letters", the arrows drop out and no stray next/prev hints are
    // left behind for discovery to resurface.
    assert_eq!(
        rendered(
            InputMode::Tab,
            &tab_nav_keymap(),
            &[("direction_keys", "letters")]
        ),
        "hl focus"
    );
    assert_eq!(
        rendered(
            InputMode::Tab,
            &tab_nav_keymap(),
            &[("direction_keys", "arrows")]
        ),
        "←→ focus"
    );
}

#[test]
fn tab_focus_shows_both_families_by_default() {
    assert_eq!(
        rendered(InputMode::Tab, &tab_nav_keymap(), &[]),
        "hl←→ focus"
    );
}

#[test]
fn the_exit_hint_honours_the_action_name_label() {
    let keymap = vec![(key(BareKey::Esc), vec![TO_NORMAL])];
    // The action-name form is what the built-in status bar used, so configs
    // carried over from it rely on this reaching the exit hint.
    assert_eq!(
        rendered(
            InputMode::Tab,
            &keymap,
            &[("label_switch_to_mode_normal", "exit")]
        ),
        "ESC exit"
    );
    // The id form keeps working too.
    assert_eq!(
        rendered(InputMode::Tab, &keymap, &[("label_mode_normal", "exit")]),
        "ESC exit"
    );
}

#[test]
fn the_exit_hint_honours_a_mode_scoped_action_name_label() {
    let keymap = vec![(key(BareKey::Esc), vec![TO_NORMAL])];
    let config = &[
        ("label_switch_to_mode_normal", "exit"),
        ("label_locked_switch_to_mode_normal", "unlock"),
    ];
    assert_eq!(rendered(InputMode::Locked, &keymap, config), "ESC unlock");
    assert_eq!(rendered(InputMode::Tab, &keymap, config), "ESC exit");
}

#[test]
fn a_relabelled_exit_hint_pins_under_its_new_label() {
    // The regression cascade: when the label did not apply, the hint stayed
    // "normal", so `hint_order "*, exit"` never matched it, so it sat in the
    // droppable middle instead of pinned last. With the label applied it is
    // pinned and survives a narrow bar that drops the middle.
    let keymap = vec![
        (key(BareKey::Char('a')), vec![Action::CloseFocus]),
        (key(BareKey::Char('b')), vec![Action::ToggleFocusFullscreen]),
        (key(BareKey::Esc), vec![TO_NORMAL]),
    ];
    let config = &[
        ("label_switch_to_mode_normal", "exit"),
        ("hint_order", "*, exit"),
        ("limit", "12"),
    ];
    let out = rendered(InputMode::Tab, &keymap, config);
    assert!(
        out.contains("exit"),
        "exit was dropped, not pinned: {:?}",
        out
    );
}

#[test]
fn every_key_that_leaves_a_mode_forms_one_hint() {
    // Enter and Esc are bound to the identical action, so they are one hint
    // rather than a "select" and an "exit" for the same thing.
    let keymap = vec![
        (key(BareKey::Enter), vec![TO_NORMAL]),
        (key(BareKey::Esc), vec![TO_NORMAL]),
    ];
    assert_eq!(rendered(InputMode::Pane, &keymap, &[]), "ESCENTER normal");
    assert_eq!(rendered(InputMode::Tab, &keymap, &[]), "ESCENTER normal");
}

#[test]
fn discovery_is_on_unless_it_is_turned_off() {
    // `z` is bound to an action the curated Pane list does not name, so
    // "pin" reaches the bar only through discovery. Nothing in the config
    // asks for it either way.
    let keymap = vec![
        (key(BareKey::Char('x')), vec![Action::CloseFocus, TO_NORMAL]),
        (key(BareKey::Char('z')), vec![Action::TogglePanePinned]),
        (key(BareKey::Esc), vec![TO_NORMAL]),
    ];
    assert!(rendered(InputMode::Pane, &keymap, &[]).contains("pin"));
    assert!(!rendered(InputMode::Pane, &keymap, &[("discover_hints", "false")]).contains("pin"));
}

#[test]
fn a_mode_keeps_its_exit_hint_without_discovery() {
    // The curated list has to carry the escape hatch itself, since with
    // discovery off there is nothing else to find it.
    let keymap = vec![
        (key(BareKey::Char('x')), vec![Action::CloseFocus, TO_NORMAL]),
        (key(BareKey::Esc), vec![TO_NORMAL]),
    ];
    assert_eq!(
        rendered(InputMode::Pane, &keymap, &[("discover_hints", "false")]),
        "x close|ESC normal"
    );
}

#[test]
fn a_mode_scoped_label_reaches_the_rendered_line() {
    let config = &[
        ("label_mode_normal", "exit"),
        ("label_locked_mode_normal", "unlock"),
    ];
    // Locked binds only the one escape hatch.
    let locked = vec![(key(BareKey::Esc), vec![TO_NORMAL])];
    assert_eq!(rendered(InputMode::Locked, &locked, config), "ESC unlock");
    // Pane binds Enter and Esc to the same action, so they form one hint
    // carrying both keys, under the global label.
    let pane = vec![
        (key(BareKey::Enter), vec![TO_NORMAL]),
        (key(BareKey::Esc), vec![TO_NORMAL]),
    ];
    assert_eq!(rendered(InputMode::Pane, &pane, config), "ESCENTER exit");
}

/// Four one-key hints, each rendering as `<key> <label>` — 6 columns wide
/// with the test harness's formats, plus a 1-column spacer between them.
fn four_hints() -> Vec<(KeyWithModifier, Vec<Action>)> {
    vec![
        (key(BareKey::Char('a')), vec![Action::CloseFocus]),
        (key(BareKey::Char('b')), vec![Action::ToggleFocusFullscreen]),
        (key(BareKey::Char('c')), vec![Action::TogglePaneFrames]),
        (key(BareKey::Char('d')), vec![Action::TogglePanePinned]),
    ]
}

#[test]
fn a_per_hint_format_overrides_the_global_one() {
    let keymap = four_hints();
    let config = &[
        ("desc_format", "[{desc}]"),
        ("desc_format_frames", "<{desc}>"),
    ];
    assert_eq!(
        rendered(InputMode::Pane, &keymap, config),
        "a [close]|b [fullscreen]|c <frames>|d [pin]"
    );
}

#[test]
fn a_per_hint_format_can_be_scoped_to_one_mode() {
    let keymap = four_hints();
    assert_eq!(
        rendered(
            InputMode::Pane,
            &keymap,
            &[("desc_format_pane_frames", "<{desc}>")]
        ),
        "a close|b fullscreen|c <frames>|d pin"
    );
    // A different mode keeps the global formatting.
    assert_eq!(
        rendered(
            InputMode::Tab,
            &keymap,
            &[("desc_format_pane_frames", "<{desc}>")]
        ),
        "a close|b fullscreen|c frames|d pin"
    );
}

#[test]
fn keys_can_be_replaced_with_a_fixed_string() {
    assert_eq!(
        rendered(InputMode::Pane, &four_hints(), &[("keys_frames", "1-9")]),
        "a close|b fullscreen|1-9 frames|d pin"
    );
}

#[test]
fn an_empty_key_replacement_leaves_the_label_alone() {
    assert_eq!(
        rendered(InputMode::Pane, &four_hints(), &[("keys_frames", "")]),
        "a close|b fullscreen|frames|d pin"
    );
}

#[test]
fn a_replaced_key_is_measured_at_its_real_width() {
    // The replacement is wider than the key it stands for, so fitting has
    // to account for it rather than for the original.
    let wide = rendered(
        InputMode::Pane,
        &four_hints(),
        &[("keys_frames", "mouse wheel"), ("limit", "40")],
    );
    assert!(
        calculate_visible_length(&wide) <= 40,
        "over the limit: {:?}",
        wide
    );
}

#[test]
fn a_chord_alias_replaces_its_exact_modifier_combination() {
    // A custom leader key programmed to send Ctrl+Alt+Shift at once
    // should render as one symbol, not three modifier names.
    let keymap = vec![(
        KeyWithModifier::new(BareKey::Char('p'))
            .with_ctrl_modifier()
            .with_alt_modifier()
            .with_shift_modifier(),
        vec![Action::CloseFocus],
    )];
    let config = &[
        ("chord_mods_leader", "ctrl+alt+shift"),
        ("chord_alias_leader", "&"),
    ];
    assert_eq!(rendered(InputMode::Pane, &keymap, config), "&p close");
}

#[test]
fn a_chord_alias_ignores_a_partial_modifier_match() {
    // Only Ctrl is held, not the full chord, so the normal modifier name
    // is shown rather than the alias.
    let keymap = vec![(ctrl('p'), vec![Action::CloseFocus])];
    let config = &[
        ("chord_mods_leader", "ctrl+alt+shift"),
        ("chord_alias_leader", "&"),
    ];
    assert_eq!(rendered(InputMode::Pane, &keymap, config), "Ctrl p close");
}

#[test]
fn multiple_chords_apply_independently() {
    let keymap = vec![
        (
            KeyWithModifier::new(BareKey::Char('p'))
                .with_ctrl_modifier()
                .with_alt_modifier(),
            vec![Action::CloseFocus],
        ),
        (
            KeyWithModifier::new(BareKey::Char('t'))
                .with_ctrl_modifier()
                .with_shift_modifier(),
            vec![Action::TogglePaneFrames],
        ),
    ];
    let config = &[
        ("chord_mods_one", "ctrl+alt"),
        ("chord_alias_one", "&"),
        ("chord_mods_two", "ctrl+shift"),
        ("chord_alias_two", "%"),
    ];
    assert_eq!(
        rendered(InputMode::Pane, &keymap, config),
        "&p close|%t frames"
    );
}

#[test]
fn a_chord_alias_is_measured_at_its_real_width() {
    // The alias is narrower than the spelled-out chord, so a limit that
    // just fits "&p close" (8 columns) would drop this hint entirely if
    // fitting were still measuring the unaliased modifier names.
    let keymap = vec![(
        KeyWithModifier::new(BareKey::Char('p'))
            .with_ctrl_modifier()
            .with_alt_modifier()
            .with_shift_modifier()
            .with_super_modifier(),
        vec![Action::CloseFocus],
    )];
    let text = rendered(
        InputMode::Pane,
        &keymap,
        &[
            ("chord_mods_leader", "ctrl+alt+shift+super"),
            ("chord_alias_leader", "&"),
            ("limit", "8"),
        ],
    );
    assert_eq!(text, "&p close");
}

#[test]
fn an_unrecognized_chord_mods_token_disables_that_chord() {
    let keymap = vec![(ctrl('p'), vec![Action::CloseFocus])];
    let config = &[
        ("chord_mods_leader", "ctrl+foo"),
        ("chord_alias_leader", "&"),
    ];
    assert_eq!(rendered(InputMode::Pane, &keymap, config), "Ctrl p close");
}

#[test]
fn a_chord_mods_without_a_paired_alias_has_no_effect() {
    let keymap = vec![(ctrl('p'), vec![Action::CloseFocus])];
    assert_eq!(
        rendered(InputMode::Pane, &keymap, &[("chord_mods_leader", "ctrl")]),
        "Ctrl p close"
    );
}

#[test]
fn a_hint_can_be_named_by_its_label_as_well_as_its_id() {
    // `close_pane` is the id; `close` is what it displays.
    assert_eq!(
        rendered(InputMode::Pane, &four_hints(), &[("keys_close", "^w")]),
        "^w close|b fullscreen|c frames|d pin"
    );
}

#[test]
fn a_relabelled_hint_is_addressable_by_its_new_label() {
    assert_eq!(
        rendered(
            InputMode::Pane,
            &four_hints(),
            &[("label_frames", "swap layout"), ("keys_swap_layout", "><")]
        ),
        "a close|b fullscreen|>< swap layout|d pin"
    );
}

#[test]
fn a_relabelled_hint_keeps_its_id() {
    // The label changes what the hint shows, never what config calls it.
    assert_eq!(
        rendered(
            InputMode::Pane,
            &four_hints(),
            &[
                ("label_frames", "◆"),
                ("keys_frames", "><"),
                ("hint_order", "frames, *"),
            ]
        ),
        ">< ◆|a close|b fullscreen|d pin"
    );
}

#[test]
fn hints_given_one_label_become_one_hint() {
    assert_eq!(
        rendered(
            InputMode::Pane,
            &four_hints(),
            &[("label_close_pane", "both"), ("label_pin", "both")]
        ),
        "ad both|b fullscreen|c frames"
    );
}

#[test]
fn hints_are_dropped_from_the_right_to_fit() {
    let all = rendered(InputMode::Pane, &four_hints(), &[]);
    assert_eq!(all, "a close|b fullscreen|c frames|d pin");
    // Enough room for the first two only.
    let fitted = rendered(InputMode::Pane, &four_hints(), &[("limit", "20")]);
    assert_eq!(fitted, "a close|b fullscreen");
}

#[test]
fn unpinned_hints_yield_before_hints_pinned_to_the_end() {
    // `pin` is pinned last, so the unpinned middle drops around it.
    let fitted = rendered(
        InputMode::Pane,
        &four_hints(),
        &[("hint_order", "*, pin"), ("limit", "20")],
    );
    assert_eq!(fitted, "a close|d pin");
}

#[test]
fn hints_pinned_to_the_front_are_kept_too() {
    let fitted = rendered(
        InputMode::Pane,
        &four_hints(),
        &[("hint_order", "frames, *, pin"), ("limit", "20")],
    );
    assert_eq!(fitted, "c frames|d pin");
}

#[test]
fn the_leading_group_is_given_up_before_the_trailing_one() {
    // `close, fullscreen, *, pin`: once the `*` is gone the leading group is
    // consumed from its inner edge outward, so the trailing `pin` — the hint
    // most deliberately placed — is the last one standing.
    let order = ("hint_order", "close, fullscreen, *, pin");
    assert_eq!(
        rendered(InputMode::Pane, &four_hints(), &[order, ("limit", "26")]),
        "a close|b fullscreen|d pin"
    );
    assert_eq!(
        rendered(InputMode::Pane, &four_hints(), &[order, ("limit", "20")]),
        "a close|d pin"
    );
    assert_eq!(
        rendered(InputMode::Pane, &four_hints(), &[order, ("limit", "8")]),
        "d pin"
    );
}

#[test]
fn precedence_lt_keeps_the_leading_group_instead() {
    let order = ("hint_order", "close, fullscreen, *, pin");
    let lt = ("hint_precedence", "lt");
    // The trailing `pin` is spent first now, so the leading pair outlives it.
    assert_eq!(
        rendered(
            InputMode::Pane,
            &four_hints(),
            &[order, lt, ("limit", "22")]
        ),
        "a close|b fullscreen"
    );
    assert_eq!(
        rendered(
            InputMode::Pane,
            &four_hints(),
            &[order, lt, ("limit", "10")]
        ),
        "a close"
    );
}

#[test]
fn precedence_defaults_to_keeping_the_trailing_group() {
    let order = ("hint_order", "close, fullscreen, *, pin");
    let explicit = ("hint_precedence", "tl");
    let limit = ("limit", "10");
    assert_eq!(
        rendered(InputMode::Pane, &four_hints(), &[order, explicit, limit]),
        rendered(InputMode::Pane, &four_hints(), &[order, limit])
    );
    assert_eq!(
        rendered(InputMode::Pane, &four_hints(), &[order, limit]),
        "d pin"
    );
}

#[test]
fn precedence_still_leaves_the_dropped_hints_in_one_run() {
    // Whichever group is spent first, the gap stays contiguous.
    assert_eq!(
        rendered(
            InputMode::Pane,
            &four_hints(),
            &[
                ("hint_order", "close, fullscreen, *, pin"),
                ("hint_precedence", "lt"),
                ("drop_indicator", "…"),
                ("limit", "22")
            ]
        ),
        "a close|b fullscreen|…"
    );
}

#[test]
fn dropped_hints_stay_one_run_so_a_single_indicator_covers_them() {
    // Each drop extends the same gap rather than opening a new one, so the
    // indicator never needs a twin.
    let order = ("hint_order", "close, fullscreen, *, pin");
    let indicator = ("drop_indicator", "…");
    assert_eq!(
        rendered(
            InputMode::Pane,
            &four_hints(),
            &[order, indicator, ("limit", "30")]
        ),
        "a close|b fullscreen|…|d pin"
    );
    assert_eq!(
        rendered(
            InputMode::Pane,
            &four_hints(),
            &[order, indicator, ("limit", "20")]
        ),
        "a close|…|d pin"
    );
    assert_eq!(
        rendered(
            InputMode::Pane,
            &four_hints(),
            &[order, indicator, ("limit", "10")]
        ),
        "…|d pin"
    );
}

#[test]
fn the_indicator_marks_where_hints_were_dropped() {
    assert_eq!(
        rendered(
            InputMode::Pane,
            &four_hints(),
            &[("drop_indicator", "…"), ("limit", "22")]
        ),
        "a close|b fullscreen|…"
    );
}

#[test]
fn no_indicator_appears_when_everything_fits() {
    assert_eq!(
        rendered(
            InputMode::Pane,
            &four_hints(),
            &[("drop_indicator", "…"), ("limit", "100")]
        ),
        "a close|b fullscreen|c frames|d pin"
    );
}

#[test]
fn the_indicator_is_paid_for_out_of_the_same_budget() {
    // A wide indicator costs room of its own, so it forces a further drop
    // rather than pushing the line over the limit.
    let wide = rendered(
        InputMode::Pane,
        &four_hints(),
        &[("drop_indicator", "<more>"), ("limit", "22")],
    );
    assert!(
        calculate_visible_length(&wide) <= 22,
        "over the limit: {:?}",
        wide
    );
    assert!(wide.contains("<more>"), "indicator missing: {:?}", wide);
}

#[test]
fn one_hint_always_survives_for_truncation_to_handle() {
    let fitted = rendered(InputMode::Pane, &four_hints(), &[("limit", "2")]);
    assert_eq!(fitted, "a close");
}

#[test]
fn wide_glyphs_are_measured_when_fitting_hints() {
    // Two hints whose labels are wide glyphs. Counting characters would call
    // this 4 columns of label; it is really 8.
    let keymap = vec![
        (key(BareKey::Char('a')), vec![Action::CloseFocus]),
        (key(BareKey::Char('b')), vec![Action::ToggleFocusFullscreen]),
    ];
    // Distinct labels, or sharing one would deliberately fuse them.
    let labels: Vec<(&str, &str)> =
        vec![("label_close_pane", "字左"), ("label_fullscreen", "字右")];
    // Each hint is "a " plus a 4-column label = 6; with the 1-column spacer
    // the pair is 13 columns, though only 9 characters.
    assert_eq!(rendered(InputMode::Pane, &keymap, &labels), "a 字左|b 字右");
    // 12 columns cannot hold both, even though 9 characters would fit.
    let mut fitted = labels.clone();
    fitted.push(("limit", "12"));
    assert_eq!(rendered(InputMode::Pane, &keymap, &fitted), "a 字左");
}

fn manifest(panes: Vec<PaneInfo>) -> PaneManifest {
    let mut map = HashMap::new();
    map.insert(0, panes);
    PaneManifest { panes: map }
}

fn limits(max_length: usize, auto: bool, reserve: usize, width: Option<usize>) -> Option<usize> {
    State {
        max_length,
        auto_width: auto,
        reserve_columns: reserve,
        terminal_width: width,
        ..Default::default()
    }
    .length_limit()
}

#[test]
fn auto_width_fits_the_hints_to_the_terminal() {
    assert_eq!(limits(0, true, 0, Some(80)), Some(80));
}

#[test]
fn reserved_columns_are_kept_free_for_the_rest_of_the_bar() {
    assert_eq!(limits(0, true, 30, Some(80)), Some(50));
    // A reserve wider than the terminal floors at zero rather than wrapping.
    assert_eq!(limits(0, true, 200, Some(80)), Some(0));
}

#[test]
fn an_explicit_max_length_is_never_exceeded_on_a_wide_terminal() {
    assert_eq!(limits(40, true, 0, Some(200)), Some(40));
}

#[test]
fn auto_width_still_narrows_below_an_explicit_max_length() {
    assert_eq!(limits(100, true, 0, Some(60)), Some(60));
}

#[test]
fn nothing_is_capped_before_the_first_pane_update() {
    // Auto-fitting waits for a real width rather than guessing one.
    assert_eq!(limits(0, true, 0, None), None);
    assert_eq!(limits(40, true, 0, None), Some(40));
}

#[test]
fn a_bar_in_its_own_pane_fits_the_pane_and_reserves_nothing() {
    let state = State {
        auto_width: true,
        reserve_columns: 30,
        terminal_width: Some(200),
        bar: BarCore {
            pane_width: Some(90),
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(state.length_limit(), Some(90));
    let capped = State {
        max_length: 40,
        ..state
    };
    assert_eq!(capped.length_limit(), Some(40));
}

#[test]
fn auto_width_off_ignores_the_pane_too() {
    let state = State {
        auto_width: false,
        bar: BarCore {
            pane_width: Some(90),
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(state.length_limit(), None);
}

#[test]
fn auto_width_off_leaves_only_the_explicit_cap() {
    assert_eq!(limits(0, false, 0, Some(80)), None);
    assert_eq!(limits(40, false, 0, Some(80)), Some(40));
}

fn label_config(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn a_mode_scoped_label_beats_the_global_one() {
    let config = label_config(&[
        ("label_mode_normal", "exit"),
        ("label_locked_mode_normal", "unlock"),
    ]);
    assert_eq!(
        label_override("mode_normal", "switch_to_mode_normal", "locked", &config),
        Some(Some("unlock".to_string()))
    );
    // Every other mode still gets the global label.
    assert_eq!(
        label_override("mode_normal", "switch_to_mode_normal", "pane", &config),
        Some(Some("exit".to_string()))
    );
}

#[test]
fn a_mode_scoped_label_works_by_action_signature_too() {
    let config = label_config(&[("label_locked_switch_to_mode_normal", "unlock")]);
    assert_eq!(
        label_override("mode_normal", "switch_to_mode_normal", "locked", &config),
        Some(Some("unlock".to_string()))
    );
}

#[test]
fn concept_id_wins_over_signature_at_the_same_scope() {
    let config = label_config(&[
        ("label_mode_normal", "by id"),
        ("label_switch_to_mode_normal", "by signature"),
    ]);
    assert_eq!(
        label_override("mode_normal", "switch_to_mode_normal", "pane", &config),
        Some(Some("by id".to_string()))
    );
}

#[test]
fn a_mode_scoped_empty_label_hides_the_hint_in_that_mode_only() {
    let config = label_config(&[("label_pane_mode_normal", "")]);
    assert_eq!(
        label_override("mode_normal", "switch_to_mode_normal", "pane", &config),
        Some(None)
    );
    assert_eq!(
        label_override("mode_normal", "switch_to_mode_normal", "tab", &config),
        None
    );
}

#[test]
fn a_mode_scope_can_reinstate_a_hint_hidden_globally() {
    let config = label_config(&[
        ("label_mode_normal", ""),
        ("label_locked_mode_normal", "unlock"),
    ]);
    assert_eq!(
        label_override("mode_normal", "switch_to_mode_normal", "locked", &config),
        Some(Some("unlock".to_string()))
    );
    assert_eq!(
        label_override("mode_normal", "switch_to_mode_normal", "tab", &config),
        Some(None)
    );
}

#[test]
fn no_matching_label_leaves_the_builtin_in_place() {
    let config = label_config(&[("label_quit", "bye")]);
    assert_eq!(
        label_override("mode_normal", "switch_to_mode_normal", "locked", &config),
        None
    );
}

#[test]
fn keys_sharing_a_modifier_stay_in_one_contiguous_run() {
    let mut keys = vec![
        KeyWithModifier::new(BareKey::Char('j')),
        KeyWithModifier::new(BareKey::Char('h')).with_ctrl_modifier(),
        KeyWithModifier::new(BareKey::Char('h')),
        KeyWithModifier::new(BareKey::Char('j')).with_ctrl_modifier(),
    ];
    sort_keys(&mut keys, KeyOrder::Qwerty);
    let rendered: Vec<String> = keys
        .iter()
        .map(|k| {
            let prefix = if k.key_modifiers.is_empty() { "" } else { "^" };
            match k.bare_key {
                BareKey::Char(c) => format!("{}{}", prefix, c),
                _ => "?".to_string(),
            }
        })
        .collect();
    assert_eq!(rendered, vec!["h", "j", "^h", "^j"]);
}

fn state_with_ancestry(ancestry: Vec<&str>) -> State {
    State {
        mode_info: ModeInfo {
            session_ancestry: ancestry.into_iter().map(str::to_owned).collect(),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn a_session_with_no_ancestry_is_not_nested() {
    assert!(!state_with_ancestry(vec![]).is_nested());
}

#[test]
fn a_session_with_ancestry_is_nested() {
    assert!(state_with_ancestry(vec!["host"]).is_nested());
}

#[test]
fn is_host_descended_reflects_session_dimmed_only() {
    let mut state = State {
        mode_info: ModeInfo {
            session_dimmed: Some(true),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(state.is_host_descended());

    // session_ascended is a different signal (this session being the
    // nested, not-yet-ascended-into side); it must not trigger the
    // host-descended indicator on its own.
    state.mode_info.session_dimmed = None;
    state.mode_info.session_ascended = Some(true);
    assert!(!state.is_host_descended());
}

#[test]
fn is_host_descended_ignores_the_dim_display_options() {
    // Whether to show the descended placeholder isn't a rendering
    // preference the way dim_when_unfocused/dim_strength are: it stays
    // true even with dimming turned off.
    let state = State {
        mode_info: ModeInfo {
            session_dimmed: Some(true),
            ..Default::default()
        },
        dim_when_unfocused: false,
        dim_strength: 0.0,
        ..Default::default()
    };
    assert!(state.is_host_descended());
    assert_eq!(state.dim_amount(), 0.0);
}

fn state_with_dim(
    dim_when_unfocused: bool,
    dim_strength: f32,
    session_ascended: Option<bool>,
    session_dimmed: Option<bool>,
) -> State {
    State {
        mode_info: ModeInfo {
            session_ascended,
            session_dimmed,
            ..Default::default()
        },
        dim_when_unfocused,
        dim_strength,
        ..Default::default()
    }
}

#[test]
fn dim_amount_is_zero_when_not_dimmed() {
    assert_eq!(
        state_with_dim(true, 0.5, Some(false), None).dim_amount(),
        0.0
    );
}

#[test]
fn dim_amount_follows_session_ascended() {
    assert_eq!(
        state_with_dim(true, 0.5, Some(true), None).dim_amount(),
        0.5
    );
}

#[test]
fn dim_amount_follows_session_dimmed() {
    assert_eq!(
        state_with_dim(true, 0.7, None, Some(true)).dim_amount(),
        0.7
    );
}

#[test]
fn descended_guest_hints_are_not_dimmed_by_descending() {
    let state = state_with_dim(true, 0.5, None, Some(true));
    assert_eq!(state.dim_amount(), 0.5);
    assert_eq!(state.descended_guest_dim_amount(), 0.0);
}

#[test]
fn descended_guest_hints_dim_when_this_session_is_not_ascended_into() {
    let state = state_with_dim(true, 0.7, Some(true), Some(true));
    assert_eq!(state.descended_guest_dim_amount(), 0.7);
    assert_eq!(
        state_with_dim(false, 0.7, Some(true), Some(true)).descended_guest_dim_amount(),
        0.0
    );
}

#[test]
fn dim_amount_respects_the_config_toggle() {
    assert_eq!(
        state_with_dim(false, 0.5, Some(true), Some(true)).dim_amount(),
        0.0
    );
}

/// Test helper: build the `HintStyle` `render_descended_indicator`
/// expects and render it, so individual tests only vary the ascend
/// keys and the format overrides they're checking.
fn rendered_descended_indicator(
    ascend_keys: Vec<KeyWithModifier>,
    key_format: Option<&str>,
    desc_format: Option<&str>,
    config: &BTreeMap<String, String>,
) -> String {
    let mode_info = ModeInfo {
        nested_ascend_keys: ascend_keys,
        ..Default::default()
    };
    let colors = Styling::default();
    let hint_order = HintOrder::default();
    let style = HintStyle {
        colors: &colors,
        dim: 0.0,
        key_format,
        desc_format,
        spacer: None,
        spacer_after_prefix: false,
        discover: false,
        direction_keys: DirectionKeys::default(),
        key_order: KeyOrder::Unsorted,
        mode: "normal",
        hint_order: &hint_order,
        limit: None,
        drop_indicator: None,
        precedence: Precedence::default(),
        shared: &[],
        config,
        click: ClickBehavior::RunActions,
    };
    render_descended_indicator(&mode_info, &style)
}

fn ascend_keys() -> Vec<KeyWithModifier> {
    vec![
        KeyWithModifier::new(BareKey::Char('o')).with_ctrl_modifier(),
        KeyWithModifier::new(BareKey::Char(']')),
    ]
}

#[test]
fn descended_indicator_is_empty_without_ascend_keys() {
    assert_eq!(
        rendered_descended_indicator(vec![], None, None, &BTreeMap::new()),
        ""
    );
}

#[test]
fn descended_indicator_names_the_ascend_keys_in_order() {
    // Unsorted key_order: these are a press-this-then-that sequence, not
    // an alternative-key set a hint's usual sort_keys should reorder.
    let styled = rendered_descended_indicator(
        ascend_keys(),
        Some("{key}"),
        Some("{desc}"),
        &BTreeMap::new(),
    );
    assert_eq!(styled, "Ctrl o ]return to host");
}

#[test]
fn descended_indicator_is_styled_not_plain_text() {
    // Regression test: this used to be printed as bare, unstyled text,
    // the only thing on either bar that didn't carry the theme's
    // colors, and the one thing that visibly didn't fade when
    // dim_when_unfocused grayed everything else out around it.
    let styled = rendered_descended_indicator(ascend_keys(), None, None, &BTreeMap::new());

    assert!(styled.contains("return to host"));
    assert!(styled.contains('\u{1b}'), "expected ANSI escape codes");
}

#[test]
fn descended_indicator_honors_key_format_and_desc_format() {
    // Rendered as an ordinary hint, so the same key_format/desc_format
    // that style the curated hints style this one too.
    let styled = rendered_descended_indicator(
        ascend_keys(),
        Some("#[fg=#ff0000]<{key}>"),
        Some("#[fg=#00ff00]({desc})"),
        &BTreeMap::new(),
    );
    assert_eq!(
        styled,
        "\u{1b}[38;2;255;0;0m<Ctrl o ]>\u{1b}[38;2;0;255;0m(return to host)\u{1b}[0m"
    );
}

#[test]
fn descended_indicator_honors_a_per_hint_desc_format_override() {
    // label_descended/desc_format_descended/keys_descended all apply
    // via the normal hint_override machinery, same as any other hint.
    let config = BTreeMap::from([("desc_format_descended".to_owned(), "{desc}!".to_owned())]);
    let styled =
        rendered_descended_indicator(ascend_keys(), Some("{key}"), Some("{desc}"), &config);
    assert_eq!(styled, "Ctrl o ]return to host!");
}

#[test]
fn mode_prefix_defaults_to_the_ribbon_selected_pill() {
    let colors = Styling::default();
    let parts = render_mode_prefix(InputMode::Pane, &colors, None, &BTreeMap::new(), 0.0);
    let rendered = ANSIStrings(&parts).to_string();

    assert!(rendered.contains("PANE"));
    assert!(rendered.contains('\u{1b}'), "expected ANSI escape codes");
}

#[test]
fn mode_prefix_honors_a_custom_format() {
    let colors = Styling::default();
    let parts = render_mode_prefix(
        InputMode::Locked,
        &colors,
        Some("#[fg=#ff0000]<{mode}>"),
        &BTreeMap::new(),
        0.0,
    );
    let rendered = ANSIStrings(&parts).to_string();

    assert!(rendered.contains("<LOCKED>"));
}

#[test]
fn mode_prefix_per_mode_override_wins_over_the_global_format() {
    let colors = Styling::default();
    let config = BTreeMap::from([(
        "mode_format_locked".to_owned(),
        "[locked: {mode}]".to_owned(),
    )]);

    // Locked has its own override, so it ignores the global format...
    let locked = render_mode_prefix(InputMode::Locked, &colors, Some("<{mode}>"), &config, 0.0);
    assert_eq!(ANSIStrings(&locked).to_string(), "[locked: LOCKED]");

    // ...while a mode without its own override still falls back to it.
    let pane = render_mode_prefix(InputMode::Pane, &colors, Some("<{mode}>"), &config, 0.0);
    assert_eq!(ANSIStrings(&pane).to_string(), "<PANE>");
}

#[test]
fn mode_prefix_custom_format_dims_the_same_as_the_default_pill() {
    // The bug this guards: a custom mode_format resolves its own literal
    // colors through format::render_template, which used to ignore the
    // dim amount entirely, so a hand-styled mode pill stayed at full
    // brightness while everything else around it dimmed.
    let colors = Styling::default();
    let parts = render_mode_prefix(
        InputMode::Normal,
        &colors,
        Some("#[fg=#ff0000]{mode}"),
        &BTreeMap::new(),
        1.0,
    );
    let rendered = ANSIStrings(&parts).to_string();

    // Full dim strength on pure red lands on the same dark gray
    // zjkit::dim::color and dim_rgb produce elsewhere (see zjkit's dim tests).
    assert!(rendered.contains("\u{1b}[38;2;23;23;23m"));
}

#[test]
fn mode_prefix_draws_the_modes_label_and_symbol_for_desc_and_key() {
    let colors = Styling::default();
    let config = BTreeMap::from([
        ("mode_label_enter_search".to_owned(), "FIND".to_owned()),
        ("mode_key_enter_search".to_owned(), "/".to_owned()),
    ]);
    let format = Some("[{key}|{desc}|{mode}]");
    let search = render_mode_prefix(InputMode::EnterSearch, &colors, format, &config, 0.0);
    assert_eq!(ANSIStrings(&search).to_string(), "[/|FIND|ENTERSEARCH]");

    // A mode with neither set draws its name and no symbol.
    let pane = render_mode_prefix(InputMode::Pane, &colors, format, &config, 0.0);
    assert_eq!(ANSIStrings(&pane).to_string(), "[|PANE|PANE]");
}

#[test]
fn mode_prefix_default_pill_puts_the_symbol_before_the_label() {
    let colors = Styling::default();
    let config = BTreeMap::from([
        ("mode_label_locked".to_owned(), "LOCK".to_owned()),
        ("mode_key_locked".to_owned(), "#".to_owned()),
    ]);
    let locked = render_mode_prefix(InputMode::Locked, &colors, None, &config, 0.0);
    assert_eq!(strip_ansi(&ANSIStrings(&locked).to_string()), " # LOCK ");
    let pane = render_mode_prefix(InputMode::Pane, &colors, None, &config, 0.0);
    assert_eq!(strip_ansi(&ANSIStrings(&pane).to_string()), " PANE ");
}

#[test]
fn mode_config_suffix_matches_zjstatus_naming() {
    assert_eq!(mode_config_suffix(InputMode::Normal), "normal");
    assert_eq!(mode_config_suffix(InputMode::EnterSearch), "enter_search");
    assert_eq!(mode_config_suffix(InputMode::RenameTab), "rename_tab");
    assert_eq!(mode_config_suffix(InputMode::RenamePane), "rename_pane");
    assert_eq!(
        mode_config_suffix(InputMode::RenameSession),
        "rename_session"
    );
}

/// The pane a nested session runs in throughout these tests.
const GUEST_PANE: PaneId = PaneId::Terminal(7);
/// A second one, for checking that two nested sessions stay apart.
const OTHER_GUEST_PANE: PaneId = PaneId::Terminal(9);

/// A `PaneInfo` for a pane a nested session runs in, focused or not.
fn guest_pane(pane_id: PaneId, is_focused: bool) -> PaneInfo {
    let (id, is_plugin) = match pane_id {
        PaneId::Terminal(id) => (id, false),
        PaneId::Plugin(id) => (id, true),
    };
    PaneInfo {
        id,
        is_plugin,
        is_focused,
        ..Default::default()
    }
}

/// A `TabInfo` for the one tab these tests use.
fn only_tab() -> TabInfo {
    TabInfo {
        position: 0,
        active: true,
        ..Default::default()
    }
}

/// A keybinding table a nested session might report: one binding in Normal
/// mode that is unmistakable in a rendered hint line, and one in Pane mode
/// so mode-specific rendering can be told apart from the base mode's.
fn guest_keybinds() -> KeybindsVec {
    vec![
        (
            InputMode::Normal,
            vec![(
                ctrl('g'),
                vec![Action::SwitchToMode {
                    input_mode: InputMode::Pane,
                }],
            )],
        ),
        (
            InputMode::Pane,
            vec![(key(BareKey::Char('x')), vec![Action::CloseFocus])],
        ),
    ]
}

/// A host that has descended into a nested session in `GUEST_PANE`, with
/// the focus and tab events that identify which pane that is.
fn state_descended_into_guest() -> State {
    let mut state = State {
        mode_info: ModeInfo {
            session_dimmed: Some(true),
            ..Default::default()
        },
        ..nested_host()
    };
    state.update(Event::TabUpdate(vec![only_tab()]));
    state.update(Event::PaneUpdate(manifest(vec![
        guest_pane(GUEST_PANE, true),
        guest_pane(OTHER_GUEST_PANE, false),
    ])));
    state
}

thread_local! {
    /// Every pane a fake fetcher was asked about, in order.
    static FETCHES: RefCell<Vec<PaneId>> = const { RefCell::new(Vec::new()) };
}

fn fetches() -> Vec<PaneId> {
    FETCHES.with(|fetches| fetches.borrow().clone())
}

/// A nested session that answers with `guest_keybinds` at generation 1.
fn answering_fetch(pane_id: PaneId) -> NestedSessionKeybindsResponse {
    FETCHES.with(|fetches| fetches.borrow_mut().push(pane_id));
    Ok(NestedSessionKeybinds {
        session_path: vec!["guest".to_string()],
        mode: InputMode::Normal,
        base_mode: Some(InputMode::Normal),
        keybinds: guest_keybinds(),
        keybinds_generation: 1,
    })
}

/// A nested session too old to report its keybindings.
fn unsupported_fetch(pane_id: PaneId) -> NestedSessionKeybindsResponse {
    FETCHES.with(|fetches| fetches.borrow_mut().push(pane_id));
    Err(NestedSessionKeybindsError::NotSupported)
}

/// Only `GUEST_PANE` answers; `OTHER_GUEST_PANE` never does.
fn only_guest_pane_answers(pane_id: PaneId) -> NestedSessionKeybindsResponse {
    if pane_id == GUEST_PANE {
        answering_fetch(pane_id)
    } else {
        FETCHES.with(|fetches| fetches.borrow_mut().push(pane_id));
        Err(NestedSessionKeybindsError::Timeout)
    }
}

/// A session whose nested sessions answer through `answering_fetch`. The
/// real fetcher would block on a host that is not there.
fn nested_host() -> State {
    State {
        fetch_keybinds: KeybindsFetcher(answering_fetch),
        ..Default::default()
    }
}

fn guest_mode_update(pane_id: PaneId, mode: InputMode) -> Event {
    guest_mode_update_at(pane_id, mode, 1)
}

fn guest_mode_update_at(pane_id: PaneId, mode: InputMode, keybinds_generation: u64) -> Event {
    Event::NestedSessionModeUpdate {
        pane_id,
        session_path: vec!["guest".to_string()],
        mode,
        base_mode: Some(InputMode::Normal),
        keybinds_generation,
    }
}

/// The bar this state renders, with the styling stripped out.
///
/// `render` talks to the host, so this drives the same branch selection
/// against the state's own fields instead.
fn rendered_bar(state: &State) -> String {
    let text = match state.bar_subject() {
        BarSubject::Hints(subject) => {
            state
                .render_hint_line(&subject, 0.0, ClickBehavior::RunActions)
                .0
        }
        BarSubject::Nothing => String::new(),
        BarSubject::DescendedIndicator => DESCENDED_HINT_LABEL.to_string(),
    };
    strip_ansi(&text)
}

/// `text` with the styling stripped out.
fn strip_ansi(text: &str) -> String {
    let mut parser = AnsiParser::new(text);
    let mut visible = String::new();
    while let Some(segment) = parser.next_segment() {
        if let AnsiSegment::VisibleChar(ch) = segment {
            visible.push(ch);
        }
    }
    visible
}

fn quit_table(key: char) -> KeybindsVec {
    vec![(
        InputMode::Normal,
        vec![(KeyWithModifier::new(BareKey::Char(key)), vec![Action::Quit])],
    )]
}

fn mode_update(mode: InputMode, keybinds: KeybindsVec) -> Event {
    Event::ModeUpdate(ModeInfo {
        mode,
        keybinds,
        ..Default::default()
    })
}

#[test]
fn keybindings_sent_once_survive_later_mode_updates() {
    let mut state = State {
        initialized: true,
        ..Default::default()
    };
    assert!(state.update(Event::InitialKeybinds(quit_table('q'))));
    assert_eq!(state.mode_info.keybinds, quit_table('q'));

    // With the subscription in place Zellij strips the table from every
    // mode report.
    assert!(state.update(mode_update(InputMode::Pane, vec![])));
    assert_eq!(state.mode_info.mode, InputMode::Pane);
    assert_eq!(state.mode_info.keybinds, quit_table('q'));

    assert!(
        !state.update(mode_update(InputMode::Pane, vec![])),
        "a repeated report should not look like a change"
    );
    assert_eq!(state.mode_info.keybinds, quit_table('q'));
}

#[test]
fn reconfigured_keybindings_replace_the_held_table() {
    let mut state = State {
        initialized: true,
        ..Default::default()
    };
    state.update(Event::InitialKeybinds(quit_table('q')));
    assert!(!state.update(Event::InitialKeybinds(quit_table('q'))));
    assert!(state.update(Event::InitialKeybinds(quit_table('x'))));
    state.update(mode_update(InputMode::Normal, vec![]));
    assert_eq!(state.mode_info.keybinds, quit_table('x'));
}

#[test]
fn keybindings_arriving_ahead_of_the_first_mode_report_are_kept() {
    let mut state = State::default();
    state.update(Event::InitialKeybinds(quit_table('q')));
    state.update(mode_update(InputMode::Normal, vec![]));
    assert_eq!(state.mode_info.keybinds, quit_table('q'));
}

#[test]
fn a_zellij_without_initial_keybinds_still_supplies_them_per_mode_update() {
    let mut state = State {
        initialized: true,
        ..Default::default()
    };
    state.update(mode_update(InputMode::Normal, quit_table('q')));
    assert_eq!(state.mode_info.keybinds, quit_table('q'));

    // Nothing arrived separately, so an empty table is taken at its word.
    assert!(state.update(mode_update(InputMode::Normal, vec![])));
    assert!(state.mode_info.keybinds.is_empty());
}

#[test]
fn a_nested_session_s_keybindings_are_fetched_once_per_generation() {
    let mut state = nested_host();
    state.update(guest_mode_update(GUEST_PANE, InputMode::Normal));
    assert_eq!(
        fetches(),
        vec![GUEST_PANE],
        "the first word from a nested session should trigger a fetch"
    );
    assert!(!state.nested_guests[&GUEST_PANE].keybinds.is_empty());

    // Mode changes within the same generation reuse what was fetched.
    for mode in [InputMode::Pane, InputMode::Tab, InputMode::Normal] {
        state.update(guest_mode_update(GUEST_PANE, mode));
    }
    assert_eq!(fetches().len(), 1);

    // A new generation means the bindings behind the report may differ.
    state.update(guest_mode_update_at(GUEST_PANE, InputMode::Normal, 2));
    assert_eq!(fetches().len(), 2);
}

#[test]
fn a_failed_fetch_is_not_retried_until_the_generation_moves() {
    let mut state = State {
        fetch_keybinds: KeybindsFetcher(unsupported_fetch),
        ..Default::default()
    };
    for mode in [InputMode::Normal, InputMode::Pane, InputMode::Tab] {
        state.update(guest_mode_update(GUEST_PANE, mode));
    }
    assert_eq!(
        fetches().len(),
        1,
        "each retry would stall the plugin for nothing"
    );
    assert!(state.nested_guests[&GUEST_PANE].keybinds.is_empty());

    state.update(guest_mode_update_at(GUEST_PANE, InputMode::Tab, 2));
    assert_eq!(fetches().len(), 2);
}

#[test]
fn a_pane_that_holds_no_nested_session_is_dropped_on_fetch() {
    fn not_nested(_: PaneId) -> NestedSessionKeybindsResponse {
        Err(NestedSessionKeybindsError::NotANestedSession)
    }
    let mut state = State {
        fetch_keybinds: KeybindsFetcher(not_nested),
        ..Default::default()
    };
    state.update(guest_mode_update(GUEST_PANE, InputMode::Normal));
    assert!(state.nested_guests.is_empty());
}

#[test]
fn two_nested_sessions_are_kept_apart_by_their_panes() {
    let mut state = State {
        fetch_keybinds: KeybindsFetcher(only_guest_pane_answers),
        ..Default::default()
    };
    state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));
    state.update(guest_mode_update(OTHER_GUEST_PANE, InputMode::Tab));

    assert_eq!(state.nested_guests[&GUEST_PANE].mode, InputMode::Pane);
    assert_eq!(state.nested_guests[&OTHER_GUEST_PANE].mode, InputMode::Tab);
    // Only one session answered with keybindings.
    assert!(!state.nested_guests[&GUEST_PANE].keybinds.is_empty());
    assert!(state.nested_guests[&OTHER_GUEST_PANE].keybinds.is_empty());
}

#[test]
fn the_deepest_session_on_the_path_names_the_hints() {
    let mut state = state_descended_into_guest();
    state.update(Event::NestedSessionModeUpdate {
        pane_id: GUEST_PANE,
        session_path: vec!["middle".to_string(), "lower".to_string()],
        mode: InputMode::Pane,
        base_mode: Some(InputMode::Normal),
        keybinds_generation: 1,
    });
    let mode_info = state
        .descended_guest_mode_info()
        .expect("the descended-into session's hints");
    assert_eq!(mode_info.session_name.as_deref(), Some("lower"));
}

#[test]
fn a_nested_session_that_ends_is_forgotten() {
    let mut state = state_descended_into_guest();
    state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));
    assert!(state.update(Event::NestedSessionEnded {
        pane_id: GUEST_PANE,
        reason: NestedSessionEndReason::Unresponsive,
    }));
    assert!(state.nested_guests.is_empty());
    assert!(state.descended_guest_mode_info().is_none());
}

#[test]
fn the_descended_into_session_is_the_one_in_the_focused_pane() {
    let mut state = state_descended_into_guest();
    state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));
    state.update(guest_mode_update(OTHER_GUEST_PANE, InputMode::Tab));

    let descended = state.descended_guest().expect("a descended-into session");
    assert_eq!(descended.mode, InputMode::Pane);
}

#[test]
fn a_focused_floating_pane_wins_only_while_the_floating_layer_is_up() {
    let mut state = State::default();
    let mut floating_guest = guest_pane(OTHER_GUEST_PANE, true);
    floating_guest.is_floating = true;
    state.update(Event::PaneUpdate(manifest(vec![
        guest_pane(GUEST_PANE, true),
        floating_guest,
    ])));

    state.update(Event::TabUpdate(vec![only_tab()]));
    assert_eq!(state.focused_pane_id(), Some(GUEST_PANE));

    state.update(Event::TabUpdate(vec![TabInfo {
        are_floating_panes_visible: true,
        ..only_tab()
    }]));
    assert_eq!(state.focused_pane_id(), Some(OTHER_GUEST_PANE));
}

#[test]
fn a_focused_pane_in_another_tab_is_not_the_focused_pane() {
    let mut state = State::default();
    let mut panes = HashMap::new();
    panes.insert(0, vec![guest_pane(GUEST_PANE, true)]);
    panes.insert(1, vec![guest_pane(OTHER_GUEST_PANE, true)]);
    state.update(Event::PaneUpdate(PaneManifest { panes }));
    state.update(Event::TabUpdate(vec![
        TabInfo {
            position: 0,
            active: false,
            ..Default::default()
        },
        TabInfo {
            position: 1,
            active: true,
            ..Default::default()
        },
    ]));
    assert_eq!(state.focused_pane_id(), Some(OTHER_GUEST_PANE));
}

#[test]
fn descending_renders_the_nested_session_s_own_hints() {
    let mut state = state_descended_into_guest();
    state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));

    // The nested session's Pane-mode binding, which this session's own
    // (empty) keybindings could not have produced.
    assert!(
        rendered_bar(&state).contains("close"),
        "expected the nested session's Pane-mode hints, got {:?}",
        rendered_bar(&state)
    );
}

#[test]
fn the_nested_session_s_mode_decides_which_of_its_hints_show() {
    let mut state = state_descended_into_guest();
    state.update(guest_mode_update(GUEST_PANE, InputMode::Normal));
    let normal = rendered_bar(&state);
    state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));
    let pane = rendered_bar(&state);

    assert!(normal.contains("pane"), "got {:?}", normal);
    assert!(pane.contains("close"), "got {:?}", pane);
    assert_ne!(normal, pane);
}

#[test]
fn ascending_returns_the_bar_to_this_session_s_own_hints() {
    let mut state = state_descended_into_guest();
    state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));
    assert!(state.descended_guest_mode_info().is_some());

    // Ascending is this session no longer being the dimmed side. The
    // nested session is still there and still reporting; it is just not
    // where the keys are going any more.
    state.mode_info.session_dimmed = Some(false);
    assert!(state.descended_guest_mode_info().is_none());
    assert!(state.nested_guests.contains_key(&GUEST_PANE));
}

#[test]
fn a_nested_session_with_no_keybindings_yet_falls_back_to_the_indicator() {
    let mut state = state_descended_into_guest();
    state.fetch_keybinds = KeybindsFetcher(unsupported_fetch);
    // Mode reported, keybindings refused by a nested session running a
    // Zellij that cannot report them.
    state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));

    assert!(
        state.descended_guest_mode_info().is_none(),
        "with no bindings there is nothing to draw hints from, so the \
         descended-into indicator should be what renders"
    );
}

#[test]
fn descending_into_a_session_that_never_reported_falls_back_to_the_indicator() {
    let state = state_descended_into_guest();
    assert!(state.descended_guest().is_none());
    assert!(state.descended_guest_mode_info().is_none());
}

#[test]
fn hide_in_base_mode_does_not_blank_the_bar_while_descended() {
    let mut state = state_descended_into_guest();
    state.hide_in_base_mode = true;
    state.update(guest_mode_update(GUEST_PANE, InputMode::Normal));

    // The nested session sits in its base mode, and so does this one. Being
    // descended is the out-of-the-ordinary state the option is there to
    // stay out of the way of, so the hints show anyway.
    assert!(rendered_bar(&state).contains("pane"));
}

#[test]
fn hiding_the_bar_in_a_nested_session_still_wins_over_a_nested_session_of_its_own() {
    let mut state = state_descended_into_guest();
    state.mode_info.session_ancestry = vec!["host".to_string()];
    state.hide_when_nested = true;
    state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));

    // A session in the middle of a chain has no bar at all, so there is
    // nowhere to put the hints of the session below it either.
    assert_eq!(rendered_bar(&state), "");
}

#[test]
fn covering_the_host_s_bar_gives_a_nested_session_its_own_back() {
    let mut state = State {
        mode_info: ModeInfo {
            session_ancestry: vec!["host".to_string()],
            keybinds: guest_keybinds(),
            ..Default::default()
        },
        hide_when_nested: true,
        ..Default::default()
    };

    // The host is drawing a bar of its own just below this one, which is
    // what `hide_when_nested` defers to.
    assert_eq!(rendered_bar(&state), "");

    // The host has expanded this session over its whole display and taken
    // its own bar off the screen, so deferring to it would leave no hints
    // anywhere.
    state.mode_info.host_fullscreen = Some(true);
    assert!(rendered_bar(&state).contains("pane"));
}

#[test]
fn covering_the_host_s_bar_surfaces_the_hints_of_the_session_below() {
    let mut state = state_descended_into_guest();
    state.mode_info.session_ancestry = vec!["host".to_string()];
    state.mode_info.host_fullscreen = Some(true);
    state.hide_when_nested = true;
    state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));

    // This session is the only one left with a bar, and the keys the user
    // is pressing belong to the session below it, so that is what it draws.
    assert!(rendered_bar(&state).contains("close"));
}

#[test]
fn an_ordinary_fullscreen_in_the_host_leaves_a_nested_bar_hidden() {
    let state = State {
        mode_info: ModeInfo {
            session_ancestry: vec!["host".to_string()],
            keybinds: guest_keybinds(),
            // Zellij reports nothing here for the fullscreen that expands a
            // pane over the viewport only: the host's bar is still up.
            host_fullscreen: None,
            ..Default::default()
        },
        hide_when_nested: true,
        ..Default::default()
    };

    assert_eq!(rendered_bar(&state), "");
}

#[test]
fn an_empty_bar_gives_its_row_back_and_a_filled_one_takes_it_again() {
    let mut state = State {
        collapse_when_empty: true,
        ..Default::default()
    };

    assert_eq!(state.next_collapsed(true), Some(true));
    assert_eq!(state.next_collapsed(false), Some(false));
}

#[test]
fn the_row_is_only_spoken_for_when_the_answer_changes() {
    let mut state = State {
        collapse_when_empty: true,
        ..Default::default()
    };

    assert_eq!(
        state.next_collapsed(false),
        None,
        "Zellij already places the pane expanded, so the first full bar has nothing to say"
    );
    assert_eq!(state.next_collapsed(true), Some(true));
    assert_eq!(state.next_collapsed(true), None);
}

#[test]
fn keeping_the_row_reserved_leaves_an_empty_bar_alone() {
    let mut state = State {
        collapse_when_empty: false,
        ..Default::default()
    };

    assert_eq!(state.next_collapsed(true), None);
}

#[test]
fn turning_the_setting_off_hands_the_row_back_without_waiting_for_hints() {
    let mut state = State {
        collapse_when_empty: true,
        ..Default::default()
    };
    assert_eq!(state.next_collapsed(true), Some(true));

    // A reconfigure while the bar is still empty. Leaving the row collapsed
    // until something happened to fill it would look like the setting had
    // been ignored.
    state.collapse_when_empty = false;
    assert_eq!(state.next_collapsed(true), Some(false));
}

#[test]
fn a_nested_session_is_forgotten_when_its_pane_closes() {
    let mut state = state_descended_into_guest();
    state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));

    state.update(Event::PaneUpdate(manifest(vec![guest_pane(
        OTHER_GUEST_PANE,
        true,
    )])));
    assert!(
        state.nested_guests.is_empty(),
        "a pane id Zellij reuses must not inherit the old session's hints"
    );
}

#[test]
fn nothing_about_a_nested_session_disturbs_a_session_hosting_none() {
    let mut state = State {
        mode_info: ModeInfo {
            keybinds: guest_keybinds(),
            mode: InputMode::Pane,
            base_mode: Some(InputMode::Normal),
            ..Default::default()
        },
        ..Default::default()
    };
    let before = rendered_bar(&state);
    state.update(Event::TabUpdate(vec![only_tab()]));
    state.update(Event::PaneUpdate(manifest(vec![guest_pane(
        GUEST_PANE, true,
    )])));

    assert!(state.nested_guests.is_empty());
    assert_eq!(rendered_bar(&state), before);
}

/// Build a plugin configuration from `key = value` pairs.
fn config(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn an_unconfigured_pipe_publishes_on_both_the_current_and_the_old_name() {
    assert_eq!(
        pipe_names_from_config(&config(&[])),
        vec!["zjhints".to_string(), "zjstatus_hints".to_string()]
    );
}

#[test]
fn a_named_pipe_publishes_on_that_name_alone() {
    assert_eq!(
        pipe_names_from_config(&config(&[("pipe_name", "hints")])),
        vec!["hints".to_string()]
    );
}

#[test]
fn naming_the_old_pipe_explicitly_does_not_publish_it_twice() {
    assert_eq!(
        pipe_names_from_config(&config(&[("pipe_name", "zjstatus_hints")])),
        vec!["zjstatus_hints".to_string()]
    );
}

/// A session in `mode` with `keymap` bound there, styled with the minimal
/// formats the `rendered` helper uses so columns are easy to count.
fn clickable_state(mode: InputMode, keymap: Vec<(KeyWithModifier, Vec<Action>)>) -> State {
    State {
        mode_info: ModeInfo {
            mode,
            base_mode: Some(InputMode::Normal),
            keybinds: vec![(mode, keymap)],
            ..Default::default()
        },
        key_format: Some("{key} ".to_string()),
        desc_format: Some("{desc}".to_string()),
        hint_spacer: Some("|".to_string()),
        discover_hints: true,
        overflow_str: DEFAULT_OVERFLOW_STR.to_string(),
        mouse: true,
        mouse_hover: DEFAULT_MOUSE_HOVER,
        mouse_menus: DEFAULT_MOUSE_MENUS,
        ..Default::default()
    }
}

/// A state in `mode` that binds `Alt h` to focus left both there and in
/// the base mode, normal, as a `shared` block does.
fn sharing_focus_left(mode: InputMode) -> State {
    let alt_h = KeyWithModifier::new(BareKey::Char('h')).with_alt_modifier();
    let binding = (
        alt_h,
        vec![Action::MoveFocus {
            direction: Direction::Left,
        }],
    );
    let mut state = clickable_state(mode, vec![binding.clone()]);
    state
        .mode_info
        .keybinds
        .push((InputMode::Normal, vec![binding]));
    state.hide_shared_hints = true;
    state
}

#[test]
fn locked_mode_shows_keys_the_base_mode_shares() {
    assert!(rendered_bar(&sharing_focus_left(InputMode::Locked)).contains("Alt h"));
}

#[test]
fn other_modes_still_hide_keys_the_base_mode_shares() {
    assert!(!rendered_bar(&sharing_focus_left(InputMode::Scroll)).contains("Alt h"));
    let mut shown = sharing_focus_left(InputMode::Scroll);
    shown.hide_shared_hints = false;
    assert!(rendered_bar(&shown).contains("Alt h"));
}

/// What a left-click on `column` of the bar this state draws would do.
fn left_click(state: &State, column: usize) -> MouseOutcome {
    let (_, click_map) = state.draw_bar();
    mouse::click_outcome(&click_map, &Mouse::LeftClick(0, column))
}

/// The column `needle` starts at on the drawn bar.
fn column_of(state: &State, needle: &str) -> usize {
    let bar = rendered_bar(state);
    let byte = bar
        .find(needle)
        .unwrap_or_else(|| panic!("{:?} not on the bar {:?}", needle, bar));
    calculate_visible_length(&bar[..byte])
}

fn menu_labels(outcome: &MouseOutcome) -> Vec<String> {
    match outcome {
        MouseOutcome::Menu(entries) => entries.iter().map(|e| e.label.clone()).collect(),
        other => panic!("expected a menu, got {:?}", other),
    }
}

fn choose(outcome: MouseOutcome, index: usize) -> MouseOutcome {
    let MouseOutcome::Menu(entries) = outcome else {
        panic!("expected a menu, got {:?}", outcome);
    };
    mouse::menu_answer_outcome(
        Some(entries),
        &PromptResult::Answered(PromptValue::Choice(index.to_string())),
    )
}

#[test]
fn clicking_a_hint_runs_what_its_key_runs() {
    let state = clickable_state(InputMode::Pane, four_hints());
    assert_eq!(rendered_bar(&state), " a close|b fullscreen|c frames|d pin");
    let run = |action| MouseOutcome::Run(vec![action]);
    // The leading space and the spacers are not part of any hint.
    assert_eq!(left_click(&state, 0), MouseOutcome::Nothing);
    assert_eq!(left_click(&state, 1), run(Action::CloseFocus));
    assert_eq!(left_click(&state, 7), run(Action::CloseFocus));
    assert_eq!(left_click(&state, 8), MouseOutcome::Nothing);
    assert_eq!(left_click(&state, 9), run(Action::ToggleFocusFullscreen));
    let pin = column_of(&state, "d pin");
    assert_eq!(left_click(&state, pin + 4), run(Action::TogglePanePinned));
    assert_eq!(left_click(&state, pin + 5), MouseOutcome::Nothing);
}

#[test]
fn a_click_runs_the_whole_binding_in_order() {
    let binding = vec![
        message_plugin(),
        Action::SwitchToMode {
            input_mode: InputMode::Normal,
        },
    ];
    let state = clickable_state(InputMode::Locked, vec![(ctrl('g'), binding.clone())]);
    let column = column_of(&state, "Ctrl g");
    assert_eq!(left_click(&state, column), MouseOutcome::Run(binding));
}

#[test]
fn click_columns_follow_the_mode_prefix() {
    let mut state = clickable_state(InputMode::Pane, four_hints());
    state.show_mode = true;
    state.mode_format = Some("<{mode}> ".to_string());
    let close = column_of(&state, "a close");
    assert!(close > 1, "the prefix should push the hints right");
    assert_eq!(left_click(&state, close - 1), MouseOutcome::Nothing);
    assert_eq!(
        left_click(&state, close),
        MouseOutcome::Run(vec![Action::CloseFocus])
    );
}

#[test]
fn the_spacer_separates_the_mode_prefix_from_the_first_hint() {
    let mut state = clickable_state(InputMode::Pane, four_hints());
    state.show_mode = true;
    state.mode_format = Some("M".to_string());
    assert_eq!(
        rendered_bar(&state),
        "M|a close|b fullscreen|c frames|d pin"
    );
    assert_eq!(left_click(&state, 1), MouseOutcome::Nothing);
    assert_eq!(
        left_click(&state, 2),
        MouseOutcome::Run(vec![Action::CloseFocus])
    );
}

#[test]
fn fitting_pays_for_the_spacer_after_the_mode_prefix() {
    let mut state = clickable_state(InputMode::Pane, four_hints());
    state.show_mode = true;
    state.mode_format = Some("M".to_string());
    state.drop_indicator = Some("…".to_string());
    // "M|a close|b fullscreen|…" would be 24 columns, one too many.
    state.max_length = 23;
    assert_eq!(rendered_bar(&state), "M|a close|…");
}

#[test]
fn without_the_mode_prefix_the_first_hint_has_no_spacer() {
    let state = clickable_state(InputMode::Pane, four_hints());
    assert_eq!(rendered_bar(&state), " a close|b fullscreen|c frames|d pin");
}

#[test]
fn click_columns_count_an_emoji_as_the_two_columns_zellij_gives_it() {
    let mut state = clickable_state(InputMode::Pane, four_hints());
    // A hand and a skin tone, two characters drawn as one emoji.
    state
        .bar
        .configuration
        .insert("label_close_pane".to_string(), "👍🏽".to_string());
    assert_eq!(rendered_bar(&state), " a 👍🏽|b fullscreen|c frames|d pin");
    // " a " is columns 0 to 2, the emoji 3 and 4, the spacer 5.
    let run = |action| MouseOutcome::Run(vec![action]);
    assert_eq!(left_click(&state, 4), run(Action::CloseFocus));
    assert_eq!(left_click(&state, 5), MouseOutcome::Nothing);
    assert_eq!(left_click(&state, 6), run(Action::ToggleFocusFullscreen));
    assert_eq!(column_of(&state, "b fullscreen"), 6);
}

#[test]
fn click_columns_count_an_ambiguous_character_as_one() {
    let mut state = clickable_state(InputMode::Pane, four_hints());
    // `│` is East Asian Ambiguous, which Zellij lays out one column wide.
    state.hint_spacer = Some("│".to_string());
    // " a close" is columns 0 to 7, the spacer 8.
    assert_eq!(left_click(&state, 8), MouseOutcome::Nothing);
    assert_eq!(
        left_click(&state, 9),
        MouseOutcome::Run(vec![Action::ToggleFocusFullscreen])
    );
}

#[test]
fn ambiguous_width_has_no_effect() {
    let draw = |ambiguous_width: Option<&str>| {
        let mut configuration = vec![
            ("hint_spacer", "│"),
            ("label_close_pane", "\u{f121} close"),
            ("mouse", "true"),
            ("max_length", "24"),
        ];
        configuration.extend(ambiguous_width.map(|width| ("ambiguous_width", width)));
        let mut state = bar_with(&configuration);
        state.mode_info = ModeInfo {
            mode: InputMode::Pane,
            base_mode: Some(InputMode::Normal),
            keybinds: vec![(InputMode::Pane, four_hints())],
            ..Default::default()
        };
        state.draw_bar()
    };
    let (line, click_map) = draw(None);
    assert!(calculate_visible_length(&line) <= 24, "{line:?}");
    assert_eq!(draw(Some("2")), (line.clone(), click_map.clone()));
    assert_eq!(draw(Some("1")), (line, click_map));
}

#[test]
fn keys_that_all_do_the_same_thing_run_it_directly() {
    let state = clickable_state(
        InputMode::Pane,
        vec![
            (key(BareKey::Char('x')), vec![Action::CloseFocus]),
            (key(BareKey::Char('q')), vec![Action::CloseFocus]),
        ],
    );
    assert_eq!(
        left_click(&state, column_of(&state, "close")),
        MouseOutcome::Run(vec![Action::CloseFocus])
    );
}

fn focus_keymap() -> Vec<(KeyWithModifier, Vec<Action>)> {
    let focus = |direction| vec![Action::MoveFocus { direction }];
    vec![
        (key(BareKey::Char('h')), focus(Direction::Left)),
        (key(BareKey::Left), focus(Direction::Left)),
        (key(BareKey::Char('l')), focus(Direction::Right)),
    ]
}

#[test]
fn keys_that_do_different_things_ask_which_to_run() {
    let state = clickable_state(InputMode::Pane, focus_keymap());
    let outcome = left_click(&state, column_of(&state, "focus"));
    assert_eq!(
        menu_labels(&outcome),
        vec!["h←  move focus left", "l  move focus right"]
    );
    assert_eq!(
        choose(outcome, 1),
        MouseOutcome::Run(vec![Action::MoveFocus {
            direction: Direction::Right
        }])
    );
}

#[test]
fn a_click_only_offers_the_keys_the_hint_shows() {
    let mut state = clickable_state(InputMode::Pane, focus_keymap());
    state.direction_keys = DirectionKeys::Arrows;
    assert_eq!(
        left_click(&state, column_of(&state, "focus")),
        MouseOutcome::Run(vec![Action::MoveFocus {
            direction: Direction::Left
        }])
    );
}

/// Bar 7, drawn once with a drop marker that opens a menu, and the
/// marker's column.
fn previewing_bar() -> (State, usize) {
    let mut state = clickable_state(InputMode::Pane, four_hints());
    state.drop_indicator = Some("…".to_string());
    state.max_length = 23;
    state.bar.plugin_id = Some(7);
    state.bar.own_url = Some("zjhints".to_string());
    state.render(1, 23);
    let marker = column_of(&state, "…");
    (state, marker)
}

/// Rest the pointer on `column` until its preview opens.
fn rest_on(state: &mut State, column: usize) {
    state.update(Event::Mouse(Mouse::Hover(0, column)));
    state.update(Event::Timer(mouse::HOVER_DELAY_SECONDS));
}

fn message_from(source: PipeSource, name: &str, payload: &str) -> PipeMessage {
    PipeMessage::new(source, name, &Some(payload.to_string()), &None, false)
}

#[test]
fn resting_on_a_picker_opens_its_preview_after_the_delay() {
    let (mut state, marker) = previewing_bar();
    state.host.clear();
    state.update(Event::Mouse(Mouse::Hover(0, marker)));
    assert!(!state.preview_memory.is_idle());
    assert_eq!(
        state.host.calls(),
        vec![Call::SetTimeout(mouse::HOVER_DELAY_SECONDS)]
    );
    state.host.clear();
    state.update(Event::Timer(mouse::HOVER_DELAY_SECONDS));
    // The popup hangs from the marker, below the bar's one row, and lists
    // the hints the marker stands for.
    assert_eq!(
        state.host.calls(),
        vec![Call::OpenPreviewPopup {
            url: "zjhints".to_string(),
            tag: "7-1".to_string(),
            entries: vec!["c  frames".to_string(), "d  pin".to_string()],
            line: 1,
            column: marker,
        }]
    );
    // Preview 1 of bar 7 says where it is, and is the one closed.
    let opened = message_from(PipeSource::Plugin(30), popup::NAMES.opened, "7-1");
    assert!(!state.pipe(opened));
    state.host.clear();
    state.update(Event::Mouse(Mouse::LeftClick(0, 0)));
    assert_eq!(
        state.host.calls(),
        vec![Call::ClosePreviewPopup {
            message: popup::NAMES.close,
            popup: 30,
        }]
    );
}

#[test]
fn with_mouse_hover_off_resting_on_a_picker_opens_no_preview() {
    let (mut state, marker) = previewing_bar();
    state.mouse_hover = false;
    rest_on(&mut state, marker);
    assert!(state.preview_memory.is_idle());
}

#[test]
fn turning_mouse_hover_off_closes_a_preview_on_the_next_hover() {
    let (mut state, marker) = previewing_bar();
    rest_on(&mut state, marker);
    let opened = message_from(PipeSource::Plugin(30), popup::NAMES.opened, "7-1");
    state.pipe(opened);
    state.mouse_hover = false;
    state.update(Event::Mouse(Mouse::Hover(0, marker)));
    assert!(state.preview_memory.is_idle());
}

#[test]
fn with_mouse_menus_off_a_click_opens_no_menu_but_still_runs_a_hint() {
    let (mut state, marker) = previewing_bar();
    assert!(matches!(
        state.bar_click_outcome(&Mouse::LeftClick(0, marker)),
        MouseOutcome::Menu(_)
    ));
    state.mouse_menus = false;
    assert_eq!(
        state.bar_click_outcome(&Mouse::LeftClick(0, marker)),
        MouseOutcome::Nothing
    );
    // The first hint, after the leading space.
    let hint = 1;
    assert!(matches!(
        state.bar_click_outcome(&Mouse::LeftClick(0, hint)),
        MouseOutcome::Run(_)
    ));
    // The right-click menu is Zellij's, and stays.
    assert!(matches!(
        state.bar_click_outcome(&Mouse::RightClick(0, hint)),
        MouseOutcome::ContextMenu { .. }
    ));
}

#[test]
fn mouse_hover_and_mouse_menus_are_on_with_mouse_and_off_without_it() {
    let on = bar_with(&[("mouse", "true")]);
    assert!(on.mouse_hover && on.mouse_menus);
    let off = bar_with(&[
        ("mouse", "true"),
        ("mouse_hover", "false"),
        ("mouse_menus", "False"),
    ]);
    assert!(!off.mouse_hover && !off.mouse_menus);
    let no_mouse = bar_with(&[("mouse_hover", "true"), ("mouse_menus", "true")]);
    assert!(!no_mouse.mouse_hover && !no_mouse.mouse_menus);
}

#[test]
fn a_bar_that_does_not_know_its_url_yet_opens_no_preview() {
    let (mut state, marker) = previewing_bar();
    state.bar.own_url = None;
    state.update(Event::Mouse(Mouse::Hover(0, marker)));
    assert!(state.preview_memory.is_idle());
}

#[test]
fn a_click_closes_the_preview() {
    let (mut state, marker) = previewing_bar();
    rest_on(&mut state, marker);
    // The leading space, which does nothing else.
    state.update(Event::Mouse(Mouse::LeftClick(0, 0)));
    assert!(state.preview_memory.is_idle());
}

#[test]
fn drawing_other_hints_closes_the_preview_and_the_same_ones_keep_it() {
    let (mut state, marker) = previewing_bar();
    rest_on(&mut state, marker);
    state.render(1, 23);
    assert!(!state.preview_memory.is_idle());
    // Every hint fits, so the marker is gone.
    state.max_length = 40;
    state.render(1, 40);
    assert!(state.preview_memory.is_idle());
}

#[test]
fn only_the_bar_s_own_popups_are_heard() {
    let (mut state, marker) = previewing_bar();
    rest_on(&mut state, marker);
    let closed = popup::NAMES.closed;
    for message in [
        message_from(PipeSource::Plugin(30), closed, "8-1"),
        message_from(PipeSource::Plugin(30), closed, "stray"),
        message_from(PipeSource::Cli("pipe".to_string()), closed, "7-1"),
    ] {
        assert!(!state.pipe(message));
    }
    assert!(!state.preview_memory.is_idle());
    state.pipe(message_from(PipeSource::Plugin(30), closed, "7-1"));
    assert!(state.preview_memory.is_idle());
}

#[test]
fn a_popup_instance_ignores_what_the_bar_handles() {
    let configuration = zjkit::popup::configuration(&popup::NAMES, "7-1", &["a".to_string()]);
    let mut state = State {
        role: Role::Popup(PreviewPopup::from_configuration(&popup::NAMES, &configuration).unwrap()),
        ..Default::default()
    };
    let pane_mode = ModeInfo {
        mode: InputMode::Pane,
        ..Default::default()
    };
    assert!(!state.update(Event::ModeUpdate(pane_mode)));
    assert_eq!(state.mode_info.mode, ModeInfo::default().mode);
    let preview = preview_args("zjhints", &[("show_mode", None, Some("true"))]);
    let message = PipeMessage::new(
        PipeSource::Plugin(5),
        settings::SCHEMA.live_message,
        &None,
        &Some(preview),
        false,
    );
    assert!(!state.pipe(message));
    assert!(!state.show_mode);
}

#[test]
fn the_drop_marker_opens_a_menu_of_the_dropped_hints() {
    let mut state = clickable_state(InputMode::Pane, four_hints());
    state.drop_indicator = Some("…".to_string());
    state.max_length = 23;
    assert_eq!(rendered_bar(&state), " a close|b fullscreen|…");
    let outcome = left_click(&state, column_of(&state, "…"));
    assert_eq!(menu_labels(&outcome), vec!["c  frames", "d  pin"]);
    assert_eq!(
        choose(outcome, 0),
        MouseOutcome::Run(vec![Action::TogglePaneFrames])
    );
}

#[test]
fn a_dropped_hint_with_several_actions_opens_its_own_picker() {
    let mut keymap = vec![(key(BareKey::Char('a')), vec![Action::CloseFocus])];
    keymap.extend(focus_keymap());
    let mut state = clickable_state(InputMode::Pane, keymap);
    state.drop_indicator = Some("…".to_string());
    state.max_length = 10;
    assert_eq!(rendered_bar(&state), " a close|…");
    let dropped = left_click(&state, column_of(&state, "…"));
    let picker = choose(dropped, 0);
    assert_eq!(
        menu_labels(&picker),
        vec!["h←  move focus left", "l  move focus right"]
    );
}

#[test]
fn overflow_str_opens_a_menu_of_everything_cut_off() {
    let mut state = clickable_state(InputMode::Pane, four_hints());
    state.drop_indicator = Some("…".to_string());
    state.max_length = 5;
    assert_eq!(rendered_bar(&state), " a...");
    // What is left of the cut hint still runs it.
    assert_eq!(
        left_click(&state, 1),
        MouseOutcome::Run(vec![Action::CloseFocus])
    );
    // The hint the cut ran through, then the dropped ones.
    let outcome = left_click(&state, 3);
    assert_eq!(
        menu_labels(&outcome),
        vec!["a  close", "b  fullscreen", "c  frames", "d  pin"]
    );
    assert_eq!(left_click(&state, 5), MouseOutcome::Nothing);
}

#[test]
fn overflow_str_is_a_format_string() {
    let mut state = clickable_state(InputMode::Pane, four_hints());
    state.overflow_str = "#[fg=#ff0000]>".to_string();
    state.max_length = 5;
    let (line, _) = state.draw_bar();
    // Reset first, so the bold of the cut key does not reach the marker.
    assert!(line.contains("\u{1b}[0m\u{1b}[38;2;255;0;0m>"), "{line:?}");
    assert_eq!(calculate_visible_length(&line), 5);
    // Plain text is drawn as it is.
    state.overflow_str = "...".to_string();
    assert_eq!(rendered_bar(&state), " a...");
}

#[test]
fn a_menu_entry_reads_like_the_hint_including_replaced_keys() {
    let mut state = clickable_state(InputMode::Pane, four_hints());
    state.bar.configuration = config(&[("keys_frames", "1-9")]);
    state.drop_indicator = Some("…".to_string());
    state.max_length = 23;
    let outcome = left_click(&state, column_of(&state, "…"));
    assert_eq!(menu_labels(&outcome), vec!["1-9  frames", "d  pin"]);
}

#[test]
fn a_nested_session_s_hints_are_not_clickable_without_mouse_nested() {
    let mut state = state_descended_into_guest();
    state.mouse = true;
    state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));
    let bar = rendered_bar(&state);
    assert!(bar.contains("close"), "expected guest hints, got {:?}", bar);
    let (_, click_map) = state.draw_bar();
    assert_eq!(click_map, ClickMap::default());
}

#[test]
fn the_descended_indicator_is_not_clickable_without_mouse_nested() {
    let mut state = state_with_ancestry(vec![]);
    state.mouse = true;
    state.show_mode = true;
    state.mode_format = Some("MODE ".to_string());
    state.mode_info.session_dimmed = Some(true);
    state.mode_info.nested_ascend_keys = vec![ctrl('o'), key(BareKey::Char(']'))];
    let (line, click_map) = state.draw_bar();
    assert!(!line.is_empty());
    assert_eq!(click_map, ClickMap::default());
}

#[test]
fn with_mouse_nested_a_nested_session_s_hint_presses_its_key() {
    let mut state = state_descended_into_guest();
    state.mouse = true;
    state.mouse_nested = true;
    state.update(guest_mode_update(GUEST_PANE, InputMode::Pane));
    let outcome = left_click(&state, column_of(&state, "close"));
    // The guest's key, typed into its pane, rather than the guest's
    // binding run in this session.
    assert_eq!(
        outcome,
        MouseOutcome::Run(keypress::press_keys(&[key(BareKey::Char('x'))]))
    );
}

#[test]
fn with_mouse_nested_the_descended_indicator_presses_the_ascend_keys() {
    let mut state = state_with_ancestry(vec![]);
    state.mouse = true;
    state.mouse_nested = true;
    state.mode_info.session_dimmed = Some(true);
    let ascend_keys = vec![ctrl('o'), key(BareKey::Char(']'))];
    state.mode_info.nested_ascend_keys = ascend_keys.clone();
    assert_eq!(
        left_click(&state, 0),
        MouseOutcome::Run(keypress::press_keys(&ascend_keys))
    );
    // Past the end of the indicator does nothing.
    let (line, _) = state.draw_bar();
    let width = calculate_visible_length(&line);
    assert_eq!(left_click(&state, width), MouseOutcome::Nothing);
}

#[test]
fn with_mouse_nested_the_mode_prefix_before_the_indicator_opens_settings() {
    let mut state = state_with_ancestry(vec![]);
    state.mouse = true;
    state.mouse_nested = true;
    state.show_mode = true;
    state.mode_format = Some("MODE ".to_string());
    state.mode_info.session_dimmed = Some(true);
    let ascend_keys = vec![ctrl('o'), key(BareKey::Char(']'))];
    state.mode_info.nested_ascend_keys = ascend_keys.clone();
    assert_eq!(left_click(&state, 0), MouseOutcome::OpenSettings);
    assert_eq!(
        left_click(&state, "MODE ".len()),
        MouseOutcome::Run(keypress::press_keys(&ascend_keys))
    );
}

#[test]
fn the_spacer_separates_the_mode_prefix_from_the_descended_indicator() {
    let mut state = state_with_ancestry(vec![]);
    state.mouse = true;
    state.mouse_nested = true;
    state.show_mode = true;
    state.mode_format = Some("MODE".to_string());
    state.hint_spacer = Some("|".to_string());
    state.mode_info.session_dimmed = Some(true);
    let ascend_keys = vec![ctrl('o'), key(BareKey::Char(']'))];
    state.mode_info.nested_ascend_keys = ascend_keys.clone();
    let (line, _) = state.draw_bar();
    assert!(
        strip_ansi(&line).starts_with("MODE|"),
        "{:?}",
        strip_ansi(&line)
    );
    assert_eq!(left_click(&state, "MODE".len()), MouseOutcome::Nothing);
    assert_eq!(
        left_click(&state, "MODE|".len()),
        MouseOutcome::Run(keypress::press_keys(&ascend_keys))
    );
}

#[test]
fn mouse_support_gives_dropped_hints_a_marker_by_default() {
    assert_eq!(drop_indicator_from_config(&config(&[]), false), None);
    assert_eq!(
        drop_indicator_from_config(&config(&[]), true),
        Some(DEFAULT_MOUSE_DROP_INDICATOR.to_string())
    );
    assert_eq!(
        drop_indicator_from_config(&config(&[("drop_indicator", "")]), true),
        Some(DEFAULT_MOUSE_DROP_INDICATOR.to_string())
    );
    assert_eq!(
        drop_indicator_from_config(&config(&[("drop_indicator", "+")]), true),
        Some("+".to_string())
    );
}

fn preview_args(
    alias: &str,
    changes: &[(&str, Option<&str>, Option<&str>)],
) -> BTreeMap<String, String> {
    let changes: Vec<live::Change> = changes
        .iter()
        .map(|(key, was, now)| live::Change {
            key: key.to_string(),
            was: was.map(str::to_string),
            now: now.map(str::to_string),
        })
        .collect();
    live::encode(alias, &changes)
}

/// Send `bar` a preview from the settings screen, as `pipe` gets it.
/// Returns whether the bar should redraw. The screen is plugin 5, whose pane
/// runs from the bar's own URL, as a bar only takes a preview from its own
/// plugin. A bar that has had no `PaneUpdate` yet is first given the alias
/// `zjhints` for its URL, as its first one would give it.
fn preview(bar: &mut State, args: BTreeMap<String, String>) -> bool {
    let url = bar
        .bar
        .own_url
        .get_or_insert_with(|| "zjhints".to_string())
        .clone();
    bar.bar.plugin_pane_urls.insert(5, url);
    bar.pipe(PipeMessage::new(
        PipeSource::Plugin(5),
        settings::SCHEMA.live_message,
        &None,
        &Some(args),
        false,
    ))
}

fn bar_with(configuration: &[(&str, &str)]) -> State {
    let mut state = State::default();
    state.apply_configuration(
        configuration
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
    );
    state
}

#[test]
fn the_zellij_color_theme_follows_a_change_of_zellij_theme() {
    let mut state = bar_with(&[("format_theme", "flat")]);
    // No palette has arrived, so there is nothing to take aliases from.
    assert!(!state.bar.configuration.contains_key("color_accent"));

    let mut mode_info = ModeInfo::default();
    mode_info.style.colors.ribbon_selected.background = PaletteColor::Rgb((1, 2, 3));
    state.update(Event::ModeUpdate(mode_info.clone()));
    assert_eq!(state.bar.configuration["color_accent"], "#010203");

    mode_info.style.colors.ribbon_selected.background = PaletteColor::Rgb((4, 5, 6));
    assert!(state.update(Event::ModeUpdate(mode_info)));
    assert_eq!(state.bar.configuration["color_accent"], "#040506");
    // The user's own options are untouched by it.
    assert!(!state.bar.user_configuration.contains_key("color_accent"));
}

#[test]
fn a_preview_changes_the_bar_through_the_same_parsing_as_load() {
    let mut state = bar_with(&[("max_length", "10")]);
    state.bar.own_url = Some("zjhints".to_string());
    let args = preview_args(
        "zjhints",
        &[
            ("max_length", Some("10"), Some("30")),
            ("show_mode", None, Some("true")),
        ],
    );
    assert!(preview(&mut state, args));
    assert_eq!(state.max_length, 30);
    assert!(state.show_mode);
    assert_eq!(
        state.bar.configuration.get("show_mode").map(String::as_str),
        Some("true")
    );
}

#[test]
fn a_preview_leaves_a_key_the_layout_set_alone() {
    // The layout gave this bar show_mode true; the alias does not set it.
    let mut state = bar_with(&[("show_mode", "true")]);
    let args = preview_args("zjhints", &[("show_mode", None, Some("false"))]);
    assert!(!preview(&mut state, args));
    assert!(state.show_mode);
}

#[test]
fn a_preview_for_another_alias_is_ignored() {
    let mut state = bar_with(&[]);
    state.bar.own_url = Some("zjhints-alt".to_string());
    let args = preview_args("zjhints", &[("show_mode", None, Some("true"))]);
    assert!(!preview(&mut state, args));
    assert!(!state.show_mode);
}

/// The styled bar `configuration` draws for a Normal mode that binds `q`
/// to quit, with the escape codes left in.
fn styled_bar(configuration: &[(&str, &str)]) -> String {
    let mut state = bar_with(configuration);
    state.mode_info.keybinds = quit_table('q');
    state
        .render_hint_line(&state.mode_info, 0.0, ClickBehavior::RunActions)
        .0
}

#[test]
fn a_themed_bar_is_drawn_in_the_themes_colors() {
    let zellij = styled_bar(&[]);
    let themed = styled_bar(&[
        ("color_theme", "catppuccin-mocha"),
        ("format_theme", "flat"),
    ]);
    assert_ne!(themed, zellij);
    // The key block: the theme's bg on its accent (#cba6f7, mauve).
    assert!(themed.contains("48;2;203;166;247"), "{themed:?}");
    assert!(themed.contains("38;2;30;30;46"), "{themed:?}");
    // The description block: the theme's text on its surface (#313244).
    assert!(themed.contains("38;2;205;214;244"), "{themed:?}");
    assert!(themed.contains("48;2;49;50;68"), "{themed:?}");
    assert!(!zellij.contains("48;2;203;166;247"));
}

#[test]
fn the_zellij_theme_draws_the_bar_as_if_no_theme_were_set() {
    let unset = styled_bar(&[]);
    for (option, value) in [
        ("color_theme", "zellij"),
        ("color_theme", "custom"),
        ("color_theme", "no-such-theme"),
        ("format_theme", "zellij"),
        ("format_theme", "no-such-theme"),
        ("alias_theme", "zellij"),
    ] {
        assert_eq!(styled_bar(&[(option, value)]), unset, "{option} {value}");
    }
}

#[test]
fn a_users_own_options_win_over_a_themes_on_the_bar() {
    let bar = styled_bar(&[
        ("color_theme", "catppuccin-mocha"),
        ("format_theme", "flat"),
        ("color_accent", "#010203"),
        ("desc_format", "<{desc}>"),
    ]);
    assert!(bar.contains("48;2;1;2;3"), "{bar:?}");
    assert!(!bar.contains("48;2;203;166;247"));
    assert!(strip_ansi(&bar).contains("<quit>"));
    // The key block still comes from the theme.
    assert!(bar.contains("38;2;30;30;46"));
}

#[test]
fn every_mode_has_a_theme_color() {
    for mode in [
        InputMode::Normal,
        InputMode::Locked,
        InputMode::Resize,
        InputMode::Pane,
        InputMode::Tab,
        InputMode::Scroll,
        InputMode::EnterSearch,
        InputMode::Search,
        InputMode::RenameTab,
        InputMode::RenamePane,
        InputMode::RenameSession,
        InputMode::Session,
        InputMode::Move,
        InputMode::Prompt,
        InputMode::Tmux,
    ] {
        let key = format!("mode_format_{}", mode_config_suffix(mode));
        assert!(
            bar_with(&[("format_theme", "flat")])
                .bar
                .configuration
                .contains_key(&key),
            "no theme color for {key}"
        );
    }
}

#[test]
fn an_unknown_theme_leaves_the_configuration_as_written() {
    let state = bar_with(&[("color_theme", "no-such-theme"), ("max_length", "10")]);
    assert_eq!(state.bar.configuration, state.bar.user_configuration);
    assert_eq!(state.bar.configuration.len(), 2);
}

#[test]
fn a_preview_that_changes_the_theme_swaps_its_colors() {
    let mut state = bar_with(&[
        ("color_theme", "catppuccin-mocha"),
        ("color_blue", "#010203"),
    ]);
    state.bar.own_url = Some("zjhints".to_string());
    assert_eq!(state.bar.configuration["color_accent"], "#cba6f7");

    let args = preview_args(
        "zjhints",
        &[("color_theme", Some("catppuccin-mocha"), Some("nord"))],
    );
    assert!(preview(&mut state, args));
    // The old theme's values are not left behind as if the user set them.
    assert_eq!(state.bar.configuration["color_accent"], "#88c0d0");
    assert_eq!(state.bar.configuration["color_bg"], "#2e3440");
    // The user's own color survives the switch.
    assert_eq!(state.bar.configuration["color_blue"], "#010203");

    let args = preview_args("zjhints", &[("color_theme", Some("nord"), Some("custom"))]);
    assert!(preview(&mut state, args));
    assert!(!state.bar.configuration.contains_key("color_accent"));
    assert_eq!(state.bar.configuration["color_blue"], "#010203");
}

#[test]
fn a_preview_of_another_option_keeps_the_theme() {
    let mut state = bar_with(&[("color_theme", "dracula"), ("format_theme", "flat")]);
    state.bar.own_url = Some("zjhints".to_string());
    let args = preview_args("zjhints", &[("max_length", None, Some("30"))]);
    assert!(preview(&mut state, args));
    assert_eq!(state.max_length, 30);
    assert_eq!(state.bar.configuration["color_accent"], "#bd93f9");
    assert!(state.key_format.is_some());
}

/// A bar with something to draw: the mode prefix on its own.
fn drawing_bar() -> State {
    let mut state = state_with_ancestry(vec![]);
    state.show_mode = true;
    state.mode_format = Some("MODE".to_string());
    assert!(!state.draw_bar().0.is_empty());
    state
}

#[test]
fn visibility_messages_keep_their_names() {
    assert_eq!(
        VISIBILITY,
        visibility::Names {
            hide: "zjhints::hide",
            show: "zjhints::show",
            toggle: "zjhints::toggle",
        }
    );
}

#[test]
fn a_hidden_bar_draws_nothing_and_has_nothing_to_click() {
    let mut state = drawing_bar();
    state.mouse = true;
    state.bar.change_visibility(Request::Hide, None);
    assert_eq!(state.draw_bar(), (String::new(), ClickMap::default()));
}

#[test]
fn a_hidden_bar_gives_its_row_back_even_when_keeping_it_reserved() {
    let mut state = drawing_bar();
    state.collapse_when_empty = false;
    state.bar.change_visibility(Request::Hide, None);
    assert_eq!(state.next_collapsed(true), Some(true));
    state.bar.change_visibility(Request::Show, None);
    assert_eq!(state.next_collapsed(false), Some(false));
    // Shown again, an empty bar keeps its row as the option says.
    assert_eq!(state.next_collapsed(true), None);
}

#[test]
fn the_hidden_option_sets_the_state_on_load_and_when_it_changes() {
    let mut state = bar_with(&[("hidden", "true")]);
    assert!(state.bar.visibility.is_hidden());
    // A message overrides it, and an unrelated preview keeps that.
    state.bar.change_visibility(Request::Show, None);
    preview(
        &mut state,
        preview_args("zjhints", &[("max_length", None, Some("40"))]),
    );
    assert!(!state.bar.visibility.is_hidden());
    // Changing the option itself sets the state again.
    preview(
        &mut state,
        preview_args("zjhints", &[("hidden", Some("true"), Some("false"))]),
    );
    assert!(!state.bar.visibility.is_hidden());
    preview(
        &mut state,
        preview_args("zjhints", &[("hidden", Some("false"), Some("true"))]),
    );
    assert!(state.bar.visibility.is_hidden());
    assert!(!bar_with(&[]).bar.visibility.is_hidden());
}

#[test]
fn a_preview_can_turn_mouse_off() {
    let mut state = bar_with(&[("mouse", "true")]);
    assert!(preview(
        &mut state,
        preview_args("zjhints", &[("mouse", Some("true"), None)])
    ));
    assert!(!state.mouse);
}

#[test]
fn clicking_the_mode_prefix_opens_settings() {
    let mut state = clickable_state(InputMode::Pane, four_hints());
    state.show_mode = true;
    state.mode_format = Some("MODE ".to_string());
    assert_eq!(left_click(&state, 0), MouseOutcome::OpenSettings);
    assert_eq!(left_click(&state, 4), MouseOutcome::OpenSettings);
    assert_eq!(
        left_click(&state, column_of(&state, "a close")),
        MouseOutcome::Run(vec![Action::CloseFocus])
    );

    // The screen edits the alias the clicked bar was started from.
    state.bar.own_url = Some("zjhints-alt".to_string());
    state.click_map = state.draw_bar().1;
    state.update(Event::Mouse(Mouse::LeftClick(0, 0)));
    assert_eq!(
        state.host.calls(),
        vec![Call::OpenSettings {
            message: OPEN_SETTINGS_MESSAGE.to_string(),
            pane_title: "zjhints settings".to_string(),
            own_url: Some("zjhints-alt".to_string()),
        }]
    );
}

#[test]
fn a_bar_without_mouse_can_add_its_settings_entry() {
    assert!(bar_permissions(false).contains(&PermissionType::ChangeApplicationState));
}

#[test]
fn asking_for_mouse_keeps_the_permissions_the_bar_always_needs() {
    let permissions = bar_permissions(true);
    for permission in BAR_PERMISSIONS.iter().chain(MOUSE_PERMISSIONS.iter()) {
        assert!(permissions.contains(permission), "{permission:?}");
    }
    assert_eq!(bar_permissions(false), BAR_PERMISSIONS.to_vec());
}

/// A clickable bar in Pane mode binding `keymap`, with its click map drawn
/// as `render` would leave it.
fn clicked_bar(keymap: Vec<(KeyWithModifier, Vec<Action>)>) -> State {
    let mut state = clickable_state(InputMode::Pane, keymap);
    state.click_map = state.draw_bar().1;
    state
}

#[test]
fn loading_a_bar_asks_for_its_permissions_and_events() {
    let mut state = State::default();
    state.load(BTreeMap::new());
    assert_eq!(state.bar.plugin_id, Some(Recorder::PLUGIN_ID));
    assert_eq!(state.bar.client_id, Some(Recorder::CLIENT_ID));
    // The pane stays selectable until the answer, so the prompt in it can be
    // answered.
    assert_eq!(
        state.host.calls(),
        vec![
            Call::PluginIds,
            Call::RequestPermission(bar_permissions(false)),
            Call::Subscribe(BAR_EVENTS.to_vec()),
        ]
    );
}

#[test]
fn loading_a_bar_with_mouse_asks_for_what_mouse_needs_too() {
    let mut state = State::default();
    state.load(BTreeMap::from([("mouse".to_string(), "true".to_string())]));
    assert_eq!(
        state.host.calls(),
        vec![
            Call::PluginIds,
            Call::RequestPermission(bar_permissions(true)),
            Call::Subscribe(BAR_EVENTS.to_vec()),
            Call::Subscribe(MOUSE_EVENTS.to_vec()),
        ]
    );
}

#[test]
fn a_permission_answer_makes_the_bar_unselectable_and_a_grant_adds_its_settings_entry() {
    let mut granted = State::default();
    assert!(granted.update(Event::PermissionRequestResult(PermissionStatus::Granted)));
    assert_eq!(
        granted.host.calls(),
        vec![
            Call::SetSelectable(false),
            Call::AddSettingsEntry("zjhints".to_string()),
        ]
    );
    let mut denied = State::default();
    assert!(denied.update(Event::PermissionRequestResult(PermissionStatus::Denied)));
    assert_eq!(denied.host.calls(), vec![Call::SetSelectable(false)]);
}

#[test]
fn a_preview_that_turns_mouse_on_asks_for_what_mouse_needs() {
    let mut state = bar_with(&[]);
    assert!(preview(
        &mut state,
        preview_args("zjhints", &[("mouse", None, Some("true"))])
    ));
    assert_eq!(
        state.host.calls(),
        vec![
            Call::SetSelectable(true),
            Call::RequestPermission(bar_permissions(true)),
            Call::Subscribe(MOUSE_EVENTS.to_vec()),
        ]
    );
    // Already on, a preview of another option asks for nothing.
    state.host.clear();
    assert!(preview(
        &mut state,
        preview_args("zjhints", &[("max_length", None, Some("9"))])
    ));
    assert_eq!(state.host.calls(), Vec::new());
}

#[test]
fn zellij_s_settings_entry_opens_settings_from_the_bar_of_the_client_that_chose_it() {
    let request = |client: &str| {
        let args = BTreeMap::from([(
            OPEN_PLUGIN_SETTINGS_CLIENT_ID_ARG.to_string(),
            client.to_string(),
        )]);
        PipeMessage::new(
            PipeSource::Plugin(1),
            OPEN_PLUGIN_SETTINGS_MESSAGE,
            &None,
            &Some(args),
            false,
        )
    };
    let mut state = State {
        bar: BarCore {
            client_id: Some(2),
            own_url: Some("zjhints".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(!state.pipe(request("3")));
    assert_eq!(state.host.calls(), Vec::new());
    assert!(!state.pipe(request("2")));
    assert_eq!(
        state.host.calls(),
        vec![Call::OpenSettings {
            message: OPEN_SETTINGS_MESSAGE.to_string(),
            pane_title: "zjhints settings".to_string(),
            own_url: Some("zjhints".to_string()),
        }]
    );
}

#[test]
fn the_settings_screen_comes_up_when_asked_to_open_again() {
    let mut state = State {
        role: Role::Settings(Box::new(SettingsScreen::new(&settings::SCHEMA))),
        ..Default::default()
    };
    let message = PipeMessage::new(
        PipeSource::Plugin(1),
        OPEN_SETTINGS_MESSAGE,
        &None,
        &None,
        false,
    );
    assert!(!state.pipe(message));
    assert_eq!(state.host.calls(), vec![Call::ShowSelf]);
}

#[test]
fn a_click_on_a_hint_runs_its_actions_one_at_a_time() {
    let binding = vec![Action::CloseFocus, Action::TogglePaneFrames];
    let mut state = clicked_bar(vec![(key(BareKey::Char('a')), binding)]);
    let column = column_of(&state, "a");
    state.update(Event::Mouse(Mouse::LeftClick(0, column)));
    let context = BTreeMap::from([(MOUSE_ACTION_CONTEXT.to_string(), String::new())]);
    let run = |action| Call::RunAction {
        action,
        context: context.clone(),
    };
    assert_eq!(state.host.calls(), vec![run(Action::CloseFocus)]);
    state.host.clear();
    state.update(Event::ActionComplete(
        Action::CloseFocus,
        None,
        context.clone(),
    ));
    assert_eq!(state.host.calls(), vec![run(Action::TogglePaneFrames)]);
}

#[test]
fn a_right_click_opens_zellij_s_context_menu_for_the_bar() {
    let mut state = clicked_bar(four_hints());
    state.update(Event::Mouse(Mouse::RightClick(0, 9)));
    assert_eq!(
        state.host.calls(),
        vec![Call::OpenContextMenu {
            target: ContextMenuTarget::Bar,
            line: 0,
            column: 9,
        }]
    );
}

#[test]
fn a_menu_is_a_prompt_and_its_answer_runs_the_chosen_entry() {
    let mut state = clicked_bar(focus_keymap());
    state.update(Event::Mouse(Mouse::LeftClick(
        0,
        column_of(&state, "focus"),
    )));
    let calls = state.host.calls();
    let [Call::Prompt { request_id, .. }] = calls.as_slice() else {
        panic!("expected one prompt, got {calls:?}");
    };
    state.host.clear();
    state.update(Event::PromptResult(
        *request_id,
        PromptResult::Answered(PromptValue::Choice("1".to_string())),
    ));
    assert_eq!(
        state.host.calls(),
        vec![Call::RunAction {
            action: Action::MoveFocus {
                direction: Direction::Right
            },
            context: BTreeMap::from([(MOUSE_ACTION_CONTEXT.to_string(), String::new())]),
        }]
    );
}

#[test]
fn rendering_publishes_the_line_on_every_zjstatus_pipe() {
    let mut state = drawing_bar();
    state.pipe_names = pipe_names_from_config(&BTreeMap::new());
    let line = state.draw_bar().0;
    state.render(1, 40);
    let published = |pipe: &str| Call::Broadcast {
        name: "pipe".to_string(),
        payload: format!("zjstatus::pipe::pipe_{pipe}::{line}"),
    };
    assert_eq!(
        state.host.calls(),
        vec![published("zjhints"), published("zjstatus_hints")]
    );
}

/// The line `state` publishes to zjstatus, as plain text.
fn published_line(state: &mut State) -> String {
    state.host.clear();
    state.render(1, 40);
    let calls = state.host.calls();
    let Some(Call::Broadcast { payload, .. }) = calls.first() else {
        panic!("nothing was published: {calls:?}");
    };
    strip_ansi(payload.trim_start_matches("zjstatus::pipe::pipe_zjhints::"))
}

#[test]
fn the_theme_s_mode_prefix_is_whole_unless_it_opens_the_bar_s_own_pane() {
    use zjkit::theme::{ARROW, PILL_LEFT, PILL_RIGHT};
    let bar = |options: &[(&str, &str)]| {
        let mut state = bar_with(options);
        state.show_mode = true;
        state.pipe_names = vec!["zjhints".to_string()];
        state
    };
    let starts = |line: String, prefix: String| {
        assert!(
            line.starts_with(&prefix),
            "{line:?} does not start {prefix:?}"
        );
    };
    // Feeding zjstatus, with no pane of its own, the line is set among
    // zjstatus's own widgets, so the pill is closed at both ends.
    let mut state = bar(&[("format_theme", "pills")]);
    starts(
        published_line(&mut state),
        format!("{PILL_LEFT} NORMAL {PILL_RIGHT}"),
    );
    // In its own pane the line opens the pane, so the pill is open there.
    state.bar.pane_width = Some(40);
    starts(published_line(&mut state), format!(" NORMAL {PILL_RIGHT}"));
    // A powerline arrow keeps its open end either way.
    let mut state = bar(&[("format_theme", "powerline")]);
    starts(published_line(&mut state), format!(" NORMAL {ARROW}"));
    // A format of the user's own is drawn as written.
    let mut state = bar(&[("format_theme", "pills"), ("mode_format", "<{mode}>")]);
    starts(published_line(&mut state), "<NORMAL>".to_string());
}

#[test]
fn rendering_a_hidden_bar_gives_its_row_back_once() {
    let mut state = drawing_bar();
    state.bar.change_visibility(Request::Hide, None);
    state.render(1, 40);
    state.render(1, 40);
    assert_eq!(state.host.calls(), vec![Call::SetCollapsed(true)]);
}
