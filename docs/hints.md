# Hints

Where the hints come from: a built-in curated set, and optional discovery of everything else your config binds.

## The curated list

Hints come from two places, and most of this document refers to both. By default
you see only the first: the **curated list**, a small table built into the
plugin naming, for each of the main modes, the actions people reach for most,
presented in a deliberate order with short hand-written labels.

It exists because a faithful list of your keybindings is not automatically a
readable one. Left to itself the plugin would show bindings in whatever order
Zellij reports them, labeled from action names: `switch_to_mode_pane` reading
as "switch to mode pane". The curated list is what makes Normal mode open as
`pane  tab  resize  move  scroll  session  quit` instead.

What it covers:

- **Normal, Pane, Tab, Resize, Move, Scroll, Search and Session** have entries.
  Other modes (Locked, Tmux, RenameTab and friends) have none, so they show
  only their escape hatch back to Normal unless
  [discovery](#discovered-hints) is on.
- **Only bindings you actually have.** An entry whose action is not bound in
  your config is skipped, so unbinding something removes its hint rather than
  leaving a dead one.
- **Grouped concepts that no single action describes**: the four resize
  directions as one `resize`, or Session mode's plugin launchers.
- **The way out of each mode.** Every key bound to `SwitchToMode "Normal"`
  (usually Enter and Esc both) forms a single `mode_normal` hint, so a mode
  always shows how to leave it even with discovery off.

Nothing about it is fixed: every entry is a normal hint with an
[id](labels.md#ids), so it can be relabeled with `label_<id>`, moved with
[`hint_order`](ordering.md#hint-order), or hidden with an empty label.

## Discovered hints

The second source is **discovery**, off by default and enabled with:

```kdl
discover_hints true
```

With it on, every *other* keybinding enabled in the current mode is found and
appended after the curated list. Nothing your config binds is left out: custom
binds appear, and so do the many defaults the curated list omits (the `Alt-*`
quick keys, `lock` in Normal mode), along with modes like Locked and Tmux that
have no curated entries at all.

Keys resolving to the same [id](labels.md#labels) become a single hint, so the four
directional focus keys collapse into one `focus` rather than four entries.

Hints are collected before anything is drawn, so the two sources merge rather
than duplicate: a curated `focus` on `hjkl` and a discovered `focus` on the
arrows become one hint carrying all eight keys.

### Which to use

The curated list is the shorter, more readable default: a handful of hints per
mode, worded for scanning. Discovery is the complete one: it will not let a
binding go unmentioned, but a mode can easily produce more hints than a status
bar has room for, at which point [fitting](fitting.md#fitting-the-bar) starts dropping
them again.

Turn it on if you want the hints to be an exhaustive reference for your config,
or while you are learning a keymap you just changed. Leave it off if you want a
compact bar that names the things you reach for.

Everything else works the same either way: discovered hints have
[ids](labels.md#ids) exactly like curated ones, so they relabel, reorder and hide
identically.

### Shared bindings

Zellij's default config binds the mode switches (`pane`, `tab`, `session`, …) in
every non-locked mode via `shared_except` groups. Discovering those in each mode
means every mode re-lists the same globals, which is mostly noise.

`hide_shared_hints` (default `true`) suppresses any discovered binding whose key
maps to the same action in the base mode, so each mode shows only what is new in
it. It has no effect while you are *in* the base mode (Normal still lists the
globals), and it respects a non-Normal `default_mode` such as `locked`.

Set it to `false` to have every mode list everything it accepts.
