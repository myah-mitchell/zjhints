# Labels

Every hint is two things: the keys it shows, and the label, meaning the words printed next to them. This page is about changing those words.

## Placeholders

The label options are families rather than single keys: you append your own value to the option name. Each family's variable part is written in angle brackets.

| Placeholder | You replace it with |
| --- | --- |
| `<id>` | A hint's permanent name, which is never displayed, such as `split_down` |
| `<action>` | The Zellij action a binding runs, such as `switch_to_mode_normal` |
| `<mode>` | A lowercased Zellij mode name, such as `locked` |

## Ids and labels

Each hint has both a label and an id:

- The label is what you see: `split down`, `focus`, `exit`.
- The id is the hint's permanent name, which is never displayed: `split_down`, `focus`, `mode_normal`.

Most ids look like their label with underscores, which makes them easy to guess, but the two do different jobs and keeping them apart is what makes everything here work.

### The id is the config key

`label_split_down "↓"` means: find the hint whose id is `split_down`, and change its label to `↓`. You can relabel a hint as often as you like and its id never moves, so your config keeps working.

### The id is also the merge key

Two bindings that resolve to the same id become one hint showing all of their keys. This is why ids exist at all, rather than just matching on the displayed text:

- Zellij binds focus-left, focus-down, focus-up and focus-right as four separate actions. All four resolve to the id `focus`, so they merge into a single `hjkl focus` hint instead of cluttering the bar with four.
- Pane mode's "new pane" and Tab mode's "new tab" both display the label `new`. Their ids differ (`new_pane`, `new_tab`), so they stay separate, and `label_new_tab "＋"` changes only the tab one.

Matching on displayed text would get the first case right and the second wrong. Ids get both.

## Changing a label

Name the id, give it the text you want:

```kdl
label_split_down  "split ↓"
label_focus       "◆"
label_mode_locked "lock"
label_next_layout "»"
label_mouse       ""      // empty string hides this hint entirely
```

This works for any hint, whether it came from the [curated list](hints.md#the-curated-list) or from [discovery](hints.md#discovered-hints).

## Finding a hint's id

Two cases:

1. It is in the [table below](#ids). Those are the hints the plugin has hand-picked ids and labels for.
2. It is not. Then the id is the Zellij action's name in `snake_case`, and the label is that same name with the underscores turned into spaces. So a hint reading `next swap layout` has the id `next_swap_layout`, and `label_next_swap_layout "»"` renames it.

That second rule is why nothing is ever unlabeled and every hint is addressable, including bindings this plugin has never heard of.

## Ids

The hints with hand-picked ids, and the label each shows unless you change it. Rows are grouped roughly by the mode they turn up in. Where the label column says "same", the label is the id with spaces instead of underscores.

| Id | Default label |
| --- | --- |
| `mode_normal`, `mode_locked`, `mode_pane`, `mode_tab`, `mode_resize`, `mode_move`, `mode_scroll`, `mode_search`, `mode_session`, `mode_rename`, `mode_tmux`, `mode_prompt` | the mode name (`lock` for locked) |
| `new_pane`, `close_pane`, `split_left`, `split_right`, `split_up`, `split_down` | `new`, `close`, `split …` |
| `stacked_pane`, `floating_pane`, `in_place_pane` | `stacked`, `floating`, `in place` |
| `fullscreen`, `float`, `embed`, `frames`, `pin` | same |
| `group_pane`, `group_marking` | `group`, `mark` |
| `focus`, `move_pane`, `move_pane_back`, `next_pane`, `prev_pane`, `toggle_focus` | `focus`, `move`, `move back`, `next`, `prev`, `toggle focus` |
| `new_tab`, `close_tab`, `next_tab`, `prev_tab`, `go_to_tab`, `toggle_tab`, `sync`, `move_tab` | `new`, `close`, `next`, `prev`, `tab`, `toggle`, `sync`, `move tab` |
| `break_pane`, `break_left`, `break_right` | `break …` |
| `resize`, `increase`, `decrease` | same |
| `scroll`, `page`, `half_page`, `top`, `bottom`, `edit` | same (`half page`) |
| `search_down`, `search_up`, `search_toggle` | `down`, `up`, `toggle` |
| `copy`, `clear`, `mouse`, `prev_layout`, `next_layout`, `confirm`, `deny`, `quit`, `detach`, `undo_rename`, `rename_session` | same (`prev layout`, `next layout`, `undo`, `rename`) |
| `rename`, `manager`, `config`, `plugins`, `about`, `share` | same |
| `layout_manager` | `layouts` |

Anything not listed falls back to the rule above: the action's own snake_case name serves as both id and label.

## By action name

A hint can also be addressed by the Zellij action behind it, using that action's snake_case name with any direction appended. These two are the same hint:

```kdl
label_split_down    "↓"   // by id
label_new_pane_down "↓"   // by the action, NewPane "Down"
```

More examples: `label_move_focus_left`, `label_resize_increase_left`, `label_switch_to_mode_locked`.

Ids are usually the better choice: they are shorter, and some hints are built from several actions at once (`resize` covers four, `mode_normal` gathers every key that leaves the mode), so they have no single action name to use. Reach for the action form when you already know the Zellij action and would rather not look one up.

If both are set for the same hint, the id wins.

## Per-mode labels

Prefixing a mode name scopes a label to that mode alone:

```kdl
label_mode_normal        "exit"   // every mode
label_locked_mode_normal "unlock" // except Locked, which says "unlock"
```

Useful where one action means different things in different modes. Leaving Locked is an unlock; leaving Pane mode is just an exit.

Mode names are the lowercased Zellij modes, and the suffix is an id or an action name, both of which work scoped:

```text
normal  locked   pane         tab        resize      move  scroll
search  session  entersearch  renametab  renamepane  tmux  prompt
```

```kdl
label_locked_switch_to_mode_normal "unlock" // same thing, by action
```

> [!NOTE]
> `mode_format_<mode>` spells three of these differently: `enter_search`, `rename_tab` and `rename_pane`, matching zjstatus's own per-mode keys. Scoped labels and formats use the unseparated spelling above.

Lookup runs most specific first, so a mode-scoped label always beats a global one:

1. `label_<mode>_<id>`
2. `label_<mode>_<action>`
3. `label_<id>`
4. `label_<action>`

An empty value still hides, at whichever scope you set it. For example, `label_pane_mode_normal ""` drops the hint in Pane mode and leaves it elsewhere. The reverse works too: hide globally with `label_mode_normal ""`, then bring it back in one mode with `label_locked_mode_normal "unlock"`.

## Combining hints

Hints normally merge only when they share an [id](#ids-and-labels). There is one deliberate exception: giving two hints the same label yourself merges them too, gathering all their keys into one hint.

```kdl
label_next_layout "swap layout"
label_prev_layout "swap layout"   // one hint, both keys
```

Next-layout and previous-layout are separate actions with separate ids, so by default they are two hints. Labeling both `swap layout` says to treat these as one thing, and they collapse into a single hint carrying both keys.

This applies only to labels you set. Hints that merely ship with the same built-in label, such as Pane's `new` and Tab's `new`, stay separate, so a merge is always something you asked for rather than an accident of the default table.

One consequence worth knowing: a merged hint's id becomes the label you chose, so [hint_order](ordering.md#hint-order) should refer to it by that label.
