# Mouse

With `mouse` on, the bar works like Zellij's own tab bar: click a tab to go to it, scroll to step through the tabs, double-click to rename one, and right-click for Zellij's menus. A middle click closes a tab after asking, on a Zellij build that sends plugins middle clicks. A click on the mode prefix opens the settings screen, and a click on the session name opens Zellij's session manager. This page covers each of those and the permissions they use.

`mouse` is on by default, since a tab bar you cannot click is unusual. Turn it off to leave the bar to the keyboard:

```kdl
mouse false
```

While the bar draws nothing, because it is hidden or has no tabs, the mouse does nothing on its row. See [Hiding the bar](../hiding.md).

## Switching tabs

Left-click a tab to switch to it. A click anywhere on the tab counts, its number and flags included. The spacers between tabs and the empty space after them do nothing.

### Markers

Clicking an overflow marker switches to the hidden tab nearest to it. The tab then becomes the active one, the window moves to it, and the marker shows what is left. See [Markers](overflow.md#markers).

## Scrolling

The wheel steps through the tabs, over any part of the bar. Scroll up goes to the next tab, to the right, and scroll down to the previous one, as in Zellij's own tab bar.

At the first or last tab the wheel does nothing. Wrapping around would make a fast scroll spin through every tab.

To swap the two directions, so that scroll up goes to the previous tab, set:

```kdl
mouse_wheel_reversed true
```

## Renaming

Double-click a tab to rename it. If it is not the active tab, it becomes the active one first, because the new name is typed into the active tab. Type the new name, press `Enter` to keep it or `Esc` to cancel.

While the name is empty the tab shows [`rename_placeholder`](formats.md#renaming).

## Closing

Middle-click a tab to close it. A middle click anywhere else on the bar, such as a marker, a spacer, the mode prefix, the session name or the empty space after the tabs, does nothing.

Before closing, the bar asks in Zellij's prompt popup, naming the tab by its number and name, because closing a tab closes every pane in it and ends what runs there. *Cancel* is selected when the prompt opens, so `Enter` or `Esc` straight away keeps the tab. Choose **Close tab** to close it. If the tab closes some other way while the prompt is open, choosing **Close tab** does nothing.

To close a tab at once, without asking, set:

```kdl
mouse_close_confirm false
```

Closing needs a Zellij build that sends plugins middle clicks. See [Zellij support](#zellij-support).

## Opening the settings screen

Left-click the [mode prefix](formats.md#mode-prefix) at the start of the bar to open the settings screen on the alias that bar was started from. Clicking it again while the screen is open brings it back up instead of opening a second one. No config is needed. See [Settings screen](../settings.md).

A middle click or a double-click on the prefix does nothing, and a right-click there opens the `bar` menu, as on a spacer. The right-click menus reach the screen too. See [Reaching the settings screen](#reaching-the-settings-screen).

## Opening the session manager

Left-click the [session name](formats.md#session-name) at the end of the bar to open Zellij's session manager, the same as `Ctrl o` then `w` in Zellij's default keybindings. It opens in a floating pane and takes the focus. If it is already open, the click focuses it instead, bringing it to the tab you are on. No config is needed.

The click opens whatever your Zellij config names `session-manager` in its `plugins` block, as the keybinding does. Only the name itself counts: the spacer before it and the empty space between it and the tabs do nothing. A middle click or a double-click on the name does nothing, and a right-click opens the `bar` menu, as on a spacer.

## Right-click menus

Right-clicking opens one of Zellij's context menus at the pointer. The items come from the `context_menu` block of Zellij's own config, not from this plugin.

- Right-click a tab for the menu of that tab, from the `tab` section. Its defaults include *Rename tab*, moving the tab and *Close tab*.
- Right-click anywhere else on the bar, such as a spacer, a marker, the mode prefix, the session name or the empty space after the tabs, for the `bar` section. Its defaults include *New pane* and *New tab*.

Both menus end with the `common` section, whose defaults include *Settings…* and *Detach*. To add an item to a menu, add it to the matching section:

```kdl
context_menu {
    bar {
        item "Hide tabs" {
            MessagePlugin { name "zjtabs::hide"; }
        }
    }
}
```

See [Hiding the bar](../hiding.md) for the messages a bar listens for.

### Reaching the settings screen

*Settings…* opens Zellij's own settings screen. Go to its *Plugins and environment* page, where each running zjtabs bar adds a *zjtabs* entry under *Plugin settings*. Selecting it opens the settings screen on the alias that bar was started from. No config is needed. See [Settings screen](../settings.md).

## Permissions

Turning `mouse` on asks for one permission on top of the ones the bar always needs, and a second while `mouse_close_confirm` is on too:

| Permission | Used for |
| --- | --- |
| *Execute actions as the user* | Renaming a tab on a double-click, and opening the session manager from the session name |
| *Start new terminals and plugins* | Asking before a middle click closes a tab, since the question is a Zellij prompt |

Switching and closing tabs and opening Zellij's menus also need permission to change Zellij state, which the bar always asks for to add its *zjtabs* settings entry. The full list is in [Permissions](overview.md#permissions).

Zellij asks once for the whole instance, in the pane of one of its bars, and remembers the answer. Turning `mouse` on after the bar was already granted its usual permissions asks again, for the whole set with the new ones. So does turning `mouse_close_confirm` on while `mouse` is on. Turning either off asks for nothing.

## Zellij support

The right-click menus use the context menu APIs from [#5685](https://github.com/zellij-org/zellij/pull/5685), which are merged and expected in the next Zellij release. Until then, they need a Zellij build that includes them. See [Zellij support](../../README.md#zellij-support).

Closing a tab with a middle click needs a Zellij that sends plugins middle clicks, which no Zellij release does. The build zjtabs is compiled against, [fork-ea-v0.46.7-261103](https://github.com/myah-mitchell/zellij/releases/tag/fork-ea-v0.46.7-261103), sends them. On any other build a middle click on the bar does nothing.
