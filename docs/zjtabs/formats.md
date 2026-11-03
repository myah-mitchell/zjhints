# Tab formats and flags

Each tab is drawn from a format string, and the states worth noticing from the bar, such as a bell, are flags drawn from small formats of their own. This page covers the placeholders, the flags, the gap between tabs, numbering, renaming and the mode prefix, for anyone restyling the tabs.

With nothing set, every tab is a ribbon in the Zellij theme's colors, the way Zellij's own tab bar draws it: its number, its name and its flags, with the active tab in the selected ribbon color. The sections below change that one part at a time, or all of it with a [theme](#themes).

## Placeholders

`tab_format` styles every tab but the active one, and `active_tab_format` the active one. Both use the format-string syntax: `#[...]` blocks set the style of what follows, and `$name` refers to a color alias. See [Styling](../styling.md) and [Colors](../styling.md#colors).

| Placeholder | Filled with |
| --- | --- |
| `{index}` | The tab's number, counted from its place in the bar |
| `{name}` | The tab's name |
| `{flags}` | The tab's [flags](#flags) |

```kdl
color_theme "catppuccin-mocha"
tab_format        "#[fg=$fg,bg=$surface] {index} {name}{flags} "
active_tab_format "#[fg=$bg,bg=$accent,bold] {index} {name}{flags} "
```

A format that leaves a placeholder out leaves that part off the tab. A format set to an empty string counts as unset, so the tab is drawn in Zellij's ribbon colors again.

### Names are drawn as text

The name is put into the format after the format has been read, so nothing in a name is taken for styling. A tab named `#[fg=red]build` shows exactly that, and a `$` or a `{` in a name is drawn as typed.

A control character in a name is drawn as an escape, such as `\u{1b}` for ESC, rather than sent to the terminal.

## Numbering

Tabs are numbered from 1, as Zellij numbers them everywhere else, from the `GoToTab` keybinding to the names it gives new tabs. `{index}` is the tab's place in the bar, so it changes when you move a tab.

To count from 0 instead, set:

```kdl
index_from_zero true
```

## Flags

`{flags}` draws one indicator for each state a tab is in, in the order of this table, straight after the name.

| Option | Shown for a tab that | Default |
| --- | --- | --- |
| `fullscreen_format` | Has a pane filling the whole tab | ` (FULLSCREEN)` |
| `sync_format` | Sends what is typed to every pane of the tab | ` (SYNC)` |
| `floating_format` | Shows its floating panes over its tiled ones | ` (FLOATING)` |
| `bell_format` | Has a pane that rang the bell or has something that needs you | ` ●` |

The bell flag also appears for a tab with a program waiting on you, failed or finished, unless you muted that tab's attention in Zellij. A program that is still working does not count. Zellij reports a bell only for a tab that is not active.

Each option is a format string without placeholders. Write the space before the flag yourself, as the defaults do, so the flag does not touch the name. An empty value hides that flag:

```kdl
sync_format ""
```

A flag is drawn in the style of the tab around it, so a symbol sits on the tab's own background. A `#[...]` block in the flag changes only what it sets, such as the color or bold, and the tab's background stays under it:

```kdl
bell_format "#[fg=$red,bold] ●"
```

![Five tabs with flat blocks after the NORMAL mode prefix, three times. The default flag formats show (FULLSCREEN), (SYNC) and a dot after the names. With sync_format empty the logs tab shows no flag. With a styled bell_format the server tab's dot is red](../img/zjtabs/flags-formats.svg)

An alias theme swaps all four flags for symbols at once. See [Themes](#themes).

## Spacing

`tab_spacer` is a format string drawn between neighboring tabs, and between a tab and an [overflow marker](overflow.md#markers). It is one space by default, which keeps ribbons in Zellij's colors from running together. Set it to an empty string for no gap, or to a styled divider:

```kdl
tab_spacer "#[fg=$muted] │ "
```

## Renaming

While you rename a tab, Zellij clears its name and updates it as you type, so the tab shows the new name as it forms. Until the first character, the bar draws `rename_placeholder` in place of the empty name, so the tab does not shrink to nothing. It reads `Enter name...` by default, the text Zellij's own bar shows:

```kdl
rename_placeholder "new name"
```

The placeholder is plain text and appears only on the tab being renamed. Double-clicking a tab starts a rename; see [Mouse](mouse.md#renaming).

## Mode prefix

The bar draws the current input mode at its left end, before the tabs. It is on by default. zjhints' own `show_mode` is off by default, so with both bars at their defaults the mode shows once, on the tab bar. Turn it off when something else on screen shows the mode already, such as zjstatus or a zjhints bar with `show_mode` on:

```kdl
show_mode false
```

The prefix takes the same options as the zjhints mode prefix, with the same names and meanings: `mode_format` for every mode, `mode_format_<mode>` for one, and `mode_label_<mode>` and `mode_key_<mode>` for the name and symbol drawn as `{desc}` and `{key}`. The mode names, such as `rename_tab` for the mode you rename a tab in, are listed in [Standing alone](../nested-sessions.md#standing-alone), and one format for every mode is described in [One format for every mode](../nested-sessions.md#one-format-for-every-mode).

![Four zjtabs bars with the input mode before the tabs. In the Zellij theme's colors a NORMAL block leads the ribbons; with the pills theme a half-pill reading NORMAL in green leads the pills, and one reading LOCKED in red in locked mode; with the powerline theme and unicode symbols a green block holds a keyboard symbol and NORMAL, ending in an arrow](../img/zjtabs/mode-prefix.svg)

With no format set, the prefix is a bold block in the colors of Zellij's selected ribbon. A format theme draws it in the shape of its tabs, and an alias theme gives each mode a symbol. See [Themes](#themes).

The prefix sits on the line like this:

- `tab_spacer` separates it from the first tab or the left [overflow marker](overflow.md#markers).
- The tabs are fitted into the columns the prefix leaves. When the prefix alone fills the bar, no tabs are drawn, and a prefix wider than the bar is cut and ends in `overflow_str`.
- It dims with the tabs under `dim_when_unfocused`.
- A click on it does nothing, and a right-click opens Zellij's menu for the bar, as a right-click past the tabs does.

## Themes

The three theme options can each be picked on its own, and take the same values as for zjhints. See [Themes](../themes.md). For zjtabs they set:

| Option | What it sets for tabs |
| --- | --- |
| `color_theme` | The color aliases, `$accent` and the rest, exactly as for zjhints |
| `format_theme` | `tab_format`, `active_tab_format`, `tab_spacer`, both overflow markers, `overflow_str`, and the [mode prefix](#mode-prefix) formats |
| `alias_theme` | The four flag options, as the symbols below, and a `mode_key_<mode>` symbol for each mode |

Your own options always win over a theme, one option at a time.

The same five tabs in each format theme, in `catppuccin-mocha` colors:

![Five rows, each a zjtabs bar of the NORMAL mode prefix and five tabs under the name of its format theme: zellij, flat, plain, powerline and pills. The zellij row uses the Zellij palette; flat shows square blocks; plain shows colored text without blocks; powerline joins the blocks with arrows; pills rounds both ends of each tab](../img/zjtabs/themes-shapes.svg)

| Value | Shape |
| --- | --- |
| `zellij` | Nothing set: ribbons in the Zellij theme's colors |
| `flat` | The index and name on a `$surface` block, the active tab on an `$accent` block in bold |
| `plain` | Colored text with no blocks, two spaces apart: `$muted` for a tab, bold `$accent` for the active one |
| `powerline` | The flat blocks, each ending in a Powerline arrow |
| `pills` | Each tab a rounded pill |

The symbols an alias theme gives the flags:

| Flag | `zellij` | `ascii` | `unicode` |
| --- | --- | --- | --- |
| Fullscreen | `(FULLSCREEN)` | `Z` | `⤢` |
| Sync | `(SYNC)` | `S` | `⇄` |
| Floating | `(FLOATING)` | `F` | `❐` |
| Bell | `●` | `!` | `●` |

The `nerd-font` theme draws Material Design glyphs for the same four: fullscreen, sync, layers and a bell.

![Five tabs after the NORMAL mode prefix under three alias themes. With zellij the flags read (FULLSCREEN), (SYNC) and a dot; with ascii they read Z, S and !; with unicode they are symbols, and the prefix gains a keyboard symbol](../img/zjtabs/flags-alias-themes.svg)

The `powerline` and `pills` shapes and the `nerd-font` symbols need a Nerd Font in your terminal. If it draws them two columns wide, set `ambiguous_width 2` so the bar measures them correctly. See [Nerd Fonts](../themes.md#nerd-fonts).
