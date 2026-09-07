# Keys and ordering

## Direction keys

Actions like focus and move are typically bound to both the `hjkl` letters and the arrow keys, which makes for a long hint. `direction_keys` picks one family:

```kdl
direction_keys "both"    // default: show everything that is bound
direction_keys "arrows"  // drop hjkl, keep ←↓↑→
direction_keys "letters" // drop the arrows, keep hjkl
```

The reduction only applies when **both** families are actually present on a hint, so a binding that only has arrows (or only letters) is never emptied.

Accepted spellings: `arrows`/`arrow`, and `letters`/`hjkl`/`vim`. Anything unrecognized falls back to `both`.

## Key order

Zellij reports a hint's keys in an arbitrary order: tab selection can arrive as `271543689`. They are sorted into a predictable reading order by default; `key_order` says which keyboard that order follows:

```kdl
key_order "qwerty"  // default
key_order "dvorak"
key_order "colemak"
key_order "abcdef"  // ignore the keyboard: 0-9 then a-z
key_order "none"    // leave keys as Zellij reports them
```

Within a hint, keys sort by kind so unlike keys never interleave:

| Kind | Order |
| --- | --- |
| Function keys | by number (`F1 F2 F3`) |
| Digits | along the digit row (`1234567890`), so `0` comes last |
| Letters | by layout row, then left to right |
| Punctuation | likewise, but after all letters |
| Arrows | `hjkl` order (`←↓↑→`) |
| Everything else | roughly physical, starting `Esc Tab Enter` |

Letters and punctuation are separate groups so they don't interleave at row boundaries: `o p a s \` sorts to `opas\`, not `op\as`.

Modifiers sort ahead of the key itself, so keys sharing one stay in a single run: a hint bound to both `hjkl` and `Ctrl hjkl` renders as `hjkl ^hjkl`, not as an interleaved `h ^h j ^j`. Groups run unmodified first, then `Ctrl`, `Super`, `Alt`, `Shift`; a key with several modifiers sorts with the strongest it carries, so `Ctrl Shift p` lands in the `Ctrl` group.

Unmodified keys lead because they are the plainest way to reach the action, and because a hint too wide to fit is [cut from the right](fitting.md#what-gets-dropped), so whatever sorts first is what survives.

This only changes ordering. Which keys appear is decided by `direction_keys` and the label settings, and is unaffected.

Note that letters sort by **physical position**, so `hjkl` reads in that order on QWERTY but not on Dvorak or Colemak, where those letters sit elsewhere. If you use a non-QWERTY layout but keep vim-style bindings on their QWERTY positions, `key_order "qwerty"` is likely what you want.

### Alphabetical

`key_order "abcdef"` ignores the keyboard entirely and sorts by the character itself, digits `0`–`9`, then letters `a`–`z`:

```kdl
key_order "abcdef"
```

Useful when the hint is a list to scan rather than keys to reach for, and it is layout-independent, so it reads the same on any keyboard.

Two differences from the layout modes:

- `0` comes **first**, not last: this is plain ascending order, not a walk along the digit row.
- `hjkl` happens to read in order here too, since `h < j < k < l` alphabetically, though for an unrelated reason.

Digits, letters and punctuation are still kept in separate groups, and arrows, function keys and named keys are unaffected: those never had a layout position to sort by.

Accepted spellings: `abcdef`, `alphabetical`, `alpha`, `abc`.

## Hint order

By default each mode shows its [curated list](hints.md#the-curated-list) first, in the order the plugin builds it, followed by anything [discovery](hints.md#discovered-hints) turned up. `hint_order` overrides that with a comma-separated list of hint ids:

```kdl
hint_order "*, exit"          // exit always last
hint_order "new, close, *"    // these two first, rest unchanged
hint_order "new, *, quit"     // pin both ends
hint_order "pane, tab, move"  // explicit order, rest follow
```

The `*` marks where every hint you didn't name goes. Entries before it lead, entries after it trail, and unlisted hints keep the order the mode built them. Leaving `*` out is the same as ending with one.

The list applies to **every mode**, so an id that mode doesn't have is simply ignored: `"*, exit"` puts `exit` last wherever it appears and changes nothing elsewhere. Naming an id that doesn't exist at all is equally harmless, so a typo degrades to no effect rather than an error.

Entries match a hint's **id or its label**, case-insensitively. Labels matter for hints you have fused by giving them a shared label: those carry an internal id like `=swap layout`, and naming them by label avoids having to write that.

`hint_order` does double duty: what you pin here is also what survives longest when the window is too narrow to show everything. See [What gets dropped](fitting.md#what-gets-dropped).

This orders hints; [`key_order`](#key-order) orders the keys inside one.
