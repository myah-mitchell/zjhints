# Mouse

With `mouse` on, the bar works like a desktop taskbar: click an entry to bring its pane up, click the focused one to minimize it, double-click to rename, middle-click to close after asking, and right-click for a menu of what can be done to the pane. The wheel steps the focus through the panes on screen. The launcher's button opens its menu, and the tray's items open what they name. Every pane also gets minimize, maximize and close buttons on its frame. This page covers each of those and the permissions they use.

`mouse` is on by default, since clicking an entry is what a taskbar is for. Turn it off to leave the bar to the keyboard:

```kdl
mouse false
```

With `mouse` off, the launcher's button is not drawn either, as it does nothing but take a click. While the bar draws nothing, because it is hidden, hidden in a nested session, or has nothing to show, the mouse does nothing on its row. See [Hiding the bar](../hiding.md).

## What each click does

| Where | Left click | Middle click | Right click |
| --- | --- | --- | --- |
| An entry | Focus its pane, restore it if minimized, or minimize it if focused | Close the pane | The pane's menu |
| A marker | Bring up the hidden entry nearest to it | Nothing | Zellij's bar menu |
| The launcher's button | The launcher's menu | Nothing | Zellij's bar menu |
| The tray's `session` | Zellij's session manager | Nothing | Zellij's bar menu |
| The tray's `mode` | The settings screen | Nothing | Zellij's bar menu |
| The tray's `clock` | Nothing | Nothing | Zellij's bar menu |
| A spacer, a separator or blank space | Nothing | Nothing | Zellij's bar menu |

A double-click renames the pane of the entry it lands on, and does nothing anywhere else. The wheel works over any part of the bar.

## Focusing and restoring

Left-click an entry to focus its pane. A click anywhere on the entry counts, its flags included. If the tab's floating panes are hidden, the bar shows them.

If the pane is in another tab, which happens with `pane_scope "all"`, the bar pulls it into the tab on screen rather than switching tabs. The pane keeps its position and size, and leaves the other tab. *Focus* in the pane menu and a marker's click pull it the same way. If Zellij refuses to move it, the bar switches to the pane's tab instead.

Left-click a minimized entry to restore its pane where it stood, focused. A minimized pane from another tab is restored into the tab on screen. See [Minimizing](#minimizing).

### Markers

Clicking an overflow marker brings up the hidden entry nearest to it, focusing or restoring its pane as a click on that entry would. The entry then has the focus, the window moves to it, and the marker shows what is left. A marker never minimizes, since the entry it stands for is never the focused one. See [Markers](overflow.md#markers).

## Minimizing

Left-click the focused entry to minimize its pane. The pane is hidden from the screen and its entry stays on the bar, drawn in `minimized_entry_format` with the `minimized_format` flag, until you restore or close it. A pane you bring back some other way, such as with Zellij's own keys, is an ordinary floating pane again: focusing it leaves it where it is, and you can minimize it again.

Zellij places a hidden floating pane as if it were new when it comes back, with a frame and at a size of its own choosing. So the bar records where the pane stood as it hides it, its position, its size and whether it has a frame, and puts it back there on restore. If the terminal shrank in the meantime, Zellij moves the pane in to fit.

Two things are not kept:

- A pinned floating pane may come back unpinned. Zellij does not tell plugins whether a pane is pinned, so the bar cannot put that back.
- The bar remembers the panes it minimized only while it runs. Zellij reports a hidden floating pane the same way as a hidden tiled one, so a pane minimized before the bar restarts, as when the plugin is reloaded, stays hidden and is not listed again.

## Renaming

Double-click an entry to rename its pane. The bar brings the pane up first, restoring or focusing it, because Zellij renames the focused pane as you type. Type the new name, press `Enter` to keep it or `Esc` to cancel.

Zellij reports a double-click as two left clicks and then the double-click itself. A second left click on the same column within 400 milliseconds, Zellij's own double-click time, is taken as part of the double-click and does nothing, so the pair does not undo itself. The first click still acts. On the focused entry, it minimizes the pane, and the double-click restores it before renaming, so the pane flickers briefly and ends where it was.

## Closing

Middle-click an entry to close its pane. A middle click anywhere else on the bar, a marker included, does nothing, since a marker stands for panes you cannot see.

Before closing, the bar asks in Zellij's prompt popup, *Close pane?*, naming the pane by its title, because closing a pane ends what runs in it. *Cancel* is selected when the prompt opens, so `Enter` or `Esc` straight away keeps the pane. Choose **Close pane** to close it. If the pane closes some other way while the prompt is open, choosing **Close pane** does nothing.

To close a pane at once, without asking, set:

```kdl
mouse_close_confirm false
```

With or without the prompt, a pane that has gone from the bar since you clicked is left alone.

Closing with a middle click needs a Zellij build that sends plugins middle clicks. The pane menu's *Close* works on any build with the menu. See [Zellij support](#zellij-support).

## The pane menu

Right-click an entry for a menu of what can be done to its pane, at the pointer:

| Item | Shown for | What it does |
| --- | --- | --- |
| *Restore* | A minimized pane | Restores the pane where it stood, focused |
| *Focus* | A pane on screen that the keyboard does not go to | Focuses the pane |
| *Minimize* | A pane on screen | Minimizes the pane |
| *Rename* | Every pane | Brings the pane up and starts renaming it |
| *Close* | Every pane | Closes the pane, asking first while `mouse_close_confirm` is on |

Choose an item to act on the pane as it stands then. Press `Esc` or click outside the menu to close it without doing anything.

The bar acts only on the menu or prompt it opened last. If you open another, from the bar or the launcher's button, before answering one, the earlier one does nothing when answered.

## Zellij's bar menu

Right-click anywhere on the bar but an entry for Zellij's own context menu, from the `bar` section of the `context_menu` block in Zellij's config. Its defaults include *New pane* and *New tab*, and it ends with the `common` section, whose defaults include *Settings…* and *Detach*. To add an item, add it to the matching section:

```kdl
context_menu {
    bar {
        item "Hide taskbar" {
            MessagePlugin { name "zjtaskbar::hide"; }
        }
    }
}
```

See [Hiding the bar](../hiding.md) for the messages a bar listens for.

### Reaching the settings screen

*Settings…* opens Zellij's own settings screen. Go to its *Plugins and environment* page, where each running zjtaskbar bar adds a *zjtaskbar* entry under *Plugin settings*. Selecting it opens the settings screen on the alias that bar was started from. A left click on the tray's `mode` item does the same, once `mode` is in `tray`. See [Settings screen](../settings.md).

## The launcher and the tray

Left-click the launcher's button for its menu of new floating panes. See [Launcher](launcher.md).

Left-click the tray's `session` item to open Zellij's session manager, the same as `Ctrl o` then `w` in Zellij's default keybindings. It opens in a floating pane, or takes the focus if it is open already. Left-click the `mode` item to open the settings screen. See [Tray](tray.md#clicking).

## Scrolling

The wheel steps the focus through the panes on screen, over any part of the bar. Scroll up goes to the next pane, to the right, and scroll down to the previous one, as in Zellij's own tab bar. With no listed pane focused, scroll up goes to the first and scroll down to the last.

Minimized panes are skipped, so looking along the bar never brings back a pane you put away. With `pane_scope "all"`, the wheel crosses into other tabs and switches to the pane's tab, rather than pulling it in as a click does, so scrolling past a pane leaves it where it is. At the first or last pane the wheel does nothing. Wrapping around would make a fast scroll spin through every pane.

To swap the two directions, so that scroll up goes to the previous pane, set:

```kdl
mouse_wheel_reversed true
```

## Frame buttons

Every pane that takes focus, tiled or floating, gets three buttons at the right of its frame's top row: minimize, maximize and close. They work with `mouse` off too, since the clicks land on the frame and not the bar.

| Button | Left click |
| --- | --- |
| Minimize, `─` | Minimizes the pane, as a left click on its focused entry does; a tiled pane is floated first |
| Maximize, `□` | Fills the tab with the pane, the button turning to `❐`; click again for its own size |
| Close, `✕` | Closes the pane, asking first while `mouse_close_confirm` is on |

Other mouse buttons on them do nothing. A minimized pane keeps its buttons for when it comes back.

A minimized tiled pane joins the bar as a floating pane, and from then on is one like any other. It comes back where Zellij places a new floating pane, since it never stood anywhere as one. The last tiled pane of a tab has no minimize button, because Zellij keeps a tab's last tiled pane tiled.

Each tab's panes take their buttons from one bar, so no frame shows two sets:

- A tab with a bar gets its buttons from that bar. A pane moved to another tab takes that tab's buttons.
- A tab without a bar gets them from the session's first bar, the one Zellij loaded first. A pane minimized there joins that bar. With `pane_scope "tab"`, that bar doesn't list the tab, so its panes have no minimize button.

A bar recognizes the others by the plugin URL they run. With two zjtaskbar aliases in one session, each counts only its own bars, so a frame can show both sets. The buttons show only on frames Zellij draws a title row for, so a borderless pane has none.

Turn the buttons off with `frame_buttons false`, or change or leave off one with its text option. See [Frame](configuration.md#frame).

## Permissions

Turning `mouse` on asks for two permissions on top of the ones the bar always needs:

| Permission | Used for |
| --- | --- |
| *Execute actions as the user* | Renaming a pane, opening the session manager from the tray, and the launcher's new terminal and Zellij screens |
| *Start new terminals and plugins* | The pane menu, the close prompt and the launcher's menu, since each is a Zellij prompt, and the launcher's plugin entries |

Focusing, minimizing, moving and closing panes, the frame buttons and opening Zellij's bar menu also need permission to change Zellij state, which the bar always asks for to add its *zjtaskbar* settings entry. The full list is in [Permissions](overview.md#permissions).

With `mouse` off, the close button's prompt still asks for *Start new terminals and plugins*, unless `mouse_close_confirm` is off too.

Zellij asks once, in the plugin's pane, and remembers the answer. Turning `mouse` on after the bar was already granted its usual permissions asks again, for the whole set with the new ones. Turning it off asks for nothing.

## Zellij support

The pane menu, the launcher's menu, the close prompt and the bar menu use the prompt and context menu APIs from [#5685](https://github.com/zellij-org/zellij/pull/5685), and the two menus close on a click outside them through [#5778](https://github.com/zellij-org/zellij/pull/5778). Both are merged and expected in the next Zellij release. Until then, they need a Zellij build that includes them. See [Zellij support](../../README.md#zellij-support).

Closing a pane with a middle click needs a Zellij that sends plugins middle clicks, which no Zellij release does. The build zjtaskbar is compiled against, [fork-ea-v0.46.7-261103](https://github.com/myah-mitchell/zellij/releases/tag/fork-ea-v0.46.7-261103), sends them. On any other build a middle click on the bar does nothing.

The frame buttons and pulling a pane into the tab on screen need the plugin frame items and the tab argument of `show_pane_with_id`, which are also only in that build.
