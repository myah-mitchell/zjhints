# zjtaskbar

zjtaskbar is a taskbar for [Zellij](https://github.com/zellij-org/zellij)'s floating panes that you style with the same format strings, themes and settings screen as [zjhints](../../README.md) and [zjtabs](../zjtabs/overview.md). This page covers what it draws, how to load it as a one-line pane of its own, and the permissions it asks for.

![The zjtaskbar bar in the Zellij theme's colors: a Menu button for the launcher, then four floating panes. The htop entry is a ribbon, the editor entry is highlighted as the focused one, and the notes entry is plain text ending in (MINIMIZED). After a line between the tabs, the man zellij entry ends in (docs), the tab it is in. The rest of the bar is blank, since the tray is empty by default](../img/zjtaskbar/overview-default.svg)

## What it does

With no options set, zjtaskbar lists every floating pane of the session as an entry, the way a desktop taskbar lists windows: a ribbon in the Zellij theme's colors holding the pane's title and its flags, with the focused pane highlighted. A launcher button starts the bar. It works with no options set, and picking a theme restyles it in one line. See [Themes](../themes.md).

- It lists terminal and plugin panes alike, grouped by tab in the order the tabs stand and ordered by pane id within a tab, so entries stay where they are as the focus moves. Tiled panes and the bars of the instance are never listed. See [Entries](configuration.md#entries).
- It lists the panes of every tab, marking the ones in another tab with its name, or of the active tab only with `pane_scope "tab"`. See [Flags](formats.md#flags).
- It flags a pane that is minimized, whose command has exited, that runs a plugin, or that is in another tab. See [Flags](formats.md#flags).
- When the entries do not fit, it keeps a window of them around the focused one and counts the rest in a marker on each side. See [Overflow](overflow.md).
- With the mouse, you click an entry to focus its pane or bring a minimized one back where it stood, click the focused one to minimize it, double-click to rename, middle-click to close after confirming, and right-click for a menu of what can be done to the pane. See [Mouse](mouse.md).
- The launcher button opens a menu of new floating panes: a terminal, your own commands and plugins, and Zellij's session manager, settings and plugin manager. See [Launcher](launcher.md).
- The tray at the right end can show the clock, the session's name and the input mode. It is empty until you list an item. See [Tray](tray.md).

## Getting the plugin

Build it from a clone of the repository and copy it into Zellij's plugin directory. The toolchain is described in [Getting set up](../../CONTRIBUTING.md#getting-set-up).

```bash
make build
install -m 644 target/wasm32-wasip1/release/zjtaskbar.wasm ~/.local/share/zellij/plugins/zjtaskbar.wasm
```

Zellij caches a plugin for the length of a session, so start a new session to load a new build.

The launcher's menu, the pane menu, the close prompt, the right-click bar menu, giving back the row while the bar is empty, and the settings screen use Zellij APIs that are not in a released Zellij yet. Closing a pane with a middle click needs a Zellij build that sends plugins middle clicks, such as the one zjtaskbar is compiled against. Everything else works on a stock Zellij. See [Zellij support](../../README.md#zellij-support) and [Mouse](mouse.md#zellij-support).

## Loading it in a layout

Define zjtaskbar as an alias in your config, then give it a one-line pane in your layout. The alias is what the settings screen edits. See [Settings screen](../settings.md).

```kdl
plugins {
    zjtaskbar location="file:~/.local/share/zellij/plugins/zjtaskbar.wasm"
}
```

```kdl
layout {
    default_tab_template {
        children
        pane size=1 borderless=true {
            plugin location="zjtaskbar"
        }
    }
}
```

This puts the bar at the bottom of every tab, the way a desktop taskbar sits. The pane in each tab is a bar of one running zjtaskbar, a [shared instance](../shared-instances.md), so every bar lists and restores the same minimized panes. Put the pane before `children` to move it to the top. To run it beside [zjtabs](../zjtabs/overview.md#loading-it-in-a-layout) or [zjhints](../installation.md#option-a-its-own-pane), give each bar a pane of its own in the same template.

Options go inside the alias's braces or inside the pane's `plugin` block, as for zjhints:

```kdl
plugins {
    zjtaskbar location="file:~/.local/share/zellij/plugins/zjtaskbar.wasm" {
        color_theme "catppuccin-mocha"
        format_theme "pills"
        tray "session clock"
    }
}
```

Every option and its default is in the [configuration reference](configuration.md).

## Permissions

zjtaskbar asks for these permissions the first time it runs. Zellij shows one prompt for all its bars, in the pane of one of them, and remembers the answer.

| Permission | Used for | Asked for |
| --- | --- | --- |
| *Access Zellij state (Panes, Tabs and UI)* | Reading the panes, the tabs, the input mode and the bar's own pane | Always |
| *Change Zellij state (Panes, Tabs and UI) and run commands* | Adding the *zjtaskbar* settings entry; with `mouse` on, focusing, minimizing, moving and closing panes and opening Zellij's bar menu | Always |
| *Send messages to and launch other plugins* | Opening the settings screen | Always |
| *Execute actions as the user* | Renaming a pane, opening the session manager from the tray, and the launcher's new terminal and Zellij screens | With `mouse` on |
| *Start new terminals and plugins* | The pane menu, the close prompt and the launcher's menu, which are Zellij prompts, and the launcher's plugin entries | With `mouse` on |
| *Run commands* | Running `date +%z` for the clock's offset from UTC, and the launcher's command entries | While the tray has a clock with no `clock_utc_offset`, or the launcher lists a command |

With the defaults, the bar asks for the first five: `mouse` is on, and the tray is empty, so nothing runs a command. Turn `mouse` off to leave out the last two of those. Changing an option that needs a new permission asks again, for the whole set with the new one added. See [Mouse](mouse.md#permissions) and [Permissions](tray.md#permissions) in the tray's page.

## Known limits

- A pane minimized before the plugin reloads stays hidden and is not listed again. See [Minimizing](mouse.md#minimizing).
- A pinned floating pane may come back unpinned after it is minimized and restored. See [Minimizing](mouse.md#minimizing).
- A pane pulled out of a tab that is not on screen keeps that tab's frame buttons until the tab is shown again. See [Frame buttons](mouse.md#frame-buttons).
- With `clock` in the tray, the bar asks for *Run commands* to run `date +%z`, since a plugin cannot read the system's time zone. Set `clock_utc_offset` to do without it. See [The time zone](tray.md#the-time-zone).
- While the clock asks the system for its offset, a change of daylight saving time shows within the hour rather than at once. See [The time zone](tray.md#the-time-zone).

## Pages

| Page | What it covers |
| --- | --- |
| [Configuration](configuration.md) | Every option zjtaskbar reads, with its default |
| [Entry formats and flags](formats.md) | The entry placeholders, the flags, spacing, the empty bar, the launcher's button and the themes for entries |
| [Overflow](overflow.md) | Fitting the entries to the bar, the markers for hidden entries, cutting an entry that is too wide, and how the entries share the line with the button and the tray |
| [Mouse](mouse.md) | Focusing, minimizing, renaming and closing panes, the wheel and the menus |
| [Launcher](launcher.md) | The launcher's menu, your own commands and plugins in it, and its button |
| [Tray](tray.md) | The clock, the session's name and the mode at the right end, and the clock's time zone |
| [Themes](../themes.md) | Color, format and alias themes, shared with zjhints and zjtabs |
| [Settings screen](../settings.md) | Editing the options in a floating pane with a live preview, shared with zjhints and zjtabs |
| [Hiding the bar](../hiding.md) | Hiding and showing the bar from a keybinding or a shell, shared with zjhints and zjtabs |
| [Nested sessions](../nested-sessions.md) | Hiding or dimming the bar in a nested session, shared with zjhints and zjtabs |
