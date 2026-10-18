# Themes

A theme gives the bar a complete look in one line: a palette of color aliases, plus the key, description and mode formats that use it. You can follow your Zellij theme, pick a built-in one, or style everything yourself.

```kdl
theme "catppuccin-mocha"
```

## Contents

- [Choosing a theme](#choosing-a-theme)
- [The built-in themes](#the-built-in-themes)
- [What a theme sets](#what-a-theme-sets)
- [Overriding parts of a theme](#overriding-parts-of-a-theme)
- [Using a theme's colors in your formats](#using-a-themes-colors-in-your-formats)
- [Following Zellij or styling it yourself](#following-zellij-or-styling-it-yourself)
- [Powerline and Nerd Fonts](#powerline-and-nerd-fonts)

## Choosing a theme

`theme` takes one of three kinds of value:

| Value | What the bar looks like |
| --- | --- |
| `zellij` | The active Zellij theme's palette. This is the default |
| `custom` | Your own options only, with no theme values added |
| A built-in name | That theme's palette and formats, under your own options |

Names are matched ignoring case and surrounding spaces. A name that is none of these is written to the plugin log and treated as `zellij`.

## The built-in themes

| Name | Palette | Key block color |
| --- | --- | --- |
| `catppuccin-mocha` | [Catppuccin](https://catppuccin.com/palette) Mocha, dark | Mauve |
| `catppuccin-latte` | Catppuccin Latte, light | Mauve |
| `dracula` | [Dracula](https://draculatheme.com/contribute), dark | Purple |
| `nord` | [Nord](https://www.nordtheme.com/docs/colors-and-palettes), dark | Frost cyan |
| `gruvbox-dark` | [Gruvbox](https://github.com/morhetz/gruvbox#palette) dark, with its bright accents | Orange |
| `tokyo-night` | [Tokyo Night](https://github.com/folke/tokyonight.nvim), the night variant | Blue |
| `powerline` | Catppuccin Mocha, with [Powerline arrows](#powerline-and-nerd-fonts) joining the blocks | Mauve |

The flat themes draw each hint as two blocks side by side: the key on the accent color, the description on a darker surface. `powerline` draws the same blocks, each ending in an arrow.

## What a theme sets

Every built-in theme defines the same color aliases, so a format that refers to `$accent` works under any of them.

| Alias | Role |
| --- | --- |
| `$bg` | The darkest background, and the text color on bright blocks |
| `$fg` | Body text |
| `$surface` | The block behind descriptions |
| `$muted` | De-emphasized text |
| `$accent` | The theme's signature color, used for keys and the default mode prefix |
| `$red` `$green` `$yellow` `$blue` `$magenta` `$cyan` | The theme's own version of each hue |

Dracula has no blue, so its `$blue` is the same purple as its `$accent`.

Alongside the aliases, a theme sets these options:

| Option | Value |
| --- | --- |
| `key_format` | `$bg` text on an `$accent` block, bold |
| `desc_format` | `$fg` text on a `$surface` block |
| `mode_format` | `$bg` text on an `$accent` block, bold |
| `mode_format_<mode>` | The same prefix on a color chosen by mode |
| `hint_spacer` | A single space |

The mode colors are the same in every theme:

| Color | Modes |
| --- | --- |
| `$green` | `normal`, `tmux` |
| `$red` | `locked` |
| `$yellow` | `resize`, `move` |
| `$blue` | `pane`, `rename_pane` |
| `$magenta` | `tab`, `rename_tab`, `session` |
| `$cyan` | `scroll`, `search`, `enter_search` |
| `$accent` | `prompt` |

A theme leaves several things alone. `show_mode` stays off until you turn it on, so the mode prefix only appears when you ask for it. `drop_indicator` and the per-hint `key_format_<id>` and `desc_format_<id>` options are not set either, so they keep their defaults.

## Overriding parts of a theme

Your own options always win. A theme only fills in what you left unset, one option at a time:

```kdl
theme "nord"
// Recolor one alias, and every format that uses it follows.
color_accent "#ebcb8b"
// Replace one format; the rest of the theme stays.
desc_format "#[fg=$fg,bg=$surface,italic] {desc} "
```

- A `color_<name>` you set replaces that alias only. You can also add aliases of your own.
- A `key_format`, `desc_format`, `mode_format` or `hint_spacer` you set replaces the theme's. Per-hint formats such as `key_format_quit` apply to their hint as they do without a theme.
- Setting `mode_format` replaces the prefix for every mode. The theme's per-mode colors are then left out, so they cannot take priority over it. Set `mode_format_<mode>` yourself to color a single mode.
- An empty value turns that part of the theme off. `key_format ""` returns the keys to the Zellij palette while the rest of the theme stays.

In the [resolution order](styling.md#styling-one-hint) for a hint's format, a theme's `key_format` sits where your own global `key_format` would.

## Using a theme's colors in your formats

The aliases are ordinary `color_<name>` options, so any format string can use them:

```kdl
theme "tokyo-night"
key_format_quit "#[fg=$bg,bg=$red,bold] {key} "
desc_format_quit "#[fg=$red,bg=$surface] {desc} "
drop_indicator "#[fg=$muted]…"
hint_spacer "#[fg=$muted] │ "
```

Switching to another theme recolors these as well, since they refer to the aliases rather than to hex values.

## Following Zellij or styling it yourself

`zellij` and `custom` both add nothing to your options. Any key, description or mode prefix you have not styled is drawn from the active Zellij theme palette, as described in [Styling](styling.md).

- Use `zellij`, or leave `theme` out, to follow whichever Zellij theme is active.
- Use `custom` when you style the bar with your own `color_<name>` and format options and want the config to say so.

The two behave identically. Spelling out `custom` records that the look is yours, which tells a reader of the config that no built-in theme is meant to be in play.

## Powerline and Nerd Fonts

`powerline` ends each block with a Powerline arrow, a Nerd Font glyph (U+E0B0). Without a Nerd Font in your terminal the arrows render as empty boxes.

Nerd Font glyphs are East Asian Ambiguous. If your terminal draws them two columns wide, set `ambiguous_width 2` so [fitting](fitting.md#glyph-width) measures the bar correctly.
