# Mouse

With `mouse` on, the bar works like Zellij's own tab bar: click a tab to go to it, scroll to step through the tabs, double-click to rename one, and right-click for Zellij's menus. This page covers each of those and the permissions they use.

`mouse` is on by default, since a tab bar you cannot click is unusual. Turn it off to leave the bar to the keyboard:

```kdl
mouse false
```

While the bar draws nothing, because it is hidden or has no tabs, the mouse does nothing on its row. See [Hiding the bar](../hiding.md).

## Switching tabs

Left-click a tab to switch to it. A click anywhere on the tab counts, its number and flags included. The spacers between tabs, and the empty end of the bar, do nothing.

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

## Right-click menus

Right-clicking opens one of Zellij's context menus at the pointer. The items come from the `context_menu` block of Zellij's own config, not from this plugin.

- Right-click a tab for the menu of that tab, from the `tab` section. Its defaults include *Rename tab*, moving the tab and *Close tab*.
- Right-click anywhere else on the bar, such as a spacer, a marker or the empty end, for the `bar` section. Its defaults include *New pane* and *New tab*.

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

Turning `mouse` on asks for one permission on top of the ones the bar always needs:

| Permission | Used for |
| --- | --- |
| *Execute actions as the user* | Renaming a tab on a double-click |

Switching tabs and opening Zellij's menus also need permission to change Zellij state, which the bar always asks for to add its *zjtabs* settings entry. The full list is in [Permissions](overview.md#permissions).

Zellij asks once, in the plugin's pane, and remembers the answer. Turning `mouse` on after the bar was already granted its usual permissions asks again, for the whole set with the new one.

## Zellij support

The right-click menus use the context menu APIs from [#5685](https://github.com/zellij-org/zellij/pull/5685), which are merged and expected in the next Zellij release. Until then, they need a Zellij build that includes them. See [Zellij support](../../README.md#zellij-support).
