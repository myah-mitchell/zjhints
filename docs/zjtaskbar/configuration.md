# zjtaskbar configuration

Every option zjtaskbar reads, with its default. The plugin works with none of them set, so this page is only for changing something.

Options are grouped by topic, and a group with more to it links the page carrying the detail. An option written with a placeholder in it is a family rather than a single key: you append your own value.

| Placeholder | You replace it with |
| --- | --- |
| `<name>` | An alias name you choose, giving `color_blue` |

Format strings use the zjstatus syntax, with the placeholders each option names below. See [Styling](../styling.md).

## Entries

Which floating panes the bar lists, and how each is drawn. The entries are grouped by tab in the order the tabs stand, and ordered by pane id within a tab. An entry's format takes these placeholders:

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

The indicators `{flags}` draws after an entry's title. Each is a format string drawn only for a pane in that state, and an empty one hides it.

| Option | Default | What it does |
| --- | --- | --- |
| `minimized_format` | ` (MINIMIZED)` | Flag of a pane hidden from the screen and kept on the bar |
| `exited_format` | ` (EXITED)` | Flag of a pane whose command has finished, which Zellij holds open |
| `plugin_format` | ` (PLUGIN)` | Flag of a pane that runs a plugin rather than a terminal |
| `other_tab_format` | ` ({tab})` | Flag of a pane in a tab other than the active one, with `{tab}` for that tab's name |

## Overflow

Fitting the entries to the bar when there are more than fit, and marking the ones left out. The bar keeps a window of entries around the focused one, or the first while none is focused.

| Option | Default | What it does |
| --- | --- | --- |
| `auto_width` | `true` | Fit the entries to the bar's own pane, keeping the focused entry on screen |
| `max_length` | `0` | Hard cap on the line's width in columns, `0` for unlimited |
| `ambiguous_width` | `1` | Columns an East Asian Ambiguous character occupies, `1` or `2` |
| `overflow_left_format` | unset | Format string for the marker of entries hidden before the shown ones, with `{count}`; Zellij's ribbon colors when unset |
| `overflow_right_format` | unset | Format string for the marker of entries hidden after the shown ones, with `{count}`; Zellij's ribbon colors when unset |
| `overflow_str` | `...` | Format string appended when a lone entry is too wide to fit and must be cut |

## Theme

Built-in colors, shapes and symbols for the whole bar, laid under your own options. See [Themes](../themes.md).

| Option | Default | What it does |
| --- | --- | --- |
| `color_theme` | `zellij` | The color aliases: `zellij` takes them from the Zellij theme, `custom` adds none, or a built-in palette such as `catppuccin-mocha` |
| `format_theme` | `zellij` | The shape of entries, markers and separators, `flat`, `plain`, `powerline` or `pills`; `zellij` adds none |
| `alias_theme` | `zellij` | Symbols for the flags, `ascii`, `unicode` or `nerd-font`; `zellij` adds none |

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

Clicking the bar, the way a desktop taskbar works. Left-click an entry to focus its pane, switching to its tab, or to restore a minimized pane where it stood. Left-click the focused entry to minimize its pane, and a marker to act on the hidden entry nearest to it.

| Option | Default | What it does |
| --- | --- | --- |
| `mouse` | `true` | Click an entry to focus, minimize or restore its pane, scroll to step through the panes on screen, double-click to rename, middle-click to close, right-click for the pane's menu or Zellij's bar menu |
| `mouse_wheel_reversed` | `false` | Scroll up for the previous pane and down for the next, the reverse of Zellij's own tab bar |
| `mouse_close_confirm` | `true` | Ask before a middle click or the pane menu closes a pane, with *Cancel* selected |

Turning `mouse` on asks Zellij for two more permissions: *Execute actions as the user*, to rename a pane, and *Start new terminals and plugins*, for the pane menu and the close prompt, which are Zellij prompts.

## Settings screen

Options of the settings screen itself. They are read only by the instance that runs the screen, not by the bar. See [Settings screen](../settings.md).

| Option | Default | What it does |
| --- | --- | --- |
| `settings` | `false` | Run this instance as the settings screen instead of the bar |
| `alias` | unset | The `plugins {}` alias to edit, when more than one loads zjtaskbar |
