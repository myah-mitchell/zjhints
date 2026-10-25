# zjhints

A [Zellij](https://github.com/zellij-org/zellij) plugin that displays context-aware key bindings for the current mode, styled and configured to match the rest of your status bar. It runs standalone in its own pane, or pipes its output into [zjstatus](https://github.com/dj95/zjstatus) for setups that already use it. See [Installation](docs/installation.md) to set it up.

> [!IMPORTANT]
> Four of this release's features need Zellij APIs that are merged but not in a released Zellij yet. Everything else works on a stock Zellij, including 0.45.1. See [Zellij support](#zellij-support).

<!-- Separates the two alerts; adjacent blockquotes would merge into one. -->

> [!NOTE]
> zjhints began as a fork of [b0o/zjstatus-hints](https://github.com/b0o/zjstatus-hints) by Maddison Cohodas, and was itself named zjstatus-hints until 0.5.0. It has gone its own way since, so everything below describes zjhints as it stands rather than how it differs.

Here it is at the bottom of a Zellij session: the mode first, then each key beside what it does. The narration under the dashed line names each key pressed; it is not part of the session.

![A Zellij session with the zjhints bar at the bottom. The bar shows the normal mode hints, then the pane mode hints after Ctrl p. After a split and a trip to tab mode, the terminal narrows and the bar drops whole hints to fit, ending in an ellipsis; hovering the ellipsis opens a popup listing the hints that did not fit. Alt comma opens the settings screen in a floating pane, where format_theme pills, color_theme dracula and alias_theme unicode restyle the bar one by one as they are picked, and Ctrl a saves them. A nested Zellij session started in the right pane takes the keyboard: the host's frames fade and the bar shows the nested session's hints, including its pane mode. Back in the host, Ctrl g locks the session and the bar shows only the key that unlocks it.](docs/img/session-demo.svg)

[Themes](docs/themes.md) restyle it in one line, and the [settings screen](docs/settings.md) edits every option while you watch the bar change.

## Zellij support

Four features in this release need plugin APIs that are not in a released Zellij yet. All of them are merged and expected in the next Zellij release:

| Zellij PR                                               | What it gives zjhints                                                                                                                                                                                                                                                  |
| ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [#5589](https://github.com/zellij-org/zellij/pull/5589) | A nested session reports its input mode and key bindings to its host, which is what draws [the hints of the session you have descended into](docs/nested-sessions.md#while-descended-into-a-nested-session)                                                            |
| [#5590](https://github.com/zellij-org/zellij/pull/5590) | A plugin can hand its layout row back while it has nothing to draw, which is what [collapse_when_empty](docs/nested-sessions.md#giving-the-row-back) uses                                                                                                              |
| [#5685](https://github.com/zellij-org/zellij/pull/5685) | Zellij's context menu, prompt and popup APIs, which [mouse](docs/mouse.md) uses for the right-click menu, the menus of hints and their hover previews, and the config APIs the [settings screen](docs/settings.md) reads, previews and saves the zjhints alias through |
| [#5729](https://github.com/zellij-org/zellij/pull/5729) | A config save edits the alias in place, keeping the file's comments, blank lines and spaces inside values, which is how the [settings screen](docs/settings.md#previewing-and-saving) leaves the rest of your config as it was                                         |

Until they are released, a Zellij build carrying all of them is published as [fork-v0.46.4](https://github.com/myah-mitchell/zellij/releases/tag/fork-v0.46.4). That is the build zjhints is compiled against, so it is the way to see these features working today.

Without them the plugin still runs, including on Zellij 0.45.1. Hints, styling, labels, ordering and fitting all behave normally, and the first two features above are absent rather than broken. `mouse` is off by default and should stay off there, and the settings screen needs the build above until the next Zellij release. See [Versioning](docs/installation.md#versioning) for which zjhints line targets which Zellij.

## Rationale

Zellij's built-in status bar shows keybinding hints for your current mode, but you lose them the moment you replace it with something else. This plugin restores that functionality as its own independent piece: run it in its own pane for a drop-in replacement, or, if you already run [zjstatus](https://github.com/dj95/zjstatus), pipe the hints into it instead of standing up a separate pane.

## What it does

zjhints shows the keys the mode you are in accepts: a [curated list](docs/hints.md#the-curated-list) of the ones people reach for most, followed by everything else your config binds. It works with no configuration. [Installation](docs/installation.md) sets it up, and every option and its default is in the [configuration reference](docs/configuration.md).

Each part of it has its own page:

| Page                                                     | What it covers                                                                                                     |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| [Installation](docs/installation.md)                     | Adding it to Zellij, release channels, versioning, and showing the hints in a pane or in zjstatus                  |
| [Standing alone](docs/nested-sessions.md#standing-alone) | Running in its own pane with the current mode prefixed onto the line, no zjstatus required                         |
| [Styling](docs/styling.md)                               | Global and per-hint format strings, colors, and key and modifier aliases, so `Ctrl Left` can read `^←`             |
| [Themes](docs/themes.md)                                 | Colors, a shape such as pills or Powerline arrows, and a symbol set, each picked on its own and overridable        |
| [Hints](docs/hints.md)                                   | The curated list, discovery of every other keybinding your config enables, and shared bindings                     |
| [Labels](docs/labels.md)                                 | Renaming, hiding and merging hints, globally or in a single mode                                                   |
| [Keys and ordering](docs/ordering.md)                    | Which keys show for a hint, keyboard-layout-aware key order, and a pinned order for the hints                      |
| [Fitting the bar](docs/fitting.md)                       | Dropping whole hints to fit the terminal as it narrows, in an order you choose, with an indicator for what was cut |
| [Mouse](docs/mouse.md)                                   | Right-click for Zellij's context menu, clicking a hint to run it, and menus of the hints that did not fit          |
| [Settings screen](docs/settings.md)                      | Editing every option in a floating pane, seeing each change on the bar, and saving it to your config file          |
| [Hiding the bar](docs/hiding.md)                         | Hiding and showing it from a keybinding or a shell, giving its row back while hidden                               |
| [Nested sessions](docs/nested-sessions.md)               | Showing the nested session's hints while you are in it, and hiding or dimming the host's own                       |

For a config in daily use rather than one written for the docs, see the [Zellij config](https://github.com/myah-mitchell/dotfiles/blob/main/zellij/.config/zellij/config.kdl) and the [layout](https://github.com/myah-mitchell/dotfiles/blob/main/zellij/.config/zellij/layouts/default.kdl) that loads it in myah-mitchell's dotfiles. Every page in `docs/` is listed in the [documentation overview](docs/overview.md).

## Known issues

Five RUSTSEC advisories are open against the dependency tree. All are warning-level, covering unmaintained or unsound crates rather than vulnerabilities, and none is reachable in the wasm plugin that actually ships. `cargo audit` passes because these are warnings.

| Advisory                             | Crate              | Reaches this project via                     |
| ------------------------------------ | ------------------ | -------------------------------------------- |
| RUSTSEC-2021-0139                    | `ansi_term`        | `zellij-tile-utils`                          |
| RUSTSEC-2021-0145, RUSTSEC-2024-0375 | `atty`             | `clap 3` and `clap_derive` in `zellij-utils` |
| RUSTSEC-2024-0370                    | `proc-macro-error` | `clap 3` and `clap_derive` in `zellij-utils` |
| RUSTSEC-2026-0221                    | `event-listener`   | `isahc` in `zellij-utils`                    |

Being a proc-macro crate, proc-macro-error never ships in any compiled output, wasm or host. The `isahc -> curl -> openssl-sys` chain is absent from the `wasm32-wasip1` dependency graph entirely, and appears only on the host target that `cargo test` builds.

None of them can be shed from here. Swapping this project's own ansi_term for nu-ansi-term would only add a second ANSI crate, since zellij-tile-utils still pulls ansi_term in, and the clap 3 and isahc advisories only clear when Zellij itself moves off those crates. Shedding them is a job for a later Zellij.

## Contributing

Issues and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for the development loop, and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for expected conduct. Security problems should go through [SECURITY.md](SECURITY.md) rather than a public issue.

How this repository builds, tests and releases itself is described in [docs/automation.md](docs/automation.md).

## From the maintainer

I, Myah, can't speak for any of the other contributors but much of the code, tests, and documentation from my work were completed with AI assistance. I review every line and every change is built and tested before anything is released. For any bugs or issues that may make it into a release, whether I fully wrote every line or if AI was used, the responsibility is on myself as the maintainer and not the tools I used to complete the work.

I complete understand how some feel about the use of AI software, and I do share a number of those same views. However, for a project like this, I would have never even considered approaching it with my limited amount of free time. I share my work in the hope that someone else finds it as useful as I do.

## License

&copy; 2026 Myah Mitchell &copy; 2025 Maddison Cohodas

A fork of [zjstatus-hints](https://github.com/b0o/zjstatus-hints) by Maddison Cohodas, itself adapted from the built-in Zellij status-bar plugin by Brooks J Rady.

MIT License
