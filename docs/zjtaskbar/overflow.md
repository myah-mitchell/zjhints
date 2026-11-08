# Overflow

When there are more floating panes than the bar has room for, zjtaskbar shows a window of entries around the focused one and replaces the rest with a marker on each side. This page covers how the width is chosen, how the line is shared between the launcher's button, the tray and the entries, how the window is picked, the markers, and what happens to a single entry that is too wide on its own.

## Fitting the bar

```kdl
auto_width true // default: fit to the bar's own pane
max_length 0    // hard cap in columns; 0 = none
```

With `auto_width` on, the bar fits the width of its own pane as Zellij reports it, and fits again when the pane is resized. Until Zellij has reported the width, nothing is cut on a guess.

`max_length` is a hard cap alongside it. With both set, the smaller wins, so an explicit cap is never exceeded on a wide terminal. Set `auto_width false` to ignore the pane and use the cap alone.

## Sharing the line

The line's columns go first to the launcher's button, then to the tray, then to the entries:

1. The button takes its columns at the left end. It is cut only when it alone is wider than the line.
2. The tray takes its columns at the right end, from what the button leaves, less a `tray_spacer` between them. When it does not fit, it drops items from its left end, whole. See [Sharing the line](tray.md#sharing-the-line) in the tray's page.
3. The entries and their markers are fitted into what is left between the two, after an `entry_spacer` following the button and a `tray_spacer` before the tray. When nothing is left, no entry is drawn.

So as the bar narrows, the entries give way first, then the tray's items one at a time, and the button last. The columns between the entries and the tray are filled with the bar's background, so the tray stands flush with the line's right end. While the tray shows anything, a `max_length` wider than the bar's pane is cut at the pane's right edge, so the tray stands there, where you can see it, rather than at `max_length`.

The same four panes, with the mode, the session and the clock in the tray, in bars from 100 down to 15 columns:

![The launcher's button, four floating panes and a tray of the mode, the session and the clock, as flat blocks in bars from 100 down to 15 columns. At 100 columns everything fits. At 64 the entries give way first, down to htop, the focused editor and a marker for the two hidden after them. At 44 the editor is left alone, as its markers no longer fit, and at 30 the entries are gone while the whole tray still fits. At 22 the tray has dropped the mode from its left end, and at 15 the session too, leaving the button and the clock](../img/zjtaskbar/fitting.svg)

With `auto_width` off and no `max_length`, the line has no end to fit against. Every entry is drawn, and the tray follows the entries after a `tray_spacer`. Until Zellij has reported the bar's width, the tray follows the entries as well.

## The window around the focused entry

If every entry fits, every entry is shown. Otherwise the window starts as the focused entry alone, or the first entry while no pane the bar lists has the keyboard, and grows one entry at a time, alternating between the two sides, so the focused entry stays near the middle. When one side has nothing left or cannot take its next entry, the other side keeps growing until neither can.

The window is never broken: the entries shown are always neighbors, so one marker per side can stand for everything hidden there. The focused entry is always shown.

Showing the last hidden entry on a side removes that side's marker, which frees its columns. So when the next entry does not fit but all the rest of the side does once its marker is gone, the whole rest of the side is shown.

A `tab_separator` wider than `entry_spacer` is counted against the entry after it, so the window never picks more than fits. That can leave a column or two unused when that entry ends up first in the window, behind a marker.

The same ten panes in bars of 100, 64 and 44 columns, with the tests pane focused, in `flat` blocks:

![Ten floating panes as flat blocks after the launcher's button, in a bar of 100, 64 and 44 columns. At 100 columns all ten fit. At 64 and 44 the bar keeps the focused tests entry and the entries around it, with a marker on each side counting the entries hidden there](../img/zjtaskbar/overflow-markers.svg)

## Markers

A marker stands for the entries hidden on its side, and `{count}` is how many.

| Option | Marks |
| --- | --- |
| `overflow_left_format` | The entries hidden before the ones shown |
| `overflow_right_format` | The entries hidden after the ones shown |

Left unset, they draw as Zellij's own tab bar does, an arrow and the count in the unselected ribbon color: ` ← +2 ` on the left and ` +3 → ` on the right. A format theme replaces them with markers in its own shape.

Set your own as for any other format, with `{count}`:

```kdl
overflow_left_format  "#[fg=$muted]‹{count} "
overflow_right_format "#[fg=$muted] {count}›"
```

A marker is separated from its neighbors by `entry_spacer`, like an entry. See [Spacing](formats.md#spacing).

With `mouse` on, clicking a marker brings up the hidden entry nearest to it. See [Markers](mouse.md#markers).

## An entry too wide to fit

When even the focused entry with its markers is wider than the room left for the entries, the markers are left out and the focused entry is shown alone. When it is wider than that room on its own, it is cut at the edge and ends in `overflow_str`. A cut never splits a character, so the line can stop a column short.

```kdl
overflow_str "..." // default
```

`overflow_str` is a format string, so it can be styled or a single glyph such as `…`. The default matches zjhints and zjtabs, so the bars cut alike, and a format theme sets its own.

![One focused floating pane titled for everything it is used for, after the launcher's Menu button in a bar of 40 columns. The entry is cut to fit and ends in three dots, then in an ellipsis with overflow_str set to one](../img/zjtaskbar/overflow-lone-entry.svg)

The part of the cut entry still showing can be clicked as usual. The `overflow_str` itself stands for nothing.

## Glyph width

Fitting measures how many columns the entries occupy, which is not the same as counting characters. The bar measures text the way Zellij lays out a plugin's pane, one grapheme cluster at a time: an emoji built from several characters takes two columns, and an East Asian Ambiguous character, such as a Nerd Font glyph, takes one whatever the terminal draws. An entry cut to fit loses a cluster that would cross the edge whole. See [Glyph width](../fitting.md#glyph-width) for the details.
