# Hints

Where the hints come from: a built-in curated set, and discovery of everything else your config binds.

## The curated list

Hints come from two places, and most of this document refers to both. The first is the curated list, a small table built into the plugin naming, for each of the main modes, the actions people reach for most, presented in a deliberate order with short hand-written labels. It runs first whatever else is on, so it is what decides the order the bar opens in.

It exists because a faithful list of your keybindings is not automatically a readable one. Left to itself the plugin would show bindings in whatever order Zellij reports them, labeled from action names: `switch_to_mode_pane` reading as "switch to mode pane". The curated list is what makes Normal mode open with `pane  tab  resize  move  scroll  session  quit` instead.

What it covers:

- Normal, Pane, Tab, Resize, Move, Scroll, Search and Session have entries. Other modes (Locked, Tmux, RenameTab and friends) have none, so with [discovery](#discovered-hints) off they show only their escape hatch back to Normal.
- Only bindings you actually have. An entry whose action is not bound in your config is skipped, so unbinding something removes its hint rather than leaving a dead one.
- Grouped concepts that no single action describes, such as the four resize directions as one `resize`, or Session mode's plugin launchers.
- The way out of each mode. Every key bound to `SwitchToMode "Normal"` (usually Enter and Esc both) forms a single `mode_normal` hint, so a mode always shows how to leave it even with discovery off.

Nothing about it is fixed: every entry is a normal hint with an [id](labels.md#ids-and-labels), so it can be relabeled with `label_<id>`, moved with [hint_order](ordering.md#hint-order), or hidden with an empty label.

## Discovered hints

The second source is discovery, which is on unless you turn it off:

```kdl
discover_hints false
```

While it is on, every remaining keybinding enabled in the current mode is found and appended after the curated list. Nothing your config binds is left out: custom binds appear, and so do the many defaults the curated list omits (the `Alt-*` quick keys, `lock` in Normal mode), along with modes like Locked and Tmux that have no curated entries at all.

Keys resolving to the same [id](labels.md#ids-and-labels) become a single hint, so the four directional focus keys collapse into one `focus` rather than four entries.

Hints are collected before anything is drawn, so the two sources merge rather than duplicate: a curated `focus` on `hjkl` and a discovered `focus` on the arrows become one hint carrying all eight keys.

### Which to use

Discovery is on by default because it will not let a binding go unmentioned: the bar becomes a full reference for whatever your config actually does, which is what you want while learning a keymap or one you have just changed. The cost is volume. A mode can easily produce more hints than a status bar has room for, at which point [fitting](fitting.md#fitting-the-bar) starts dropping them, and discovered hints sit at the end of the line, so they are the first to go.

Turn it off for a compact bar. What is left is the curated list on its own: a handful of hints per mode, worded for scanning, naming the things you reach for rather than everything available.

Everything else works the same either way: discovered hints have [ids](labels.md#ids-and-labels) exactly like curated ones, so they relabel, reorder and hide identically.

### Shared bindings

Zellij's default config binds the mode switches (pane, tab, session, and the rest) in every non-locked mode via `shared_except` groups. Discovering those in each mode means every mode re-lists the same globals, which is mostly noise.

`hide_shared_hints` (default `true`) suppresses any discovered binding whose key maps to the same action in the base mode, so each mode shows only what is new in it. It has no effect on the base mode itself, where Normal still lists the globals, and it respects a non-Normal `default_mode` such as `locked`.

Set it to `false` to have every mode list everything it accepts.
