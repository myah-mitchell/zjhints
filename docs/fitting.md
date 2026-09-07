# Fitting the bar

The hint line grows with the number of bindings a mode has, and can easily exceed the terminal. By default the plugin fits it to the available width, **dropping whole hints** rather than cutting one in half; you choose which ones go, and they can be replaced by an indicator:

```kdl
auto_width      true // default: fit to the terminal
reserve_columns 0    // columns to leave for the rest of the bar
max_length      0    // hard cap; 0 = none
```

Width is learned from Zellij's pane geometry rather than the plugin's own `render` dimensions, whether it runs headless behind zjstatus's pipe or in its own pane: it measures the right edge of the widest visible tiled pane; floating and suppressed panes are ignored, as neither tracks the terminal's real width. Until the first pane update arrives no width is assumed, so nothing is cut on a guess.

`reserve_columns` is what keeps room for anything sharing the bar. The plugin cannot see zjstatus's `format_right`, so if you have one, reserve roughly its width:

```kdl
format_left  "{pipe_zjstatus_hints}"
format_right "{command_user}@{command_host}:{session}" // ≈30 columns
```

```kdl
reserve_columns 32
```

`max_length` acts as a hard cap alongside this. With both set the smaller wins, so an explicit cap is never exceeded on a wide terminal. Set `auto_width false` to ignore the terminal entirely and use the fixed cap alone.

## Glyph width

Fitting depends on measuring how many **columns** the hints occupy, which is not the same as counting characters: a CJK ideograph or an emoji takes two.

Nerd Font glyphs are East Asian **Ambiguous**: one column by the Unicode standard, but two in a terminal actually set up to display them. If you use them in `key_alias_*` or `mod_alias_*` and the hints overflow slightly (fitting looks right on wide windows and breaks near the edge), set:

```kdl
ambiguous_width 2
```

Getting this wrong under-counts every such glyph, so the plugin believes the line fits while the terminal wraps or clips it.

## What gets dropped

[`hint_order`](ordering.md#hint-order) decides what survives. Unpinned hints (the `*`, the ones you never spoke for) are given up first, starting with the rightmost:

```kdl
hint_order "*, exit" // exit outlives the hints in the "*"
```

So on a narrowing window the middle thins out while `exit` stays put, rather than `exit` being the first thing over the edge.

When the `*` runs out, dropping continues **outward from that same gap**: the leading group is consumed from its inner edge, then the trailing group from its inner edge. Hints given up therefore always form one contiguous run, so a single indicator can stand for all of them:

```
new close split float frames pin exit   ← everything fits
new close split … exit                  ← the "*" thinning out
new … exit                              ← leading group being consumed
… exit                                  ← trailing group is last to go
```

Dropping from the far ends instead would open a second gap and need a second indicator.

A single hint wider than the whole bar survives this and is cut with `overflow_str` as a last resort.

### Which pinned group goes first

`hint_precedence` names the pinned groups in the order they are held onto, reading like zjstatus's `format_precedence`:

```kdl
hint_precedence "tl" // default: trailing kept, leading spent first
hint_precedence "lt" // leading kept, trailing spent first
```

With the default, a hint pinned to the end outlives one pinned to the front:

```kdl
hint_order      "new, *, exit"
hint_precedence "tl"
```

```
new … exit   ← the "*" is gone
… exit       ← "new" spent
exit         ← trailing group is last
```

Reverse it with `"lt"` if the hints you pin to the front are the ones you most want kept. Either way the dropped hints stay one contiguous run, so a single `drop_indicator` still covers them. The `*` is always spent first, whichever precedence is set.

## Marking the gap

`drop_indicator` is rendered in place of the dropped run:

```kdl
drop_indicator "#[fg=$gray]…"
```

It is a format string like `hint_spacer`, so it can be styled, and it is spaced like a hint so it reads as one. It costs columns of its own, which come out of the same budget: a wide indicator forces one more hint to drop rather than pushing the line over the limit.

Left unset, hints are dropped silently.
