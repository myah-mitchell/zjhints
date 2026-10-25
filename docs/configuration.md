# Configuration

Every option the plugin reads, with its default. The plugin works with none of them set, so this page is only for changing something.

Options are grouped by topic, and each group names the page carrying the detail behind it. An option written with a placeholder in it is a family rather than a single key: you append your own value.

| Placeholder | You replace it with |
| --- | --- |
| `<id>` | A hint's concept id, giving `label_split_down` |
| `<mode>` | A Zellij mode name, giving `key_format_locked` |
| `<name>` | An alias name you choose, giving `color_blue` |

For a filled-in config rather than a list of options, see the [Zellij config](https://github.com/myah-mitchell/dotfiles/blob/main/zellij/.config/zellij/config.kdl) and the [layout](https://github.com/myah-mitchell/dotfiles/blob/main/zellij/.config/zellij/layouts/default.kdl) that loads it in myah-mitchell's dotfiles.

## Fitting

The hint line is fitted to the terminal as it narrows. See [Fitting the bar](fitting.md).

| Option | Default | What it does |
| --- | --- | --- |
| `auto_width` | `true` | Fit the hints to the bar's own pane, or to the terminal when feeding zjstatus |
| `max_length` | `0` | Hard cap on the line's width in columns, `0` for unlimited |
| `reserve_columns` | `0` | Columns to leave free beside the hints in zjstatus; unused in the bar's own pane |
| `ambiguous_width` | `1` | Columns an East Asian Ambiguous character occupies, `1` or `2` |
| `overflow_str` | `...` | Format string appended when a lone hint is too wide to fit and must be cut mid-hint |
| `hint_precedence` | `tl` | Which pinned group is kept longest, `tl` or `lt` |

## Hints

Which bindings become hints in the first place. See [Hints](hints.md).

| Option | Default | What it does |
| --- | --- | --- |
| `discover_hints` | `true` | Also show every enabled keybinding beyond the curated list |
| `hide_shared_hints` | `true` | Hide bindings inherited from the base mode, so each mode shows only what is new in it; locked mode shows them all |
| `hide_in_base_mode` | `false` | Hide hints in the base mode, whichever mode `default_mode` names |
| `hidden` | `false` | Start with the bar hidden, until a `zjhints::show` or `zjhints::toggle` message; see [Hiding the bar](hiding.md) |

## Keys and ordering

Which keys a hint shows, and what order things appear in. See [Keys and ordering](ordering.md).

| Option | Default | What it does |
| --- | --- | --- |
| `direction_keys` | `both` | Which keys to show when a hint is bound to both `hjkl` and the arrows, `both`, `arrows` or `letters` |
| `key_order` | `qwerty` | How keys within a hint are ordered, `qwerty`, `dvorak`, `colemak`, `abcdef` or `none` |
| `hint_order` | unset | Comma-separated hint ids pinned to the start or end of each mode, around a `*` for everything else |

## Theme

Built-in colors, shapes and symbols for the whole bar, laid under your own options. See [Themes](themes.md).

| Option | Default | What it does |
| --- | --- | --- |
| `color_theme` | `zellij` | The color aliases: `zellij` takes them from the Zellij theme, `custom` adds none, or a built-in palette such as `catppuccin-mocha` |
| `format_theme` | `zellij` | The shape of hints and the mode prefix, `flat`, `plain`, `powerline` or `pills`; `zellij` adds none |
| `alias_theme` | `zellij` | Symbols for modifiers, keys and modes, `ascii`, `unicode` or `nerd-font`; `zellij` adds none |

## Styling

Format strings for each hint, and the fixed strings that stand in for a hint's keys. See [Styling](styling.md).

| Option | Default | What it does |
| --- | --- | --- |
| `key_format` | unset | Format string for the keybinding portion of each hint, theme palette when unset |
| `desc_format` | unset | Format string for the description portion of each hint, theme palette when unset |
| `hint_spacer` | unset | Format string drawn between consecutive hints and after the mode prefix, adjacent when unset |
| `drop_indicator` | unset | Format string marking where hints were dropped, unmarked when unset (`…` when `mouse` is on) |
| `key_format_<id>` | unset | Format string for one hint's keys, overriding `key_format` |
| `desc_format_<id>` | unset | Format string for one hint's description, overriding `desc_format` |
| `keys_<id>` | unset | Replace a hint's keys with a fixed string, as in `keys_go_to_tab "1-9"` |

## Colors

Named colors for format strings. See [Colors](styling.md#colors).

| Option | Default | What it does |
| --- | --- | --- |
| `color_<name>` | unset | Define a color alias referenced as `$name` in any format string |

## Key aliases

Symbols drawn in place of key names. See [Key aliases](styling.md#key-aliases).

| Option | Default | What it does |
| --- | --- | --- |
| `key_alias_<name>` | unset | Replace a key's name with a symbol, as in `key_alias_enter "↵"` |

## Modifier aliases

Symbols drawn in place of modifier names, singly or as a combination. See [Modifier aliases](styling.md#modifier-aliases) and [Chord aliases](styling.md#chord-aliases).

| Option | Default | What it does |
| --- | --- | --- |
| `mod_alias_<name>` | unset | Replace a modifier's name with a symbol, as in `mod_alias_ctrl "^"` |
| `chord_mods_<name>` | unset | The modifier combination for a chord alias, as in `chord_mods_leader "ctrl+alt+super+shift"` |
| `chord_alias_<name>` | unset | The symbol shown instead of that combination, as in `chord_alias_leader "&"` |

## Labels

The words printed next to a hint's keys. See [Labels](labels.md).

| Option | Default | What it does |
| --- | --- | --- |
| `label_<id>` | unset | Override or hide any hint's label, curated or discovered, as in `label_split_down "split ↓"` |
| `label_<mode>_<id>` | unset | The same, scoped to one mode, as in `label_locked_mode_normal "unlock"` |

An empty value hides the hint entirely, at whichever scope it is set.

## Mode prefix

The current input mode, printed before the hints for a bar in its own pane. See [Standing alone](nested-sessions.md#standing-alone).

| Option | Default | What it does |
| --- | --- | --- |
| `show_mode` | `false` | Prefix the hints line with the current input mode |
| `mode_format` | unset | Format string for the mode prefix, with `{desc}`, `{key}` and `{mode}` placeholders |
| `mode_format_<mode>` | unset | The same for one mode only, as in `mode_format_normal`, falling back to `mode_format` |
| `mode_label_<mode>` | unset | The mode's name for `{desc}`, as in `mode_label_locked "LOCK"`, the uppercase mode name when unset |
| `mode_key_<mode>` | unset | A symbol for the mode, drawn by `{key}`, as in `mode_key_locked "🔒"` |

## Nested sessions

How the bar behaves when one Zellij session runs inside another, and when the plugin has a pane of its own. See [Nested sessions](nested-sessions.md).

| Option | Default | What it does |
| --- | --- | --- |
| `hide_when_nested` | `true` | Render nothing while this session is nested inside another |
| `collapse_when_empty` | `true` | Hand this plugin's layout row back to the panes around it while the bar has nothing to draw |
| `dim_when_unfocused` | `true` | Dim hints while this session is not the one receiving input |
| `dim_strength` | `0.5` | How strongly to dim, from `0.0` to `1.0`, quoted |

`dim_strength` has to be quoted. Zellij's plugin-config parser accepts only string, integer and boolean node values, so a bare decimal fails to load.

## Mouse

Clicking the bar. Needs the plugin in its own pane. See [Mouse](mouse.md).

| Option | Default | What it does |
| --- | --- | --- |
| `mouse` | `false` | Right-click for Zellij's context menu, left-click a hint to run it, and click the drop indicator or `overflow_str` for the hints that did not fit |
| `mouse_nested` | `false` | With `mouse` on, also click a descended-into nested session's hints and its "return to host" indicator, which types their keys into that session |

## Integration

| Option | Default | What it does |
| --- | --- | --- |
| `pipe_name` | `zjhints` | Name of the pipe zjstatus reads the hints from |

`pipe_name` matters only when the plugin is piped into zjstatus rather than given a pane of its own. See [Displaying the hints](installation.md#displaying-the-hints).

Left unset, the hints also go out on `zjstatus_hints`, the name this plugin published on before it was renamed, so a zjstatus config written against `{pipe_zjstatus_hints}` keeps rendering untouched. Setting `pipe_name` publishes on that name alone.

## Settings screen

Options of the settings screen itself. They are read only by the instance that runs the screen, not by the bar. See [Settings screen](settings.md).

| Option | Default | What it does |
| --- | --- | --- |
| `settings` | `false` | Run this instance as the settings screen instead of the bar |
| `alias` | unset | The `plugins {}` alias to edit, when more than one loads zjhints |
