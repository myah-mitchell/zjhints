# Entry formats and flags

Each floating pane is drawn as an entry from a format string, and the states worth noticing from the bar, such as a minimized pane, are flags drawn from small formats of their own. This page covers the placeholders, the flags, the gaps between entries, the empty bar, the launcher's button and the themes, for anyone restyling the bar.

With nothing set, every entry is a ribbon in the Zellij theme's colors, the way Zellij's own tab bar draws a tab: the pane's title and its flags, with the focused pane in the selected ribbon color and a minimized pane in plain text. The sections below change that one part at a time, or all of it with a [theme](#themes).

## Placeholders

Three formats draw the entries, each in the format-string syntax: `#[...]` blocks set the style of what follows, and `$name` refers to a color alias. See [Styling](../styling.md) and [Colors](../styling.md#colors).

| Option | Draws | Unset |
| --- | --- | --- |
| `entry_format` | Each entry neither focused nor minimized | Zellij's unselected ribbon |
| `focused_entry_format` | The entry of the pane the keyboard goes to | Zellij's selected ribbon |
| `minimized_entry_format` | The entry of a minimized pane | Zellij's text colors, with no ribbon |

Each takes the same placeholders:

| Placeholder | Filled with |
| --- | --- |
| `{title}` | The pane's title, which for a terminal is usually the running command or the working directory |
| `{index}` | The entry's place on the bar, counted from 1 |
| `{tab}` | The name of the pane's tab, or its number while it has no name |
| `{flags}` | The pane's [flags](#flags) |

```kdl
color_theme "catppuccin-mocha"
entry_format           "#[fg=$fg,bg=$surface] {index} {title}{flags} "
focused_entry_format   "#[fg=$bg,bg=$accent,bold] {index} {title}{flags} "
minimized_entry_format "#[fg=$muted,bg=$surface] {index} {title}{flags} "
```

A format that leaves a placeholder out leaves that part off the entry. A format set to an empty string counts as unset, so the entry is drawn in Zellij's colors again.

`{index}` counts every entry the bar lists, the hidden ones included, so an entry keeps its number as the [window](overflow.md#the-window-around-the-focused-entry) moves. It changes when a pane before it opens or closes.

### Titles are drawn as text

The title and the tab's name are put into the format after the format has been read, so nothing in them is taken for styling. A pane titled `#[fg=red]build` shows exactly that, and a `$` or a `{` in a title is drawn as typed.

A control character in a title is drawn as an escape, such as `\u{1b}` for ESC, rather than sent to the terminal. So is a character that reorders text, such as `\u{202e}`, the right-to-left override, so a title cannot draw itself backwards.

## Flags

`{flags}` draws one indicator for each state a pane is in, in the order of this table, straight after the title.

| Option | Shown for a pane that | Default |
| --- | --- | --- |
| `minimized_format` | Is hidden from the screen and kept on the bar | ` (MINIMIZED)` |
| `exited_format` | Ran a command that has finished, and that Zellij holds open | ` (EXITED)` |
| `plugin_format` | Runs a plugin rather than a terminal | ` (PLUGIN)` |
| `other_tab_format` | Is in a tab other than the active one | none |

Each option is a format string. Only `other_tab_format` takes a placeholder, `{tab}`, for the name of the tab the pane is in. It is unset by default, as `tab_separator` already sets each tab's panes apart; set it to name the tab on each entry, or pick an alias theme, which gives it a symbol. With `pane_scope "tab"` the bar lists only the active tab's panes, so the flag never shows:

```kdl
other_tab_format " ({tab})"
```

Write the space before the flag yourself, as the defaults do, so the flag does not touch the title. An empty value hides that flag:

```kdl
plugin_format ""
```

A flag is drawn in the style of the entry around it, so a symbol sits on the entry's own background. A `#[...]` block in the flag changes only what it sets, such as the color or bold, and the entry's background stays under it:

```kdl
exited_format "#[fg=$red,bold] ✕"
```

An alias theme swaps all four flags for symbols at once. See [Themes](#themes).

## Spacing

`entry_spacer` is a format string drawn between neighboring entries of one tab, and between an entry and an [overflow marker](overflow.md#markers). It is one space by default, which keeps ribbons in Zellij's colors from running together.

`tab_separator` takes the spacer's place between the last entry of one tab and the first of the next, so you can see where one tab's panes end. It is a line with a space on each side, ` │ `, by default. Set either to an empty string for no gap, or to a styled divider of your own:

```kdl
entry_spacer  ""
tab_separator "#[fg=$muted] | "
```

## The empty bar

With no floating panes open, the bar draws nothing where the entries go. Set `empty_format` to draw something there instead, such as a hint at what the bar is for:

```kdl
empty_format "#[fg=$muted] no floating panes"
```

While the bar draws nothing at all, `collapse_when_empty` hands its row back to the panes around it. The launcher's button and the tray are drawn whether or not there are entries, so with the button on, as it is by default, the row stays. See [Giving the row back](../nested-sessions.md#giving-the-row-back).

## The launcher's button

The button at the left end is drawn from `launcher_format`, with `{symbol}` for the text of `launcher_symbol`. The symbol is `Menu` by default. With no format set it is drawn in bold on Zellij's unselected ribbon. It is drawn only while `mouse` and `show_launcher` are on, since it does nothing but take a click. See [The button](launcher.md#the-button) for the symbols each alias theme gives it, and [Launcher](launcher.md) for its menu.

## Themes

The three theme options can each be picked on its own, and take the same values as for zjhints. See [Themes](../themes.md). For zjtaskbar they set:

| Option | What it sets for the bar |
| --- | --- |
| `color_theme` | The color aliases, `$accent` and the rest, exactly as for zjhints |
| `format_theme` | The three entry formats, `entry_spacer`, `tab_separator`, both overflow markers, `overflow_str`, `launcher_format`, `tray_spacer`, and the formats of the tray's clock, session and mode |
| `alias_theme` | The four flag options, `launcher_symbol`, and a `mode_key_<mode>` symbol for each mode |

No theme sets `empty_format`. Your own options always win over a theme, one option at a time.

The same four panes in each format theme, in `catppuccin-mocha` colors:

![The launcher's Menu button and the same four floating panes under each format theme, in catppuccin-mocha colors: zellij ribbons, flat blocks, plain colored text, blocks joined by Powerline arrows, and rounded pills. The focused editor entry stands out in the accent color and the minimized notes entry is dimmer than the rest](../img/zjtaskbar/themes-shapes.svg)

| Value | Shape |
| --- | --- |
| `zellij` | Nothing set: ribbons in the Zellij theme's colors |
| `flat` | The title on a `$surface` block, the focused entry on an `$accent` block in bold, a minimized one in `$muted` |
| `plain` | Colored text with no blocks, two spaces apart: `$fg` for an entry, bold `$accent` for the focused one, `$muted` for a minimized one |
| `powerline` | The flat blocks, each ending in a Powerline arrow |
| `pills` | Each entry a rounded pill |

Under every format theme the tab separator is a `$muted` line, and the button is the shape of an entry with its symbol in `$accent`, bold in every shape but `plain`. The tray's items face back along the line. See [Themes](tray.md#themes) in the tray's page.

The symbols an alias theme gives the flags and the launcher's button:

| Part | `zellij` | `ascii` | `unicode` | `nerd-font` |
| --- | --- | --- | --- | --- |
| Minimized | `(MINIMIZED)` | `_` | `↓` | Material Design window-minimize, U+F05B0 |
| Exited | `(EXITED)` | `X` | `✕` | Material Design exit-to-app, U+F0206 |
| Plugin | `(PLUGIN)` | `P` | `⚙` | Material Design puzzle, U+F0431 |
| Another tab | none | `@<tab>` | `↗<tab>` | Material Design tab, U+F04E9, before the tab's name |
| Launcher | `Menu` | `+` | `☰` | Material Design apps, U+F003B |

In the table, `<tab>` stands for the name of the pane's tab.

![Five floating panes as pills after the launcher's button: the focused editor, notes minimized, cargo build exited, the filepicker plugin, and man zellij in the docs tab. With the zellij alias theme the button reads Menu and the flags read (MINIMIZED), (EXITED) and (PLUGIN), with none for the other tab; with ascii the button is a plus and the flags are _, X, P and @docs; with unicode the button is three bars and the flags are a down arrow, a cross, a gear and an up-right arrow before docs](../img/zjtaskbar/flags-alias-themes.svg)

The image leaves out the `nerd-font` symbols, as it has no Nerd Font to draw them with. The `powerline` and `pills` shapes and the `nerd-font` symbols need a Nerd Font in your terminal. Each glyph takes one column, the width Zellij lays it out at. See [Nerd Fonts](../themes.md#nerd-fonts).
