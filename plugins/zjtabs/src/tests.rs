use super::*;
use zjkit::bar::Handled;
use zjkit::host::Call;

fn cfg(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// A bar loaded with `pairs` for its configuration.
fn loaded(pairs: &[(&str, &str)]) -> State {
    let mut state = State::default();
    state.load(cfg(pairs));
    state
}

#[test]
fn loading_the_bar_asks_for_its_permissions_and_events() {
    let state = loaded(&[]);
    let calls = state.host.calls();
    // `mouse` is on by default.
    assert!(calls.contains(&Call::RequestPermission(bar_permissions(true))));
    assert!(calls.contains(&Call::Subscribe(BAR_EVENTS.to_vec())));
    assert!(calls.contains(&Call::Subscribe(MOUSE_EVENTS.to_vec())));
}

#[test]
fn a_bar_without_mouse_asks_for_nothing_the_mouse_needs() {
    let state = loaded(&[("mouse", "false")]);
    let calls = state.host.calls();
    assert!(calls.contains(&Call::RequestPermission(BAR_PERMISSIONS.to_vec())));
    assert!(!calls.contains(&Call::Subscribe(MOUSE_EVENTS.to_vec())));
}

#[test]
fn asking_for_mouse_keeps_the_permissions_the_bar_always_needs() {
    assert_eq!(bar_permissions(false), BAR_PERMISSIONS.to_vec());
    let permissions = bar_permissions(true);
    for permission in BAR_PERMISSIONS.iter().chain(MOUSE_PERMISSIONS.iter()) {
        assert!(permissions.contains(permission), "{permission:?}");
    }
    assert!(!bar_permissions(false).contains(&PermissionType::RunActionsAsUser));
}

#[test]
fn the_settings_screen_asks_for_the_permissions_of_a_bar_with_mouse() {
    assert_eq!((settings::SCHEMA.permissions)(), bar_permissions(true));
}

#[test]
fn turning_mouse_on_later_asks_for_what_the_mouse_needs() {
    let mut state = loaded(&[("mouse", "false")]);
    state.host.clear();
    assert!(BarCore::follow(
        &mut state,
        &BAR,
        Handled::Reconfigure(cfg(&[]))
    ));
    let calls = state.host.calls();
    assert!(calls.contains(&Call::Subscribe(MOUSE_EVENTS.to_vec())));
    assert!(calls.contains(&Call::RequestPermission(bar_permissions(true))));
}

#[test]
fn unset_options_take_their_defaults() {
    let state = loaded(&[]);
    assert_eq!(state.tab_format, None);
    assert_eq!(state.active_tab_format, None);
    assert_eq!(state.tab_spacer, DEFAULT_TAB_SPACER);
    assert_eq!(
        state.flag_formats,
        [
            DEFAULT_FULLSCREEN_FORMAT,
            DEFAULT_SYNC_FORMAT,
            DEFAULT_FLOATING_FORMAT,
            DEFAULT_BELL_FORMAT,
        ]
    );
    assert_eq!(state.overflow_left_format, None);
    assert_eq!(state.overflow_right_format, None);
    assert_eq!(state.index_from_zero, DEFAULT_INDEX_FROM_ZERO);
    assert_eq!(state.rename_placeholder, DEFAULT_RENAME_PLACEHOLDER);
    assert_eq!(state.overflow_str, DEFAULT_OVERFLOW_STR);
    assert_eq!(state.auto_width, DEFAULT_AUTO_WIDTH);
    assert_eq!(state.max_length, DEFAULT_MAX_LENGTH);
    assert!(!state.wide_ambiguous);
    assert_eq!(state.hide_when_nested, DEFAULT_HIDE_WHEN_NESTED);
    assert_eq!(state.collapse_when_empty, DEFAULT_COLLAPSE_WHEN_EMPTY);
    assert_eq!(state.dim_when_unfocused, DEFAULT_DIM_WHEN_UNFOCUSED);
    assert_eq!(state.dim_strength, DEFAULT_DIM_STRENGTH);
    assert_eq!(state.mouse, DEFAULT_MOUSE);
}

#[test]
fn every_option_is_read_from_its_key() {
    let state = loaded(&[
        ("tab_format", "{name}"),
        ("active_tab_format", "[{name}]"),
        ("tab_spacer", "|"),
        ("fullscreen_format", "F"),
        ("sync_format", "S"),
        ("floating_format", "W"),
        ("bell_format", "B"),
        ("overflow_left_format", "<{count}"),
        ("overflow_right_format", "{count}>"),
        ("index_from_zero", "true"),
        ("rename_placeholder", "?"),
        ("overflow_str", "~"),
        ("auto_width", "false"),
        ("max_length", "40"),
        ("ambiguous_width", "2"),
        ("hide_when_nested", "false"),
        ("collapse_when_empty", "false"),
        ("dim_when_unfocused", "false"),
        ("dim_strength", "0.25"),
        ("mouse", "false"),
    ]);
    assert_eq!(state.tab_format.as_deref(), Some("{name}"));
    assert_eq!(state.active_tab_format.as_deref(), Some("[{name}]"));
    assert_eq!(state.tab_spacer, "|");
    assert_eq!(state.flag_formats, ["F", "S", "W", "B"]);
    assert_eq!(state.overflow_left_format.as_deref(), Some("<{count}"));
    assert_eq!(state.overflow_right_format.as_deref(), Some("{count}>"));
    assert!(state.index_from_zero);
    assert_eq!(state.rename_placeholder, "?");
    assert_eq!(state.overflow_str, "~");
    assert!(!state.auto_width);
    assert_eq!(state.max_length, 40);
    assert!(state.wide_ambiguous);
    assert!(!state.hide_when_nested);
    assert!(!state.collapse_when_empty);
    assert!(!state.dim_when_unfocused);
    assert_eq!(state.dim_strength, 0.25);
    assert!(!state.mouse);
}

#[test]
fn an_empty_format_means_zellij_s_look_but_an_empty_spacer_or_flag_means_none() {
    let state = loaded(&[
        ("format_theme", "flat"),
        ("tab_format", ""),
        ("tab_spacer", ""),
        ("sync_format", ""),
    ]);
    assert_eq!(state.tab_format, None);
    assert!(state.active_tab_format.is_some());
    assert_eq!(state.tab_spacer, "");
    assert_eq!(state.flag_formats[1], "");
}

#[test]
fn the_themes_are_laid_under_the_user_s_options() {
    let state = loaded(&[
        ("format_theme", "pills"),
        ("alias_theme", "ascii"),
        ("active_tab_format", "*{name}*"),
    ]);
    assert_eq!(state.active_tab_format.as_deref(), Some("*{name}*"));
    assert!(state
        .tab_format
        .as_deref()
        .is_some_and(|format| format.contains(zjkit::theme::PILL_LEFT)));
    assert_eq!(state.flag_formats, [" Z", " S", " F", " !"]);
    assert_ne!(state.overflow_str, DEFAULT_OVERFLOW_STR);
}

#[test]
fn hidden_hides_the_bar() {
    let state = loaded(&[("hidden", "true")]);
    assert!(state.bar.visibility.is_hidden());
}
