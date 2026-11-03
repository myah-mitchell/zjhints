# zjtaskbar tray

The tray is a row of small items at the right end of the zjtaskbar bar, flush with it, the way a desktop taskbar keeps its clock and status icons apart from the windows. This page covers the items, how the tray shares the line with the entries, what a click does, the clock's format and time zone, and the permission the clock asks for. Every option and its default is in the [configuration reference](configuration.md#tray).

## The items

`tray` names the items to draw, left to right. It is empty by default, so the bar draws no tray and asks for no permission for one.

| Item | What it draws | Left click |
| --- | --- | --- |
| `mode` | The current input mode, as zjhints and zjtabs draw their mode prefix | Opens the settings screen |
| `session` | The session's name, as zjtabs draws it | Opens Zellij's session manager |
| `clock` | The time of day | Nothing |

Names are split by spaces or commas and matched in any case. A name zjtaskbar does not know is skipped, as is a second mention of one already listed, so a typo costs only that item.

```kdl
tray "mode session clock"
```

No item is on by default. The session and the mode repeat what the neighboring bars show: zjtabs draws the session's name and the mode, and zjhints can draw the mode, so add them when zjtaskbar is the only bar on screen. The clock needs a permission or an offset you set yourself. See [Turning the clock on](#turning-the-clock-on).

`tray_spacer` is a format string drawn between the tray's items, and between the tray and whatever stands before it. It is one space by default, and an empty string leaves no gap.

## Sharing the line

The bar's columns go first to the launcher's button, then to the tray, then to the entries:

- The tray stands at the right end of the line, and the columns between it and the entries are filled with the bar's background.
- The entries are fitted into the columns the button and the tray leave, so they give way first.
- When the tray does not fit beside the button, it drops items from its left end, whole, until the rest fit. An item is never cut, so the item listed last is the one kept longest. In `mode session clock`, that is the clock.
- The entries then take whatever columns the dropped items freed.
- The tray dims with the entries under `dim_when_unfocused`.

With `auto_width` off and no `max_length`, or before Zellij has reported the bar's width, the line has no right end to stand against, so the tray follows the entries after `tray_spacer`. With `auto_width` off and a `max_length` wider than the bar's pane, the tray stands at the pane's right edge, where you can see it, rather than at `max_length`.

## Clicking

With `mouse` on, a left click on an item does what the table under [The items](#the-items) says. A right click on any item opens Zellij's menu for the bar, as a right click anywhere off an entry does.

## Turning the clock on

Add `clock` to `tray`:

```kdl
tray "clock"
```

The clock then asks Zellij for *Run commands*, to run `date +%z` for the local time's offset from UTC. To turn it on without that permission, set the offset yourself as well:

```kdl
tray "clock"
clock_utc_offset "+01:00"
```

A fixed offset does not follow daylight saving time. See [The time zone](#the-time-zone) and [Permissions](#permissions).

## Clock

`clock_format` writes the time with `strftime` specifiers. It is `%H:%M` by default, such as `09:41`.

| Specifier | What it draws |
| --- | --- |
| `%H` | Hour of the day, `00` to `23` |
| `%I` | Hour on a 12-hour clock, `01` to `12` |
| `%p` | `AM` or `PM` |
| `%M` | Minute, `00` to `59` |
| `%S` | Second, `00` to `59` |
| `%Y` | Year, such as `2026` |
| `%y` | Year within its century, `00` to `99` |
| `%m` | Month, `01` to `12` |
| `%d` | Day of the month, `01` to `31` |
| `%e` | Day of the month with a leading space, ` 1` to `31` |
| `%j` | Day of the year, `001` to `366` |
| `%a` | Short weekday name, such as `Tue` |
| `%A` | Full weekday name, such as `Tuesday` |
| `%b` | Short month name, such as `Nov` |
| `%B` | Full month name, such as `November` |
| `%z` | Offset from UTC, such as `+0200` |
| `%%` | A literal `%` |

Names are in English. Any other letter after a `%`, or a `%` at the end, is drawn as typed. This example draws `Tue 03 Nov 09:41 AM`:

```kdl
clock_format "%a %d %b %I:%M %p"
```

`clock_item_format` is the format string the time is drawn in, with `{clock}` for the text `clock_format` writes. The time is filled in after the format is read, so a `#[` or a `{` in `clock_format` is drawn as typed. With no format set, the time is drawn on the Zellij theme's unselected ribbon, as an entry is.

```kdl
clock_item_format "#[fg=$accent,bold] {clock} "
```

The clock redraws when the minute changes, or each second while `clock_format` has `%S` in it. A format with neither still checks each minute, so a date changes within a minute of midnight. The bar redraws only when the drawn text changes.

## The time zone

Zellij gives plugins the time only in UTC, so the clock needs the local time's offset from UTC. Set it yourself with `clock_utc_offset`, or leave it unset for the clock to ask the system.

`clock_utc_offset` takes a sign and then hours, hours and minutes, or hours and minutes split by a colon. Examples are `+02`, `-0530` and `+05:30`. A value that does not parse is ignored, and the clock keeps UTC. A fixed offset does not follow daylight saving time, so the system's answer suits most setups better.

```kdl
clock_utc_offset "+01:00"
```

While `clock_utc_offset` is unset, the clock works out the offset like this:

1. The bar asks Zellij for *Run commands* along with its other permissions.
2. Once that is granted, the bar runs `date +%z`, which prints the system's offset, such as `+0200`.
3. The clock draws the time at that offset.
4. On the first tick of each new hour, the bar runs `date +%z` again, so a change of daylight saving time shows within the hour.

The clock draws nothing until the first answer comes, rather than drawing UTC for a moment and then jumping. If the permission is denied, or an answer does not parse before any has, the clock draws UTC. An answer that does not parse after a good one keeps the last good offset.

## Session and mode

The `session` item draws the session's name with the options zjtabs uses: `show_session`, `session_format` and its `{session}` placeholder. While `show_session` is unset, Zellij's `hide_session_name` hides the name, as on zjtabs, and a name Zellij hides still shows while you rename the session. The name is filled in after the format is read, so a session named with a `#[` or a `{` in it is drawn as typed.

The `mode` item draws the input mode with the options zjhints and zjtabs use, which are listed under [Mode](configuration.md#mode). There is no `show_mode`; leave `mode` out of `tray` instead. See [Mode prefix](../zjtabs/formats.md#mode-prefix) for the placeholders and examples.

With no format set, each is a bold block in the colors of Zellij's selected ribbon.

## Themes

A `format_theme` gives every item a look in the shape of the entries. The session's name and the mode are drawn as zjtabs draws its session's name: the mirror image of a mode prefix, opening toward the entries, since the whole tray stands at the right end and faces them.

| Theme | Clock | Session and mode |
| --- | --- | --- |
| `flat` | A block on `$surface` | A bold block on the item's color |
| `plain` | The time alone, in `$fg` | The name alone, bold, in the item's color |
| `powerline` | An arrow pointing left into a block on `$surface` | An arrow pointing left into a bold block on the item's color |
| `pills` | A half-pill on `$surface`, open to the right | A half-pill on `$surface`, open to the right, with the name in the item's color |

The item's color is the mode's own color for the mode, such as `$red` for locked mode, and `$accent` for the session's name, which does not change with the mode.

An `alias_theme` gives each mode a symbol for `{key}`. The mode's block then holds the symbol before the name, and in `pills` the symbol gets a half-pill of its own in the mode's color. See [Themes](../themes.md).

## Permissions

The tray asks Zellij for one permission beyond those of the bar and the mouse: *Run commands*, only while `clock` is in `tray` and `clock_utc_offset` is unset. The default tray is empty, so it asks for nothing. The clicks need nothing beyond what `mouse` asks for.

Set `clock_utc_offset` if you would rather not grant *Run commands*. The bar then runs no command for the clock and does not ask for the permission, though a launcher entry with a `command` still does. The cost is daylight saving time, which a fixed offset does not follow, so you change the offset yourself when the clocks change. Opening the [settings screen](../settings.md#permissions) asks for *Run commands* either way, because it asks for everything a bar may use.
