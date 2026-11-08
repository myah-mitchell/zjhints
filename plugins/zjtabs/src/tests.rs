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
    // `mouse` and `mouse_close_confirm` are on by default.
    assert!(calls.contains(&Call::RequestPermission(bar_permissions(true, true))));
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
fn a_bar_with_mouse_but_no_close_confirmation_asks_for_no_prompt() {
    let state = loaded(&[("mouse_close_confirm", "false")]);
    let calls = state.host.calls();
    assert!(calls.contains(&Call::RequestPermission(bar_permissions(true, false))));
    assert!(calls.contains(&Call::Subscribe(MOUSE_EVENTS.to_vec())));
}

#[test]
fn the_permission_sets_grow_with_mouse_and_then_the_close_confirmation() {
    // Without the mouse there is no middle click to confirm, so
    // `mouse_close_confirm` alone adds nothing.
    for mouse_close_confirm in [false, true] {
        assert_eq!(
            bar_permissions(false, mouse_close_confirm),
            BAR_PERMISSIONS.to_vec()
        );
    }
    let mut with_mouse = BAR_PERMISSIONS.to_vec();
    with_mouse.extend(MOUSE_PERMISSIONS);
    assert_eq!(bar_permissions(true, false), with_mouse);
    let mut with_confirmation = with_mouse.clone();
    with_confirmation.extend(CLOSE_CONFIRM_PERMISSIONS);
    assert_eq!(bar_permissions(true, true), with_confirmation);
    assert!(!with_mouse.contains(&PermissionType::OpenTerminalsOrPlugins));
    assert!(with_confirmation.contains(&PermissionType::OpenTerminalsOrPlugins));
}

#[test]
fn the_settings_screen_asks_for_the_permissions_of_a_bar_with_every_option_on() {
    assert_eq!(
        (settings::SCHEMA.permissions)(),
        bar_permissions(true, true)
    );
    assert_eq!((BAR.permissions)(true), bar_permissions(true, true));
    assert_eq!((BAR.permissions)(false), bar_permissions(false, false));
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
    assert!(calls.contains(&Call::RequestPermission(bar_permissions(true, true))));
}

#[test]
fn turning_the_close_confirmation_on_later_asks_for_the_larger_set() {
    let mut state = loaded(&[("mouse_close_confirm", "false")]);
    state.host.clear();
    assert!(BarCore::follow(
        &mut state,
        &BAR,
        Handled::Reconfigure(cfg(&[]))
    ));
    // The prompt's answer comes with the mouse's events, subscribed already.
    assert_eq!(
        state.host.calls(),
        vec![
            Call::SetSelectable(true),
            Call::RequestPermission(bar_permissions(true, true)),
        ]
    );
}

#[test]
fn turning_the_close_confirmation_off_asks_for_nothing() {
    let mut state = loaded(&[]);
    state.host.clear();
    assert!(BarCore::follow(
        &mut state,
        &BAR,
        Handled::Reconfigure(cfg(&[("mouse_close_confirm", "false")]))
    ));
    assert_eq!(state.host.calls(), vec![]);
}

#[test]
fn the_close_confirmation_alone_asks_for_nothing_while_mouse_is_off() {
    let mut state = loaded(&[("mouse", "false"), ("mouse_close_confirm", "false")]);
    state.host.clear();
    assert!(BarCore::follow(
        &mut state,
        &BAR,
        Handled::Reconfigure(cfg(&[("mouse", "false")]))
    ));
    assert_eq!(state.host.calls(), vec![]);
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
    assert_eq!(state.hide_when_nested, DEFAULT_HIDE_WHEN_NESTED);
    assert_eq!(state.collapse_when_empty, DEFAULT_COLLAPSE_WHEN_EMPTY);
    assert_eq!(state.dim_when_unfocused, DEFAULT_DIM_WHEN_UNFOCUSED);
    assert_eq!(state.dim_strength, DEFAULT_DIM_STRENGTH);
    assert_eq!(state.show_mode, DEFAULT_SHOW_MODE);
    assert_eq!(state.mode_format, None);
    // Unset rather than its default, so Zellij's own setting can decide.
    assert_eq!(state.show_session, None);
    assert_eq!(state.session_format, None);
    assert_eq!(state.mouse, DEFAULT_MOUSE);
    assert_eq!(state.mouse_wheel_reversed, DEFAULT_MOUSE_WHEEL_REVERSED);
    assert_eq!(state.mouse_close_confirm, DEFAULT_MOUSE_CLOSE_CONFIRM);
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
        ("hide_when_nested", "false"),
        ("collapse_when_empty", "false"),
        ("dim_when_unfocused", "false"),
        ("dim_strength", "0.25"),
        ("show_mode", "false"),
        ("mode_format", "<{mode}>"),
        ("show_session", "false"),
        ("session_format", "[{session}]"),
        ("mouse", "false"),
        ("mouse_wheel_reversed", "true"),
        ("mouse_close_confirm", "false"),
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
    assert!(!state.hide_when_nested);
    assert!(!state.collapse_when_empty);
    assert!(!state.dim_when_unfocused);
    assert_eq!(state.dim_strength, 0.25);
    assert!(!state.show_mode);
    assert_eq!(state.mode_format.as_deref(), Some("<{mode}>"));
    assert_eq!(state.show_session, Some(false));
    assert_eq!(state.session_format.as_deref(), Some("[{session}]"));
    assert!(!state.mouse);
    assert!(state.mouse_wheel_reversed);
    assert!(!state.mouse_close_confirm);
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

// Anyone who can launch a plugin can add a pane with any options to a
// running instance, and a panic in one slot takes every bar down. See
// `zjkit::junk`.

#[test]
fn no_options_at_all_make_an_instance_panic() {
    zjkit::junk::shake::<State>(&BAR, 0x79c637b8, 400, || {});
}
