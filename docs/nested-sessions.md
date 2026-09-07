# Nested sessions

Zellij 0.45 added built-in handling for running one Zellij session inside another: descend into the nested session with its own keybinding, and ascend back out with another. Without help from the status bar, a nested session still draws its own full set of chrome on top of the host's, including its own copy of this plugin's hints.

This page covers how the two bars cooperate instead, and how to run this plugin in a pane of its own so that cooperation is possible at all.

## The state this relies on

The plugin picks up two pieces of state Zellij already reports on every `ModeUpdate`:

- `session_ancestry`: non-empty when this session is nested inside another.
- `session_ascended` and `session_dimmed`: set when this session is not the one currently receiving input, meaning a host that has descended into a child, or a nested session not yet ascended into.

Those last two are the same fields Zellij's own bundled tab-bar and compact-bar plugins use to dim their chrome, so this plugin dims in step with core rather than inventing its own signal.

```kdl
hide_when_nested    true // default
collapse_when_empty true // default
dim_when_unfocused  true // default
dim_strength        "0.5" // default, clamped to 0.0-1.0
```

`dim_strength` has to be quoted. Zellij's own plugin-config parser only has a converter for string, integer, and boolean node values; a bare decimal like `0.5` is none of those, so Zellij fails to parse the whole config with "Failed to parse plugin block configuration" rather than falling back to the default. This is a Zellij core gap, not something this plugin controls.

## Hiding a nested session's hints

`hide_when_nested` makes the plugin render nothing at all while nested, rather than hiding the hints line the way `hide_in_base_mode` does for the base mode. The point is to let one shared layout serve both a host and a nested session: point both at the same config, and the nested session's copy renders empty while the host's keeps working normally. Set it to `false` if you would rather a nested session kept showing its own hints.

The one time a nested session draws its bar anyway is when the host has taken its own off the screen. Zellij has two fullscreens. The ordinary one (`ToggleFocusFullscreen`, `Ctrl p` then `f`) expands a pane over the viewport only, so the host's status bar and tab bar stay put and there is still a bar below to defer to. The other (`ToggleFocusNoUiFullscreen`, `Ctrl p` then `Shift f`) expands over the whole display and hides every other pane, the host's bars included.

Deferring in that second case would leave the screen with no hints anywhere, so the nested session takes its bar back for as long as it lasts. This needs no configuring, and it comes back off when the host leaves that fullscreen.

## Giving the row back

A layout that puts this plugin in its own pane reserves a row for it, and that row stays reserved whether or not there is anything on it. A nested session with `hide_when_nested` on therefore draws a blank line where its hints would have been, wasting a row of the terminal for nothing.

`collapse_when_empty` (default `true`) hands that row to the panes around it whenever the bar has nothing to draw, and takes it back the moment it does. The pane keeps its place in the layout the whole time, so the row that comes back is the exact one the layout asked for rather than an approximation of it. Set it to `false` to keep the row reserved unconditionally, for a layout whose proportions should not shift.

This is not specific to nesting. `hide_in_base_mode` empties the bar too, and so does any configuration that leaves nothing to show, and the row goes away in those cases as well.

> [!NOTE]
> This needs the `set_self_collapsed` plugin command, which is not in a released Zellij yet. On a Zellij without it the setting does nothing and the row stays where it is.

## Dimming

`dim_when_unfocused` blends colors toward neutral gray by `dim_strength` whenever this session is not the one currently receiving input. `dim_strength` ranges from `0.0`, no change, to `1.0`, full gray.

This applies equally to the theme palette's default styling and to anything you have styled yourself, from `key_format` and `desc_format` through to the per-mode `mode_format_<mode>`. A `$alias` or `#hex` color in one of those fades the same amount, so a hand-styled bar dims along with everything else rather than staying at fixed brightness regardless of focus.

Only true RGB colors can be blended this way. A named ANSI color or an indexed `color<n>` has no RGB triple to fade, so it renders unaffected.

If you also use the [zjstatus fork with the matching feature](https://github.com/myah-mitchell/zjstatus), its `dim_when_unfocused` and `dim_strength` use the same blend, so a host's top and bottom bars dim together rather than one changing and the other staying bright.

## While descended into a nested session

Whenever this session's own focus has moved to a nested child it is hosting, its own hints would describe a mode it is not actually receiving input in. The keys the user is pressing belong to the nested session, so that is what the bar describes: its current mode, and hints built from its real keybindings.

### Everything applies unchanged

The nested session's hints go through exactly the same machinery this session's own hints do. Every option in the [configuration reference](configuration.md) applies unchanged, from curation and ordering through styling, dimming and truncation, so the bar looks and behaves the same whichever session it happens to be describing.

The colors are this session's, since it is this session's bar. A nested session with a different theme does not repaint the host's status line.

### Nothing needs configuring

The plugin asks each nested session for its keybindings once, the first time that session reports itself, and follows its mode from then on. A nested session that reloads its config sends its new mode and keybindings without being asked again, so the bar does not go on describing bindings that session no longer has.

Nested sessions are tracked separately per pane, so a layout hosting several of them shows the hints of the one the keys are actually going to.

### When the nested session cannot answer

This needs Zellij to report the nested session's mode and keybindings, which older releases do not do. When there is nothing to draw hints from, the plugin shows a small placeholder naming the keys that ascend back out, its own `nested_ascend_keys`. That happens in the moment between descending and the nested session answering, and permanently for a nested session running a Zellij too old to answer at all.

The placeholder is styled and configured exactly like any other hint. Its key part is the ascend keys (`Ctrl o ]`), and its description is "return to host". Its concept id is `descended`, so every option ending in `_descended` overrides it individually, and it is truncated to fit the terminal the same way normal hints are. See [Styling one hint](styling.md#styling-one-hint) and [Labels](labels.md#ids-and-labels).

### How this interacts with the other settings

All of this is independent of `dim_when_unfocused` and `dim_strength`, and of whether this session is itself also nested under something else. It keys off `session_dimmed`, which Zellij sets on a session's own `ModeInfo` the moment its focus defers to a child, host or not. What the bar describes is not a display preference the way dimming is.

Being descended also outranks `hide_in_base_mode`. A nested session sitting in its base mode still gets its hints shown, because descending is itself the out-of-the-ordinary state that option exists to keep off the bar.

`hide_when_nested` still wins over both: a session in the middle of a chain renders nothing at all, so there is nowhere to put the hints of the session below it either. The exception is a session whose own host has covered its bar, which makes it the only bar left on the screen, and it then shows the hints of the session it has descended into, the same as any other host would.

## A known gap: headless plugins see none of this

Everything above depends on four fields arriving on this plugin's own `ModeInfo`: `nested_ascend_keys`, and the three under [The state this relies on](#the-state-this-relies-on). Loaded as a real pane, meaning a `pane { plugin location=... }` entry in a layout the way zjstatus itself usually is, those fields arrive normally.

Loaded headless instead, via a top-level `load_plugins` block with no pane of its own and its output reaching the screen only through zjstatus's pipe protocol (`{pipe_zjhints}`), none of those fields ever arrive. `hide_when_nested`, `dim_when_unfocused` and `dim_strength` all silently do nothing, as does the descended placeholder. `session_ancestry` stays empty forever regardless of actual nesting state.

This was confirmed against Zellij 0.46: `update_all_clients_nesting_mode_info` in `zellij-server/src/screen.rs` updates `ModeInfo` for every pane in every tab. It never notifies plugins in `background_plugin_subscriptions`, the list a `load_plugins` block builds, the way Zellij's regular mode-switch path does. It is a Zellij core gap, not something either plugin's own code can work around.

## Standing alone

Giving this plugin its own pane, rather than piping it through zjstatus, is the only way to get the nested-session behavior above, and it needs no zjstatus at all. The one thing you give up by dropping the pipe is zjstatus's own `{mode}` widget on that line. `show_mode` fills that gap:

```kdl
show_mode true // default false
mode_format "#[fg=#cba6f7,bold] {mode} " // default unset, uses the ribbon_selected theme color
```

`show_mode` prefixes the hints line with the current input mode, rendering as " NORMAL ", so this plugin reads the same as `{mode}{hints}` did through the pipe, without needing zjstatus's widget alongside it. `mode_format` is a zjstatus-style format string with a `{mode}` placeholder standing in for the mode's name: `NORMAL`, `PANE`, `LOCKED`, and the rest. When unset, the prefix uses the theme's `ribbon_selected` colors, the same "currently active" style Zellij's own ribbons use. The prefix dims along with the rest of the line under `dim_when_unfocused`, and counts against `max_length` the same way a hint would.

Each mode can also have its own format, the same way zjstatus's own `mode_normal` and `mode_locked` do, so a mode can carry its own icon instead of sharing one generic look:

```kdl
mode_format_normal "#[bg=blue,bold] NORMAL #[bg=blue,fg=black,bold] 󰌌 #[fg=blue]"
mode_format_locked "#[bg=red,bold] LOCKED #[bg=red,fg=black,bold] 󰌾 #[fg=red]"
mode_format_pane   "#[bg=cyan,bold]  PANE  #[bg=cyan,fg=black,bold] 󰯌 #[fg=cyan]"
```

The suffix after `mode_format_` matches zjstatus's own per-mode config keys exactly:

```text
normal  locked   resize  pane    tab          scroll       enter_search
search  session  move    prompt  rename_tab   rename_pane  tmux
```

A mode with its own `mode_format_<mode>` uses that. A mode without one falls back to the global `mode_format`, and with neither set every mode uses the same theme-colored default. Icons already set up for zjstatus's own per-mode keys copy over as-is, with `mode_normal` becoming `mode_format_normal`.
