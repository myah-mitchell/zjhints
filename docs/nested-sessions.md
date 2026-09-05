# Nested sessions

Zellij 0.45 added built-in handling for running one Zellij session inside
another: descend into the nested session with its own keybinding, and
ascend back out with another. Without help from the status bar, though, a
nested session still draws its own full set of chrome on top of the host's,
including its own copy of this plugin's hints.

This plugin picks up two pieces of state Zellij already reports on every
`ModeUpdate` so the two bars can cooperate instead:

- `session_ancestry`: non-empty when this session is nested inside another.
- `session_ascended` / `session_dimmed`: set when this session is not the
  one currently receiving input: a host that has descended into a child,
  or a nested session not yet ascended into. These are the same fields
  Zellij's own bundled tab-bar and compact-bar plugins use to dim their
  chrome, so this plugin dims in step with core rather than inventing its
  own signal.

```kdl
hide_when_nested   true // default
dim_when_unfocused true // default
dim_strength       "0.5" // default, clamped to 0.0-1.0
```

`dim_strength` has to be quoted. Zellij's own plugin-config parser only has a
converter for string, integer, and boolean node values; a bare decimal like
`0.5` isn't any of those, so Zellij fails to parse the whole config with
"Failed to parse plugin block configuration" rather than falling back to the
default. This is a Zellij core gap, not something this plugin controls.

## Hiding a nested session's hints

`hide_when_nested` makes the plugin render nothing at all while nested,
rather than hiding the hints line the way `hide_in_base_mode` does for the
base mode. The point is to let one shared layout serve both a host and a
nested session: point both at the same config, and the nested session's
copy renders empty while the host's keeps working normally. Set it to
`false` if you would rather a nested session kept showing its own hints.

## Dimming

`dim_when_unfocused` blends colors toward neutral gray by `dim_strength`
whenever this session isn't the one currently receiving input.
`dim_strength` ranges from `0.0` (no change) to `1.0` (full gray). This
applies equally to the theme palette's default styling and to any custom
`key_format`/`desc_format`/`hint_spacer`/`drop_indicator`/`mode_format`
(including `mode_format_<mode>`) you've configured: a `$alias` or `#hex`
color in one of those fades the same amount, so a hand-styled bar dims along
with everything else rather than staying at fixed brightness regardless of
focus. Only true RGB colors can be blended this way; a named ANSI color or
an indexed `color<N>` has no RGB triple to fade, so it renders unaffected.

If you also use the [zjstatus fork with the matching
feature](https://github.com/myah-mitchell/zjstatus), its `dim_when_unfocused`
and `dim_strength` use the same blend, so a host's top and bottom bars dim
together rather than one changing and the other staying bright.

## While descended into a nested session

Whenever this session's own focus has moved to a nested child it is
hosting, its own hints would describe a mode it isn't actually receiving
input in. Rather than show them anyway, the plugin shows a small
placeholder naming the keys that ascend back out (its own
`nested_ascend_keys`), styled and configured exactly like any other hint:
its key part is the ascend keys (`Ctrl o ]`), and its description is
"return to host". `key_format`/`desc_format` style it the same way they
style every hint, and its concept id is `descended`, so
`label_descended`, `key_format_descended`, `desc_format_descended`, and
`keys_descended` all override it individually the same way they would for
any other hint (see [Styling one hint](styling.md#styling-one-hint) and
[Labels](labels.md#labels)). It is truncated to fit the terminal the same
way normal hints are.

This check is independent of `dim_when_unfocused`/`dim_strength`, and of
whether this session is itself also nested under something else: it fires
whenever `session_dimmed` is set, which Zellij sets on a session's own
`ModeInfo` the moment its focus defers to a child, host or not. Whether to
show the placeholder at all isn't a display preference the way dimming is.

This is **not** a live view of the nested session's actual mode or hints.
Zellij's plugin API does not currently expose one session's mode to
another, so there is no cross-session data this plugin can draw on for
that. It is a fixed, locally-known hint about how to get back, nothing
more. A future Zellij release may add the missing piece; until then, the
nested session's own bar (if `hide_when_nested false`) is the source of
truth for what mode it is actually in.

## A known gap: headless plugins don't see any of this

Everything above depends on `session_ancestry`/`session_ascended`/
`session_dimmed`/`nested_ascend_keys` on this plugin's own `ModeInfo`. If
this plugin is loaded as a real pane (a `pane { plugin location=... }`
entry in a layout, the way zjstatus itself usually is), those fields
arrive normally.

If instead it is loaded headless, via a top-level `load_plugins` block,
with no pane of its own, and its output reaches the screen only through
zjstatus's pipe protocol (`{pipe_zjstatus_hints}`), none of those fields
ever arrive: `hide_when_nested`, `dim_when_unfocused`/`dim_strength`, and
the descended placeholder all silently do nothing, and `session_ancestry`
stays empty forever, regardless of actual nesting state. This was
confirmed against Zellij 0.46: `update_all_clients_nesting_mode_info` in
`zellij-server/src/screen.rs` updates `ModeInfo` for every pane in every
tab, but never notifies plugins in `background_plugin_subscriptions` (the
`load_plugins` list) the way Zellij's regular mode-switch path does. It is
a Zellij core gap, not something either plugin's own code can work around.

If you use the pipe integration today, giving this plugin its own pane
instead is the only way to get the nested-session behavior above. That
also means losing zjstatus's own `{mode}` widget on that line, since it
lives in the other plugin's pane now. `show_mode` fills that gap:

```kdl
show_mode true // default false
mode_format "#[fg=#cba6f7,bold] {mode} " // default unset, uses the ribbon_selected theme color
```

`show_mode` prefixes the hints line with the current input mode, e.g.
" NORMAL ", so this plugin reads the same as `{mode}{hints}` did through
the pipe, without needing zjstatus's widget alongside it. `mode_format` is
a zjstatus-style format string with a `{mode}` placeholder standing in for
the mode's name (`NORMAL`, `PANE`, `LOCKED`, ...); when unset, the prefix
uses the theme's `ribbon_selected` colors, the same "currently active"
style Zellij's own ribbons use. The prefix dims along with the rest of the
line under `dim_when_unfocused`, and counts against `max_length` the same
way a hint would.

Each mode can also have its own format, the same way zjstatus's own
`mode_normal`/`mode_locked`/etc. do, so a mode can carry its own icon
instead of sharing one generic look:

```kdl
mode_format_normal "#[bg=blue,bold] NORMAL #[bg=blue,fg=black,bold] 󰌌 #[fg=blue]"
mode_format_locked "#[bg=red,bold] LOCKED #[bg=red,fg=black,bold] 󰌾 #[fg=red]"
mode_format_pane   "#[bg=sky,bold]  PANE  #[bg=sky,fg=black,bold] 󰯌 #[fg=sky]"
```

The suffix after `mode_format_` matches zjstatus's own per-mode config keys
exactly: `normal`, `locked`, `resize`, `pane`, `tab`, `scroll`,
`enter_search`, `search`, `rename_tab`, `rename_pane`, `session`, `move`,
`prompt`, `tmux`. A mode with its own `mode_format_<mode>` uses that; a
mode without one falls back to the global `mode_format`; with neither set,
every mode uses the same theme-colored default. If you already have icons
set up for zjstatus's `mode_normal`/etc., the values can be copied over
as-is (`mode_normal "..."` becomes `mode_format_normal "..."`).
