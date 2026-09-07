# Styling

Each hint is rendered in two parts: the key, such as `Ctrl + p`, and its description, such as `pane`. By default both are styled from the active Zellij theme palette so they blend in with the rest of your status bar.

To customize them, set `key_format` and `desc_format`. These use the same format-string syntax as any other [zjstatus](https://github.com/dj95/zjstatus) widget, so styling hints works just like styling `{mode}`, `{tabs}`, and friends:

- Wrap styling directives in `#[...]`; everything after a block is painted with that style until the next block.
- `{key}` and `{desc}` are placeholders substituted with the keybinding text and its label, respectively.
- If either option is unset or empty, that part falls back to the theme palette, so you can restyle just the keys, just the descriptions, or both.

```kdl
key_format  "#[fg=$black,bg=$blue,bold] {key} "
desc_format "#[fg=#cdd6f4,bg=#313244,italic] {desc} "
```

## Contents

- [Placeholders](#placeholders)
- [Styling one hint](#styling-one-hint)
- [Replacing the keys](#replacing-the-keys)
- [Spacing](#spacing)
- [Directives](#directives)
- [Colors](#colors)
- [Key aliases](#key-aliases)
- [Modifier aliases](#modifier-aliases)
- [Chord aliases](#chord-aliases)
- [A complete alias set](#a-complete-alias-set)

## Placeholders

Most options on this page are families rather than single keys: you append your own value to the option name. Each family's variable part is written in angle brackets.

| Placeholder | You replace it with |
| --- | --- |
| `<id>` | A hint's [concept id](labels.md#ids-and-labels), giving `key_format_split_down` |
| `<label>` | A hint's label with spaces written as underscores, giving `key_format_swap_layout` |
| `<mode>` | A lowercased Zellij mode name, giving `key_format_locked_split_down` |
| `<name>` | The thing being aliased: a name you choose for `color_<name>` and the chord options, a key name for `key_alias_<name>`, a modifier for `mod_alias_<name>` |
| `<color>` | Any of the forms under [Colors](#colors), such as `#89b4fa` or `$blue` |
| `<n>` | An ANSI 256 color index, `0` to `255` |

## Styling one hint

`key_format` and `desc_format` set the look of every hint. Suffixing an id changes one of them:

```kdl
desc_format      "#[fg=$fg,bg=$bg] {desc} "         // every hint
desc_format_quit "#[fg=$red,bg=$bg,bold] {desc} "   // except quit
key_format_quit  "#[fg=$red,bg=$bg,bold] {key} "
```

Useful for the hints that are not like the others: a destructive action worth coloring, or the way out of a mode worth setting apart from the actions in it.

Resolution runs most specific first, and can be scoped to a mode exactly as [labels](labels.md#per-mode-labels) can:

1. `key_format_<mode>_<id>`
2. `key_format_<mode>_<label>`
3. `key_format_<id>`
4. `key_format_<label>`
5. `key_format`, the global setting
6. the theme palette

A hint can be named by its [id](labels.md#ids) or by its label, with spaces written as underscores. The label form matters for hints you have [fused](labels.md#combining-hints) under a shared label, whose internal id is that label with a marker prefix:

```kdl
label_next_layout "swap layout"
label_prev_layout "swap layout"
key_format_swap_layout "#[fg=$mauve]{key} "   // addresses the merged hint
```

An empty value falls back to the theme palette, the same as leaving the global option unset.

## Replacing the keys

`keys_<id>` substitutes a fixed string for a hint's keys:

```kdl
keys_go_to_tab "1-9"      // instead of 123456789
keys_focus     "hjkl/←↓↑→"
keys_mouse     "🖱"
```

For hints better described than enumerated: a long run of keys standing in as a range, or an action whose real binding says little. The string is used verbatim, so [key aliases](#key-aliases) and [key ordering](ordering.md#key-order) do not apply to it, since there are no keys left to alias or sort.

It is still measured at its real width, so [fitting](fitting.md#fitting-the-bar) accounts for a replacement that is wider than what it replaced.

An empty value drops the key part altogether, leaving the label to stand alone:

```kdl
keys_mouse ""   // renders as just "mouse"
```

This addresses hints the same way `key_format_<id>` does, mode scoping included.

## Spacing

By default hints sit directly against one another. `hint_spacer` inserts a separator between consecutive hints, never before the first or after the last, so it never leaks into the edges of the piped output:

```kdl
hint_spacer "  "             // just widen the gap
hint_spacer "#[fg=$gray] │ " // a styled divider
```

It is parsed as a format string like `key_format` and `desc_format`, without any placeholders, so `$name` color aliases and all the directives below work in it. The default key styling already emits one leading space per hint, so the spacer adds to that gap rather than replacing it.

## Directives

Inside `#[...]`, comma-separate any of the following:

- `fg=<color>`: foreground color
- `bg=<color>`: background color
- Any effect name from the list below, which takes no value

```text
bold  italic  underscore  blink  dim  strikethrough  reverse  hidden
```

## Colors

`<color>` accepts the same forms as zjstatus:

- `#RRGGBB`: hex RGB, for example `#89b4fa`
- A named color from the list below, or the same name with a `bright_` prefix
- `0` to `255`, or `colour<n>`: an ANSI 256 color index
- `$name`: a color alias, resolved from the matching `color_<name>` option

```text
black  red  green  yellow  blue  magenta  cyan  white
```

> [!NOTE]
> Directives zjstatus supports but the hint renderer cannot express, such as `us=` underline colors and the fancy underline variants, are accepted and ignored, so a palette shared with zjstatus never errors.

## Key aliases

By default keys are rendered with their Zellij names, such as `ENTER`, `ESC` and `SPACE`. Replace any of them with a symbol using `key_alias_<name>` options, which follow the same per-line alias pattern as `color_<name>`:

```kdl
key_alias_enter     "↵"
key_alias_space     "␣"
key_alias_esc       "⎋"
key_alias_tab       "⇥"
key_alias_backspace "⌫"
key_alias_left      "←"
```

- `<name>` is the lowercase key name. Aliases apply everywhere the key appears, including inside the `{key}` placeholder of a custom `key_format`.
- Keys without an alias keep their default representation, so you only need to set the ones you want to change.

The names recognized, with an alternative spelling in parentheses where there is one:

```text
enter (return)  esc (escape)  tab            space
backspace       delete (del)  insert (ins)   home
end             pageup (pgup) pagedown (pgdn)
up              down          left           right
capslock        scrolllock    numlock        printscreen
pause           menu          f1 to f12
```

Any single character works too, aliased as `key_alias_x` and so on.

## Modifier aliases

Modifiers work the same way via `mod_alias_<name>`, with `<name>` one of the four modifiers below:

```kdl
mod_alias_ctrl  "^"
mod_alias_alt   "⌥"
mod_alias_shift "⇧"
mod_alias_super "⌘"
```

Keys sharing a modifier are grouped into a single run rather than repeating the modifier once per key:

```text
Ctrl h  Alt <  Ctrl k  Alt l   renders as   ^hk ⌥<l
```

How the modifier joins its keys depends on the alias: a word such as `Ctrl` is followed by a space (`Ctrl hk`), while a symbol such as `^` hugs them (`^hk`). This is decided by the last character, so you get the natural spacing either way without configuring it.

## Chord aliases

If you have remapped a physical key to send an unusual modifier combination as a personal leader key, such as `Ctrl+Alt+Super+Shift` chosen because it collides with nothing else, spelling that combination out on every hint is excessive. `chord_mods_<name>` and `chord_alias_<name>` collapse it to one symbol instead:

```kdl
chord_mods_leader  "ctrl+alt+super+shift"
chord_alias_leader "&"
```

That turns `Ctrl+Alt+Super+Shift p` into `&p`.

- `<name>` is chosen by you, and only pairs a `chord_mods_<name>` with its `chord_alias_<name>`; it is not shown anywhere.
- List the modifiers in `chord_mods_<name>` separated by `+`, using the same four names as [modifier aliases](#modifier-aliases), case-insensitively.
- Only the exact combination matches. A hint bound to just part of a chord, `Ctrl` alone for instance, renders normally.
- Define more than one pair to alias more than one custom chord.
- A `chord_mods_<name>` with no matching `chord_alias_<name>` has no effect.

## A complete alias set

A full set using only standard Unicode, with no Nerd Font required and no private-use codepoints, so it renders in most terminal fonts:

```kdl
// Keys
key_alias_enter     "⏎"
key_alias_esc       "⎋"
key_alias_tab       "⇥"
key_alias_space     "␣"
key_alias_backspace "⌫"
key_alias_delete    "⌦"
key_alias_insert    "⎀"
key_alias_left      "←"
key_alias_down      "↓"
key_alias_up        "↑"
key_alias_right     "→"
key_alias_home      "⇱"
key_alias_end       "⇲"
key_alias_pageup    "⇞"
key_alias_pagedown  "⇟"

// Modifiers
mod_alias_ctrl  "^"
mod_alias_alt   "⌥"
mod_alias_shift "⇧"
mod_alias_super "⌘"
```

That turns `Ctrl Left` into `^←` and `Alt Shift PgDn` into `⌥⇧⇟`.

Nerd Fonts offer alternatives for several of these: arrows, Home and End, Page Up and Page Down, and Delete all have glyphs in the private-use area. They look sharper if you have the font, but they render as tofu for anyone who does not, so the set above is the safer default.

> [!NOTE]
> The four arrows and `⇧` are East Asian Ambiguous, as are all Nerd Font glyphs. If your terminal draws them double-width, set `ambiguous_width 2` or the hints will overflow slightly.

Everything else in that set is unambiguously one column. See [Glyph width](fitting.md#glyph-width).
