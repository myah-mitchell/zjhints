# zjtabs configuration

Every option zjtabs reads, with its default. The plugin works with none of them set, so this page is only for changing something.

Options are grouped by topic, and each group names the page carrying the detail behind it. An option written with a placeholder in it is a family rather than a single key: you append your own value.

| Placeholder | You replace it with |
| --- | --- |
| `<mode>` | A Zellij mode name, giving `mode_format_locked` |
| `<name>` | An alias name you choose, giving `color_blue` |

Format strings use the zjstatus syntax, with the placeholders each option names below. See [Styling](../styling.md).

## Tabs

How each tab is drawn. See [Tab formats and flags](formats.md).

| Option | Default | What it does |
| --- | --- | --- |
| `tab_format` | unset | Format string for each tab but the active one, with `{index}`, `{name}` and `{flags}`; Zellij's ribbon colors when unset |
| `active_tab_format` | unset | Format string for the active tab, with the same placeholders; Zellij's ribbon colors when unset |
| `tab_spacer` | ` ` | Format string drawn between tabs and markers, one space by default, empty for no gap |
| `index_from_zero` | `false` | Number tabs from 0 in `{index}`, rather than from 1 |
| `rename_placeholder` | `Enter name...` | Name drawn for the tab being renamed while its new name is still empty |

## Flags

The indicators `{flags}` draws after a tab's name. Each is a format string drawn only for a tab in that state, and an empty one hides it. See [Flags](formats.md#flags).

| Option | Default | What it does |
| --- | --- | --- |
| `fullscreen_format` | ` (FULLSCREEN)` | Flag of a tab with a fullscreen pane |
| `sync_format` | ` (SYNC)` | Flag of a tab whose panes all take the same input |
| `floating_format` | ` (FLOATING)` | Flag of a tab showing its floating panes |
| `bell_format` | ` ●` | Flag of a tab that rang the bell or needs you |

## Mode prefix

The current input mode, drawn before the tabs. The options are named and read as for zjhints. See [Standing alone](../nested-sessions.md#standing-alone).

| Option | Default | What it does |
| --- | --- | --- |
| `show_mode` | `true` | Draw the current input mode before the tabs |
| `mode_format` | unset | Format string for the mode prefix, with `{desc}`, `{key}` and `{mode}` placeholders; a block in the selected ribbon's colors when unset |
| `mode_format_<mode>` | unset | The same for one mode only, as in `mode_format_normal`, falling back to `mode_format` |
| `mode_label_<mode>` | unset | The mode's name for `{desc}`, as in `mode_label_locked "LOCK"`, the uppercase mode name when unset |
| `mode_key_<mode>` | unset | A symbol for the mode, drawn by `{key}`, as in `mode_key_locked "🔒"` |

## Overflow

Fitting the tabs to the bar when there are more than fit, and marking the ones left out. See [Overflow](overflow.md).

| Option | Default | What it does |
| --- | --- | --- |
| `auto_width` | `true` | Fit the tabs to the bar's own pane, keeping the active tab on screen |
| `max_length` | `0` | Hard cap on the line's width in columns, `0` for unlimited |
| `ambiguous_width` | `1` | Columns an East Asian Ambiguous character occupies, `1` or `2` |
| `overflow_left_format` | unset | Format string for the marker of tabs hidden before the shown ones, with `{count}`; Zellij's ribbon colors when unset |
| `overflow_right_format` | unset | Format string for the marker of tabs hidden after the shown ones, with `{count}`; Zellij's ribbon colors when unset |
| `overflow_str` | `...` | Format string appended when a lone tab is too wide to fit and must be cut mid-tab |

## Theme

Built-in colors, shapes and symbols for the whole bar, laid under your own options. See [Themes](../themes.md) and [Themes for tabs](formats.md#themes).

| Option | Default | What it does |
| --- | --- | --- |
| `color_theme` | `zellij` | The color aliases: `zellij` takes them from the Zellij theme, `custom` adds none, or a built-in palette such as `catppuccin-mocha` |
| `format_theme` | `zellij` | The shape of tabs, markers and the mode prefix, `flat`, `plain`, `powerline` or `pills`; `zellij` adds none |
| `alias_theme` | `zellij` | Symbols for the flags and the modes, `ascii`, `unicode` or `nerd-font`; `zellij` adds none |

## Colors

Named colors for format strings. See [Colors](../styling.md#colors).

| Option | Default | What it does |
| --- | --- | --- |
| `color_<name>` | unset | Define a color alias referenced as `$name` in any format string |

## Behavior

Whether the bar shows at all, and how it behaves beside other sessions. See [Hiding the bar](../hiding.md) and [Nested sessions](../nested-sessions.md).

| Option | Default | What it does |
| --- | --- | --- |
| `hidden` | `false` | Start with the bar hidden, until a `zjtabs::show` or `zjtabs::toggle` message; see [Hiding the bar](../hiding.md) |
| `hide_when_nested` | `true` | Render nothing while this session is nested inside another |
| `collapse_when_empty` | `true` | Hand this plugin's layout row back to the panes around it while the bar has nothing to draw |
| `dim_when_unfocused` | `true` | Dim the tabs while this session is not the one receiving input |
| `dim_strength` | `0.5` | How strongly to dim, from `0.0` to `1.0`, quoted |

`dim_strength` has to be quoted. Zellij's plugin-config parser accepts only string, integer and boolean node values, so a bare decimal fails to load.

## Mouse

Clicking the bar. See [Mouse](mouse.md).

| Option | Default | What it does |
| --- | --- | --- |
| `mouse` | `true` | Click a tab to switch to it, scroll to step through tabs, double-click to rename, middle-click to close, right-click for Zellij's menus |
| `mouse_wheel_reversed` | `false` | Scroll up for the previous tab and down for the next, instead of Zellij's own way round |
| `mouse_close_confirm` | `true` | Ask before a middle click closes a tab |

## Settings screen

Options of the settings screen itself. They are read only by the instance that runs the screen, not by the bar. See [Settings screen](../settings.md).

| Option | Default | What it does |
| --- | --- | --- |
| `settings` | `false` | Run this instance as the settings screen instead of the bar |
| `alias` | unset | The `plugins {}` alias to edit, when more than one loads zjtabs |
