//! What `ModeInfo` says about nested Zellij sessions: whether this one is
//! nested, whether it has handed the keyboard to one it hosts, and how much
//! a bar should fade while it is not where the keyboard goes.

use zellij_tile::prelude::ModeInfo;

/// Whether this session is nested inside another Zellij session.
pub fn is_nested(mode_info: &ModeInfo) -> bool {
    !mode_info.session_ancestry.is_empty()
}

/// Whether the host has expanded this session's pane over its whole display,
/// taking its own bars off the screen with it.
///
/// Zellij has two fullscreens. The ordinary one (`ToggleFocusFullscreen`)
/// expands a pane over the viewport only, so the host's tab bar and status
/// bar stay where they are; Zellij does not report that one here at all. The
/// other (`ToggleFocusNoUiFullscreen`) expands over the whole display and
/// hides every other pane, the host's bars included, and that is the one this
/// reports. It is set only for a nested session, since it arrives from a
/// host.
pub fn host_ui_is_covered(mode_info: &ModeInfo) -> bool {
    mode_info.host_fullscreen == Some(true)
}

/// Whether this session currently has its own focus deferred to a nested
/// child it is hosting, regardless of whether this session is itself also
/// nested inside something else. This is the raw fact, independent of any
/// dimming preference.
pub fn is_host_descended(mode_info: &ModeInfo) -> bool {
    mode_info.session_dimmed == Some(true)
}

/// Whether this session is nested and its host has ascended out of it, so
/// nothing in it is receiving input.
pub fn is_ascended_out_of(mode_info: &ModeInfo) -> bool {
    mode_info.session_ascended == Some(true)
}

/// The dim strength to render with right now, in `crate::dim::color`'s
/// `0.0..=1.0` scale: `0.0` (no change) unless `dim_when_unfocused` is set
/// and this session is the dimmed side of a nested-session pair, a host that
/// has descended into a child, or a nested session not currently ascended
/// into.
///
/// `session_ascended` and `session_dimmed` are the same fields Zellij's own
/// bundled tab-bar and compact-bar plugins use for this, and the zjstatus
/// fork's `ZellijState::dim_amount` mirrors this exactly so a shared layout's
/// bars dim in step.
pub fn dim_amount(mode_info: &ModeInfo, dim_when_unfocused: bool, strength: f32) -> f32 {
    let is_dimmed = is_ascended_out_of(mode_info) || is_host_descended(mode_info);
    if dim_when_unfocused && is_dimmed {
        strength
    } else {
        0.0
    }
}
