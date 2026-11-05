# zjtaskbar configuration

Every option zjtaskbar reads, with its default. The plugin works with none of them set, so this page is only for changing something.

Options are grouped by topic, and a group with more to it links the page carrying the detail. An option written with a placeholder in it is a family rather than a single key: you append your own value.

| Placeholder | You replace it with |
| --- | --- |
| `<name>` | An alias name you choose, giving `color_blue` |
| `<n>` | A launcher entry's number, a whole number with no leading zeros, giving `launch_1_name` |
| `<key>` | A configuration key of the entry's plugin, giving `launch_2_config_welcome_screen` |
| `<mode>` | A Zellij mode name, giving `mode_format_locked` |

Format strings use the zjstatus syntax, with the placeholders each option names below. See [Styling](../styling.md).

## Entries

Which floating panes the bar lists, and how each is drawn. The entries are grouped by tab in the order the tabs stand, and ordered by pane id within a tab. See [Entry formats and flags](formats.md) for examples. An entry's format takes these placeholders:

| Placeholder | What it draws |
| --- | --- |
| `{title}` | The pane's title, drawn as typed |
| `{index}` | The entry's place on the bar, counted from 1 |
| `{tab}` | The name of the pane's tab, or its number while it has no name |
| `{flags}` | The pane's flags, from the options under [Flags](#flags) |

| Option | Default | What it does |
| --- | --- | --- |
| `pane_scope` | `all` | List the floating panes of every tab, `all`, or of the active tab only, `tab` |
| `entry_format` | unset | Format string for each entry neither focused nor minimized; Zellij's ribbon colors when unset |
| `focused_entry_format` | unset | Format string for the entry of the pane the keyboard goes to; Zellij's selected ribbon colors when unset |
| `minimized_entry_format` | unset | Format string for the entry of a minimized pane; Zellij's text colors when unset |
| `entry_spacer` | ` ` | Format string drawn between entries of one tab and beside the markers, one space by default, empty for no gap |
| `tab_separator` | ` │ ` | Format string drawn between the last entry of one tab and the first of the next, in place of the spacer |
| `empty_format` | unset | Format string drawn while there are no floating panes; nothing when unset |

## Flags

The indicators `{flags}` draws after an entry's title. Each is a format string drawn only for a pane in that state, and an empty one hides it. See [Flags](formats.md#flags).

| Option | Default | What it does |
| --- | --- | --- |
| `minimized_format` | ` (MINIMIZED)` | Flag of a pane hidden from the screen and kept on the bar |
| `exited_format` | ` (EXITED)` | Flag of a pane whose command has finished, which Zellij holds open |
| `plugin_format` | ` (PLUGIN)` | Flag of a pane that runs a plugin rather than a terminal |
| `other_tab_format` | ` ({tab})` | Flag of a pane in a tab other than the active one, with `{tab}` for that tab's name |

## Overflow

Fitting the entries to the bar when there are more than fit, and marking the ones left out. The bar keeps a window of entries around the focused one, or the first while none is focused. See [Overflow](overflow.md).

| Option | Default | What it does |
| --- | --- | --- |
| `auto_width` | `true` | Fit the entries to the bar's own pane, keeping the focused entry on screen |
| `max_length` | `0` | Hard cap on the line's width in columns, `0` for unlimited |
| `ambiguous_width` | `1` | Columns an East Asian Ambiguous character occupies, `1` or `2` |
| `overflow_left_format` | unset | Format string for the marker of entries hidden before the shown ones, with `{count}`; Zellij's ribbon colors when unset |
| `overflow_right_format` | unset | Format string for the marker of entries hidden after the shown ones, with `{count}`; Zellij's ribbon colors when unset |
| `overflow_str` | `...` | Format string appended when a lone entry is too wide to fit and must be cut |

## Launcher

A button at the bar's left end, drawn while `mouse` is on. Left-click it for a menu of new floating panes to open: a new terminal, your own entries, then Zellij's session manager, settings and plugin manager. Right-click it for Zellij's bar menu. See [Launcher](launcher.md) for the entries and an example.

The button's format takes one placeholder:

| Placeholder | What it draws |
| --- | --- |
| `{symbol}` | The text of `launcher_symbol` |

| Option | Default | What it does |
| --- | --- | --- |
| `show_launcher` | `true` | Draw the launcher's button while `mouse` is on and its menu lists something |
| `launcher_format` | unset | Format string for the button, with `{symbol}`; Zellij's ribbon colors when unset |
| `launcher_symbol` | `Menu` | What `{symbol}` draws; an `alias_theme` sets a symbol in its place |
| `launcher_new_terminal` | `true` | List a new terminal first in the menu, started in the focused pane's directory |
| `launcher_zellij_actions` | `true` | List Zellij's session manager, settings and plugin manager last in the menu |
| `launch_<n>_name` | unset | What the menu shows for entry `<n>`; entries are listed in the order of `<n>` |
| `launch_<n>_command` | unset | The program entry `<n>` runs in a new floating pane |
| `launch_<n>_args` | unset | The arguments of entry `<n>`'s command in one string, split as a shell splits them |
| `launch_<n>_cwd` | unset | The directory entry `<n>`'s command runs in; the focused pane's when unset |
| `launch_<n>_plugin` | unset | The plugin URL or alias entry `<n>` opens in a new floating pane |
| `launch_<n>_config_<key>` | unset | One key of the configuration entry `<n>`'s plugin opens with |

The button asks Zellij for no permission beyond those of `mouse`, apart from one: an entry with `launch_<n>_command` asks for *Run commands*, and only while there is one.

## Tray

A row of items at the bar's right end, flush with it, the way a desktop taskbar keeps its clock apart from the windows. The tray is empty by default, so it draws nothing and asks for no permission until you list an item. The entries give way to the tray, and the tray gives way to the launcher's button. See [Tray](tray.md) for the items, the fitting and the clicks.

| Option | Default | What it does |
| --- | --- | --- |
| `tray` | unset | The items to draw, left to right, from `mode`, `session` and `clock`, split by spaces or commas; no tray when unset or empty |
| `tray_spacer` | ` ` | Format string drawn between the tray's items and between the tray and what stands before it, empty for no gap |

## Clock

The time of day, drawn while `clock` is in `tray`. The clock's item format takes one placeholder:

| Placeholder | What it draws |
| --- | --- |
| `{clock}` | The time, as `clock_format` writes it |

| Option | Default | What it does |
| --- | --- | --- |
| `clock_format` | `%H:%M` | The text of the time, with `strftime` specifiers such as `%H`, `%M` and `%a`; see [Clock](tray.md#clock) for the full list |
| `clock_item_format` | unset | Format string for the clock, with `{clock}`; Zellij's ribbon colors when unset |
| `clock_utc_offset` | unset | The local time's offset from UTC, such as `+02:00` or `-0530`; asked of the system when unset |

While `clock_utc_offset` is unset, the clock asks Zellij for *Run commands*, to run `date +%z`. See [The time zone](tray.md#the-time-zone).

## Session

The session's name, drawn while `session` is in `tray`. The options are named and read as for zjtabs.

| Option | Default | What it does |
| --- | --- | --- |
| `show_session` | `true` | Draw the session's name; while unset, Zellij's `hide_session_name` hides it |
| `session_format` | unset | Format string for the session's name, with a `{session}` placeholder; a block in the selected ribbon's colors when unset |

## Mode

The current input mode, drawn while `mode` is in `tray`. The options are named and read as for zjhints and zjtabs. See [Standing alone](../nested-sessions.md#standing-alone) for the mode names.

| Option | Default | What it does |
| --- | --- | --- |
| `mode_format` | unset | Format string for the mode, with `{desc}`, `{key}` and `{mode}` placeholders; a block in the selected ribbon's colors when unset |
| `mode_format_<mode>` | unset | The same for one mode only, as in `mode_format_normal`, falling back to `mode_format` |
| `mode_label_<mode>` | unset | The mode's name for `{desc}`, as in `mode_label_locked "LOCK"`, the uppercase mode name when unset |
| `mode_key_<mode>` | unset | A symbol for the mode, drawn by `{key}`, as in `mode_key_locked "L"` |

## Frame

Buttons on the frames of every pane, as a desktop window has: minimize, maximize and close, at the right of the frame's top row. They need the Zellij build with plugin frame items; see [Frame buttons](mouse.md#frame-buttons).

| Option | Default | What it does |
| --- | --- | --- |
| `frame_buttons` | `true` | Put minimize, maximize and close buttons on the frames of every pane, minimize floating a tiled pane |
| `frame_minimize_text` | ` [─]` | Text of the minimize button, which an empty value leaves off |
| `frame_maximize_text` | ` [□]` | Text of the maximize button, which an empty value leaves off |
| `frame_restore_text` | ` [❐]` | Text of the maximize button while its pane fills the tab, a click giving it back its own size |
| `frame_close_text` | ` [✕] ` | Text of the close button, which an empty value leaves off |

Zellij draws a plugin's buttons side by side with nothing between them, so each default starts with a space. The close button asks first while `mouse_close_confirm` is on, which takes the *Start new terminals and plugins* permission even with `mouse` off.

## Theme

Built-in colors, shapes and symbols for the whole bar, laid under your own options. See [Themes](../themes.md).

| Option | Default | What it does |
| --- | --- | --- |
| `color_theme` | `zellij` | The color aliases: `zellij` takes them from the Zellij theme, `custom` adds none, or a built-in palette such as `catppuccin-mocha` |
| `format_theme` | `zellij` | The shape of entries, markers, separators, the launcher's button and the tray's items, `flat`, `plain`, `powerline` or `pills`; `zellij` adds none |
| `alias_theme` | `zellij` | Symbols for the flags, the launcher's button and the modes, `ascii`, `unicode` or `nerd-font`; `zellij` adds none |

## Colors

Named colors for format strings. See [Colors](../styling.md#colors).

| Option | Default | What it does |
| --- | --- | --- |
| `color_<name>` | unset | Define a color alias referenced as `$name` in any format string |

## Behavior

Whether the bar shows at all, and how it behaves beside other sessions. See [Hiding the bar](../hiding.md) and [Nested sessions](../nested-sessions.md).

| Option | Default | What it does |
| --- | --- | --- |
| `hidden` | `false` | Start with the bar hidden, until a `zjtaskbar::show` or `zjtaskbar::toggle` message; see [Hiding the bar](../hiding.md) |
| `hide_when_nested` | `true` | Render nothing while this session is nested inside another |
| `collapse_when_empty` | `true` | Hand this plugin's layout row back to the panes around it while the bar has nothing to draw |
| `dim_when_unfocused` | `true` | Dim the entries while this session is not the one receiving input |
| `dim_strength` | `0.5` | How strongly to dim, from `0.0` to `1.0`, quoted |

`dim_strength` has to be quoted. Zellij's plugin-config parser accepts only string, integer and boolean node values, so a bare decimal fails to load.

## Mouse

Clicking the bar, the way a desktop taskbar works. Left-click an entry to focus its pane, pulling it into the tab on screen from another tab, or to restore a minimized pane where it stood. Left-click the focused entry to minimize its pane, and a marker to act on the hidden entry nearest to it. See [Mouse](mouse.md) for every click.

| Option | Default | What it does |
| --- | --- | --- |
| `mouse` | `true` | Click an entry to focus, minimize or restore its pane, scroll to step through the panes on screen, double-click to rename, middle-click to close, right-click for the pane's menu or Zellij's bar menu |
| `mouse_wheel_reversed` | `false` | Scroll up for the previous pane and down for the next, the reverse of Zellij's own tab bar |
| `mouse_close_confirm` | `true` | Ask before a middle click, the pane menu or a frame's close button closes a pane, with *Cancel* selected |

Turning `mouse` on asks Zellij for two more permissions: *Execute actions as the user*, to rename a pane, and *Start new terminals and plugins*, for the pane menu and the close prompt, which are Zellij prompts. See [Permissions](mouse.md#permissions).

## Settings screen

Options of the settings screen itself. They are read only by the instance that runs the screen, not by the bar. See [Settings screen](../settings.md).

| Option | Default | What it does |
| --- | --- | --- |
| `settings` | `false` | Run this instance as the settings screen instead of the bar |
| `alias` | unset | The `plugins {}` alias to edit, when more than one loads zjtaskbar |
