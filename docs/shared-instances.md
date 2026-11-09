# Shared instances

zjhints and zjtabs each run as one shared Zellij plugin instance: a single running copy of the plugin serves every pane that loads it. This page covers what that means for a layout, what the bars of an instance share, and the `instance` option that keeps bars apart.

## Requirements

The plugins need Zellij 0.46 or newer. Until it is released, use the early-access build they are compiled against, [fork-ea-v0.46.7-261103](https://github.com/myah-mitchell/zellij/releases/tag/fork-ea-v0.46.7-261103). See [Zellij support](../README.md#zellij-support).

## One instance for every bar

Zellij starts one instance for each plugin location. Every pane that loads the location joins it, in every tab and whichever alias or inline options the pane uses. A layout with a zjhints bar in each of ten tabs runs zjhints once.

Each bar keeps its own options. Two bars from different aliases, or one with `show_mode true` set in the layout and one without, each behave as they would alone. A bar in a new tab joins the instance and starts from its own options, as its layout and alias give them, not from changes the settings screen is previewing on the other bars. See [Closing with unsaved changes](settings.md#closing-with-unsaved-changes).

Aliases that name different locations, such as a release URL and a local file, run as separate instances. So do panes that give the plugin different `instance` values (see [Keeping bars apart](#keeping-bars-apart)).

## What the bars share

- Permissions. Zellij asks once for the instance, in the pane of one of its bars, and the answer covers every bar, the settings screen and the hover popups. Only one prompt is up at a time, and a bar that needs more waits for the prompt up to be answered, then asks for the whole set. Denying a prompt, such as the settings screen's, takes the grant away from every bar of the instance.
- Messages. A hide, show or toggle message such as `zjhints::toggle` reaches every bar of the instance, including one loaded in the background with `load_plugins` and one in a tab you are not looking at. See [Hiding the bar](hiding.md).
- The settings screen and the hover popup. Both open as panes of the bar's instance, and the screen's previews reach only the bars of its own instance. See [Settings screen](settings.md#what-a-preview-reaches).

Zellij runs an instance as a unit, so a crash in the plugin stops every bar of that instance together.

## Keeping bars apart

Give panes different `instance` values to run them as separate instances. The value is any string, and a pane without one belongs to the instance with no value. This runs the second alias apart from the first:

```kdl
plugins {
    zjhints location="file:~/.local/share/zellij/plugins/zjhints.wasm"
    zjhints-work location="file:~/.local/share/zellij/plugins/zjhints.wasm" {
        instance "work"
    }
}
```

Separate instances differ in two ways:

- Hide, show and toggle messages without a payload still reach every bar, since Zellij delivers them to every running plugin.
- A settings screen previews only on the bars of its own instance.

Permissions don't keep them apart. Each instance asks in a pane of its own, but Zellij remembers the answer by the plugin's location, not by instance, so a grant in one covers the other without a prompt, and a later answer in either replaces the remembered one. To keep permissions apart, load the second alias from a copy of the wasm file at another path.

`instance` is read by Zellij rather than by the plugins, so it has no row in the configuration references. The settings screen lists it under *Other* when an alias sets it, and keeps it when you save. Zellij reads it only as a pane loads, so a preview never moves a running bar to another instance.

### Opening the settings screen

A screen opened from Zellij's settings screen, or from a click on the mode prefix, joins the instance of the bar it was opened from. A keybinding or a `context_menu` item launches it from the alias, so it joins the alias's instance. If only a layout pane sets `instance`, add the same line to the action, beside `settings true`:

```kdl
LaunchOrFocusPlugin "zjhints" {
    floating true
    move_to_focused_tab true
    settings true
    instance "work"
}
```
