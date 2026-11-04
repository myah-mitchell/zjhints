# zjtaskbar launcher

The launcher is a button at the left end of the zjtaskbar bar. Left-click it for a menu of new floating panes to open, the way a desktop taskbar's start button opens programs. This page covers what the menu lists, how to add your own commands and plugins to it, and the permissions it asks for. Every option and its default is in the [configuration reference](configuration.md#launcher).

## The menu

The menu opens at the mouse and lists, in this order:

1. *New terminal*, a floating terminal running your default shell in the focused pane's directory, as Zellij's own `Ctrl p a` opens one.
2. Your own entries, in the order of their numbers.
3. *Session manager*, *Settings* and *Plugin manager*, each opened as Zellij's session-mode keys `w`, `c` and `p` open them.

Choose an item to open it. Press `Esc` or click outside the menu to close it without opening anything. Right-click the button for Zellij's bar menu.

Turn `launcher_new_terminal` off to leave out the first item, and `launcher_zellij_actions` off to leave out the last three. The button is drawn only while `mouse` is on, since it does nothing but take a click, and only while the menu lists something.

## Adding your own entries

Each entry is a set of keys sharing a number `<n>`, set inside the plugin's block like any other option:

| Key | What it holds |
| --- | --- |
| `launch_<n>_name` | What the menu shows; every entry needs one |
| `launch_<n>_command` | The program to run in a new floating pane |
| `launch_<n>_args` | The command's arguments, in one string |
| `launch_<n>_cwd` | The directory the command runs in |
| `launch_<n>_plugin` | The plugin to open in a new floating pane, by URL or alias |
| `launch_<n>_config_<key>` | One key of the plugin's configuration |

An entry sets exactly one of `launch_<n>_command` and `launch_<n>_plugin`, which says what kind of entry it is. This example adds two commands and two plugins:

```kdl
plugins {
    zjtaskbar location="file:~/.local/share/zellij/plugins/zjtaskbar.wasm" {
        launch_1_name "htop"
        launch_1_command "htop"

        launch_2_name "Run the tests"
        launch_2_command "cargo"
        launch_2_args "test --workspace -- --nocapture"
        launch_2_cwd "/home/user/project"

        launch_3_name "Files"
        launch_3_plugin "zellij:strider"

        launch_4_name "Sessions"
        launch_4_plugin "zellij:session-manager"
        launch_4_config_welcome_screen "true"
    }
}
```

The menu then lists *New terminal*, *htop*, *Run the tests*, *Files*, *Sessions*, *Session manager*, *Settings* and *Plugin manager*.

### Numbers and order

Entries are listed in the order of their numbers, so `launch_2_name` comes before `launch_10_name`. The numbers need not run on from each other: leave a gap to make room for an entry later. A number is written without leading zeros, so `launch_01_name` belongs to no entry.

### Arguments

`launch_<n>_args` is split into arguments the way a shell splits a command line:

- Spaces separate arguments, however many there are.
- Single quotes keep everything inside them as written: `'a b'` is one argument.
- Double quotes keep everything inside them too, except that `\"` is a quote and `\\` a backslash.
- Outside quotes, a backslash keeps the next character as written: `a\ b` is one argument.
- Quoted text joins the text around it, so `--name="a b"` is one argument, and `""` is an empty one.

Nothing is expanded: the program starts directly, not through a shell, so `$HOME`, `~`, `*.rs` and `|` reach it as written. To get a shell, run one, as in `launch_1_command "sh"` with `launch_1_args "-c 'ls | wc -l'"`.

### Working directory

A command with no `launch_<n>_cwd` starts in the directory of the focused pane, as a new terminal does. Zellij reads a relative `launch_<n>_cwd` from the directory the bar was started in, and maps a few folders to the bar's own:

| Directory | What Zellij runs the command in |
| --- | --- |
| `/host` | The directory the bar was started in |
| `/data` | The bar's own data folder |
| `/cache` | The bar's own cache folder |
| `/tmp` | Zellij's own temporary folder |

Give a directory outside these four, such as `/home/user/project`, to have it used as written.

### Plugin configuration

Each `launch_<n>_config_<key>` key passes `<key>` to the plugin. In the example, `launch_4_config_welcome_screen "true"` gives the session manager `welcome_screen "true"`. Zellij keeps a few keys for itself and takes them out of any plugin's configuration, among them `name`, `cwd` and `title`, so the plugin never sees those.

### Entries left off the menu

An entry that is not whole is left off the menu, and the rest still show. That is an entry with:

- no `launch_<n>_name`, or one of only spaces;
- neither `launch_<n>_command` nor `launch_<n>_plugin`, or both;
- a `launch_<n>_args` with a quote left open or a lone backslash at its end.

A key with a field the bar does not know, such as `launch_1_icon`, is ignored.

## The button

With no `format_theme`, the button is drawn in Zellij's unselected ribbon colors, in bold. Each built-in shape gives it the shape of an entry, with its symbol in the accent color so it stands apart from the entries. Set `launcher_format` to draw it yourself, with `{symbol}` for the text of `launcher_symbol`.

`launcher_symbol` defaults to the word `Menu`. Each `alias_theme` sets a symbol in its place:

| `alias_theme` | Symbol |
| --- | --- |
| `ascii` | `+` |
| `unicode` | `☰` |
| `nerd-font` | The Material Design apps glyph, U+F003B |

The button dims with the rest of the bar while the session is not receiving input. When the bar is too narrow for everything, the entries give way first; the button is cut only once nothing else is left.

## Permissions

The launcher asks for no permission the mouse has not asked for already, unless one of your entries runs a command:

| Permission | Asked for |
| --- | --- |
| *Execute actions as the user* | For a new terminal and Zellij's own plugins, while either group is on |
| *Start new terminals and plugins* | For the menu, a Zellij prompt, and plugin entries |
| *Run commands* | Only while the menu lists an entry with `launch_<n>_command` |

The launcher asks for none of them while its button is not drawn.
