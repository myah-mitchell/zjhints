# Hiding the bar

The bar can be hidden and shown again from a keybinding, the right-click menu or a shell, without touching the configuration. A hidden bar draws nothing, sends zjstatus an empty string, and gives its row back to the panes around it. This page covers the messages that do it and the `hidden` option for starting hidden.

## From a keybinding

Zjhints listens for three pipe messages: `zjhints::toggle`, `zjhints::hide` and `zjhints::show`. Bind one with Zellij's `MessagePlugin` action, leaving out a plugin name so the message reaches every plugin already running:

```kdl
keybinds {
    shared_except "locked" {
        bind "Alt h" { MessagePlugin { name "zjhints::toggle"; } }
    }
}
```

Pick any free key. Hiding lasts until the bar is shown again or the session ends; a new session starts from the [`hidden` option](#starting-hidden).

A bar opened later, in a new tab, asks the running bars of its alias whether they are hidden and follows the first answer, so it joins them. With no bar running to ask, it starts from the `hidden` option.

A toggle flips each bar on its own. Bars that end up out of step, such as two aliases hidden separately, stay that way through a toggle; send `zjhints::hide` or `zjhints::show` to bring them all to one state.

The same goes for a bar that is already blank for another reason, such as [`hide_in_base_mode`](configuration.md#hints) in the base mode: a toggle hides it with nothing to see, and it stays hidden after the mode changes. Bind `zjhints::hide` and `zjhints::show` to separate keys if that gets confusing.

Don't name the plugin in the action, as in `MessagePlugin "zjhints" { ... }`. Zellij delivers a message addressed that way only to an instance whose configuration matches exactly, and a bar given its own settings in a layout does not match, so Zellij starts a new instance instead.

## Hiding one alias's bars

With no payload, a message reaches every bar in the session, on every tab. Give it a payload naming a `plugins {}` alias and only the bars started from that alias respond. This hides a nested session's bar while leaving the host's alone:

```kdl
bind "Alt H" { MessagePlugin { name "zjhints::toggle"; payload "zjhints-nested"; } }
```

A bar loaded with `load_plugins` to feed zjstatus runs in a hidden pane of its own, so it knows its alias and responds the same way. Only a bar that has just started, before Zellij first reports the panes, cannot tell its alias yet, and it responds to every message in that moment.

## From the right-click menu

With [`mouse`](mouse.md) on, add an item to the `bar` section of Zellij's `context_menu` block:

```kdl
context_menu {
    bar {
        item "Hide hints" {
            MessagePlugin { name "zjhints::hide"; }
        }
    }
}
```

The menu belongs to the bar. On a Zellij with [#5590](https://github.com/zellij-org/zellij/pull/5590) the hidden bar gives its row back, leaving nothing to right-click, so bring it back with a keybinding or a shell. Without #5590 the blank row stays and still opens the menu, so a `zjhints::show` item works there too.

## From a shell

`zellij pipe` sends the same messages from a script or another terminal. Use `zjhints::hide` and `zjhints::show` when the script needs a known end state rather than a toggle:

```bash
zellij pipe --name zjhints::hide
zellij pipe --name zjhints::show -- zjhints-nested
```

The text after `--` is the payload, naming the alias as above.

When stdin is not a terminal, as in cron, a systemd unit or `ssh host cmd`, `zellij pipe` also sends each line it reads from stdin as a payload, and waits until stdin ends. Redirect it from `/dev/null` in scripts:

```bash
zellij pipe --name zjhints::hide -- zjhints-nested </dev/null
```

## Starting hidden

Set `hidden true` in the alias to start every bar hidden. A message shows it as usual.

Set it only alongside a `zjhints::show` or `zjhints::toggle` keybinding. A hidden bar has no mode prefix to click for the [settings screen](settings.md), so the keybinding, `zellij pipe` or editing the config file are the only ways back.

```kdl
zjhints location="https://github.com/myah-mitchell/zjhints/releases/latest/download/zjhints.wasm" {
    hidden true
}
```

Changing `hidden` on the [settings screen](settings.md) hides or shows the bars straight away, like any other option previewed there.

## The row

A bar in its own pane gives its row back while hidden even with [`collapse_when_empty`](nested-sessions.md#giving-the-row-back) off, and takes it back when shown. That option only decides what happens when the bar has nothing to draw by itself. Giving the row back needs a Zellij with [#5590](https://github.com/zellij-org/zellij/pull/5590); without it the row stays, blank.
