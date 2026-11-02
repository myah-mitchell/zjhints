# zjtabs

zjtabs is a tab bar for [Zellij](https://github.com/zellij-org/zellij) that you style with the same format strings, themes and settings screen as [zjhints](../../README.md). This page covers what it draws, how to load it as a one-line pane of its own, and the permissions it asks for.

![The zjtabs bar in the Zellij theme's colors: five tabs numbered from 1 as ribbons, the editor tab highlighted, the server tab ending in a dot, and the notes tab ending in (FLOATING)](../img/zjtabs/overview-default.svg)

## What it does

With no options set, zjtabs draws each tab the way Zellij's own tab bar does: a ribbon in the Zellij theme's colors holding the tab's number, its name and its flags, with the active tab highlighted. It works with no options set, and picking a theme restyles it in one line. See [Themes](../themes.md).

- It shows a flag for a tab with a fullscreen pane, synchronized panes, shown floating panes or a bell. See [Flags](formats.md#flags).
- When the tabs do not fit, it keeps a window of them around the active tab and counts the rest in a marker on each side. See [Overflow](overflow.md).
- With the mouse, you click, scroll, double-click and right-click the tabs as in Zellij's own bar. See [Mouse](mouse.md).

## Getting the plugin

Build it from a clone of the repository and copy it into Zellij's plugin directory. The toolchain is described in [Getting set up](../../CONTRIBUTING.md#getting-set-up).

```bash
make build
install -m 644 target/wasm32-wasip1/release/zjtabs.wasm ~/.local/share/zellij/plugins/zjtabs.wasm
```

Zellij caches a plugin for the length of a session, so start a new session to load a new build.

The right-click menus, giving back the row while the bar is hidden, and the settings screen use Zellij APIs that are not in a released Zellij yet. Everything else works on a stock Zellij. See [Zellij support](../../README.md#zellij-support).

## Loading it in a layout

Define zjtabs as an alias in your config, then give it a one-line pane in your layout. The alias is what the settings screen edits. See [Settings screen](../settings.md).

```kdl
plugins {
    zjtabs location="file:~/.local/share/zellij/plugins/zjtabs.wasm"
}
```

```kdl
layout {
    default_tab_template {
        pane size=1 borderless=true {
            plugin location="zjtabs"
        }
        children
    }
}
```

This puts the bar at the top of every tab in place of Zellij's own tab bar. Put the pane after `children` to move it to the bottom, or add a second pane there for [zjhints](../installation.md#option-a-its-own-pane).

Options go inside the alias's braces or inside the pane's `plugin` block, as for zjhints:

```kdl
plugins {
    zjtabs location="file:~/.local/share/zellij/plugins/zjtabs.wasm" {
        color_theme "catppuccin-mocha"
        format_theme "pills"
    }
}
```

Every option and its default is in the [configuration reference](configuration.md).

## Permissions

zjtabs asks for these permissions the first time it runs. Zellij shows the prompt in the plugin's pane and remembers the answer.

| Permission | Used for |
| --- | --- |
| *Access Zellij state (Panes, Tabs and UI)* | Reading the tabs, the input mode and the bar's own pane |
| *Change Zellij state (Panes, Tabs and UI) and run commands* | Switching tabs, opening Zellij's menus and adding the *zjtabs* settings entry |
| *Send messages to and launch other plugins* | The hide, show and toggle messages between bars, and opening the settings screen |
| *Execute actions as the user* | Renaming a tab on a double-click, asked for only with `mouse` on |

With `mouse` off, the bar asks for the first three alone. Turning `mouse` on later asks again, with the fourth added. See [Mouse](mouse.md#permissions).

## Pages

| Page | What it covers |
| --- | --- |
| [Configuration](configuration.md) | Every option zjtabs reads, with its default |
| [Tab formats and flags](formats.md) | The tab placeholders, the flags, spacing, numbering, renaming and the themes for tabs |
| [Overflow](overflow.md) | Fitting the tabs to the bar, the markers for hidden tabs, and cutting a tab that is too wide |
| [Mouse](mouse.md) | Clicking, scrolling, renaming and the right-click menus |
| [Themes](../themes.md) | Color, format and alias themes, shared with zjhints |
| [Settings screen](../settings.md) | Editing the options in a floating pane with a live preview, shared with zjhints |
| [Hiding the bar](../hiding.md) | Hiding and showing the bar from a keybinding or a shell, shared with zjhints |
| [Nested sessions](../nested-sessions.md) | Hiding or dimming the bar in a nested session, shared with zjhints |
