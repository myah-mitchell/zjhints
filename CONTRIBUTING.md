# Contributing

Thanks for taking a look. zjhints is maintained for personal use, so it moves when it needs to rather than on a schedule, but issues and pull requests are welcome. It tracks its own direction and may not take everything.

## Getting set up

You need Rust 1.96.0 with the `wasm32-wasip1` target. The toolchain is pinned in `rust-toolchain.toml`, so rustup will fetch the right one automatically.

```bash
git clone https://github.com/myah-mitchell/zjhints
cd zjhints
make build
```

### Tests need OpenSSL headers

Tests build for the host rather than wasm, which pulls in `zellij-utils -> isahc -> curl -> openssl-sys`. Wasm builds exclude that chain entirely, so this only bites when you run tests:

```bash
sudo apt install libssl-dev    # Debian/Ubuntu
sudo dnf install openssl-devel # Fedora
```

The first host build compiles all of `zellij-utils` and takes a while. After that it is fast.

## Repository layout

The repository is a Cargo workspace. Run every command from its root: `make`, `cargo test` and `cargo build` cover every crate in it, and the built plugins land in `target/wasm32-wasip1/release/` as `zjhints.wasm`, `zjtabs.wasm` and `zjtaskbar.wasm`.

| Path | What it holds |
| --- | --- |
| `crates/zjkit` | The library the plugins share, for code any bar needs rather than code about hints |
| `plugins/zjhints` | The zjhints plugin itself |
| `plugins/zjtabs` | The zjtabs plugin, a tab bar |
| `plugins/zjtaskbar` | The zjtaskbar plugin, a taskbar for floating panes |
| `docs/` | The documentation of every plugin, with the zjtabs pages in `docs/zjtabs/` and the zjtaskbar pages in `docs/zjtaskbar/`, and the images their tests draw |

### The shared crate

zjkit holds what a bar needs whatever it shows. A plugin supplies its own names and options to it: popup and visibility message names through the `popup_names!` and `visibility_names!` macros, and its settings through a `settings::schema::Schema`. The plugin describes itself to the shared bar code in a static `BarSpec` and keeps a `Role` and a `BarCore` in its state. See the docs of `zjkit::bar`.

Every plugin runs as a Zellij shared instance, registered in its `main.rs` with `register_shared_plugin!(zjkit::shared::Shared<State>)`. Zellij runs one instance for all the panes that load the plugin, and `Shared` keeps a separate `State` for each pane, its slot, so a `State` is still written for one pane. Three rules follow from that. Print nothing outside `render`, and make every Zellij call before `render` prints. Make a call about the plugin's own pane through `Host`, which hands it to `Shared` to make for the acting slot. Keep what the slots must share in one place every slot reaches, as zjtaskbar does in `instance.rs`. See the docs of `zjkit::shared`, and [Shared instances](docs/shared-instances.md) for how the instance looks to a user.

Every Zellij call a bar makes goes through the `Host` trait. In the plugin that is `Zellij`, and in tests it is `Recorder`, which makes no calls and keeps a `Call` for each one, so a test can assert what the bar asked Zellij to do. `cfg(test)` is set per crate, so zjkit cannot choose between them. Each plugin makes the choice once, as a `BarHost` alias in its `host.rs`. The popup (`PreviewPopup::load` and its event handling) and the settings screen (`SettingsScreen::load` and saving) call Zellij directly, so tests drive their pure parts rather than those entry points.

| Module | What it does |
| --- | --- |
| `ansi` | Measures and cuts ANSI-styled text by visible width |
| `bar` | What every bar does the same way: its role, the permission flow, visibility and settings requests, and the live preview |
| `config` | Typed reads of a plugin's options: bool, count, fraction and non-empty string |
| `dim` | Fades palette colors and Zellij `Styling` while a session is not where the keyboard goes |
| `fit` | Drops whole items until a line fits, leaving one gap for the indicator, or shows a window of items around a pinned one |
| `format` | Renders zjstatus-style format strings |
| `host` | The `Host` trait over every Zellij call a bar makes, with `Zellij` and the test `Recorder`, including the time of day, where a plugin runs from, running a command, focusing, hiding, moving and closing panes, and opening floating command and plugin panes |
| `images` | The SVG engine behind the documentation images, built only for tests |
| `launch` | Zellij's own launch-or-focus and new floating terminal actions, so a click opens what its default keybindings open |
| `mode` | The current input mode drawn as a prefix, its per-mode options, the look each theme gives it against either end of a bar (`Edge`), and the session name, its mirror image, with the rule for showing it |
| `mouse` | Click maps, column spans for bars of items, press classification, menus of hints or plain labels, the prompt a bar waits on (`PendingPrompt`) and the hover-preview state machine |
| `nesting` | Reads nested-session state from `ModeInfo` |
| `panes` | Focus and widths from a pane manifest |
| `popup` | The floating pane a hover preview opens in |
| `settings` | The settings screen, driven by the plugin's `Schema`, with an add line for each option family, and the check of a `Schema` against its reference page |
| `shared` | Runs a plugin as a Zellij shared instance, a plugin state per slot, routing events, timers, permissions and calls about its own pane to the right slot |
| `theme` | Color themes, format themes and symbol sets |
| `visibility` | Hide, show and toggle messages, and collapsing an empty row |

### The zjhints plugin

| File | What it holds |
| --- | --- |
| `main.rs` | The plugin state, its options, the Zellij event loop, and the plugin's registration as a shared plugin |
| `hints.rs` | Which actions get a hint, their labels, and laying a mode out on one line |
| `keys.rs` | Key grouping, key and modifier aliases, and key order |
| `host.rs` | The `BarHost` alias, permissions, pipe names, and the `BarSpec` |
| `keypress.rs` | Pressing keys in the focused pane, which is how a click reaches a nested session |
| `popup.rs` | zjhints' names for the hover-preview popup |
| `theme.rs` | The hint options each shared theme sets |
| `settings/` | zjhints' option tables and `Schema` |
| `tests.rs`, `images.rs` | The tests, and the documentation images they draw and compare |

### The zjtabs plugin

zjtabs is a library with a binary on top. `main.rs` only registers the library's `State` with Zellij as a shared plugin, so that zjhints' tests can depend on the library and draw zjtabs in its images (see [Images](#images)).

| File | What it holds |
| --- | --- |
| `lib.rs` | The plugin state, its options, the Zellij event loop, and `draw_for_image` for the images |
| `main.rs` | The plugin's registration with Zellij as a shared plugin |
| `tabs.rs` | The tabs, the mode prefix before them and the session name after them, laying them out on one line and the window around the active tab |
| `flags.rs` | The four flags a tab can show and their formats |
| `mouse.rs` | Clicks, the wheel, double-click renaming, middle-click closing, the right-click menus and opening the settings screen from the mode prefix |
| `host.rs` | The `BarHost` alias, permissions, pipe names, and the `BarSpec` |
| `theme.rs` | The tab options each shared theme sets |
| `settings/` | zjtabs' option tables and `Schema` |
| `tests.rs`, `images.rs` | The tests, and the documentation images they draw and compare |

### The zjtaskbar plugin

zjtaskbar is a library with a binary on top, as zjtabs is, so that its `draw_for_image` can be reached from tests elsewhere in the workspace through its `images` feature.

| File | What it holds |
| --- | --- |
| `lib.rs` | The plugin state, its options, the Zellij event loop, and `draw_for_image` for the images |
| `main.rs` | The plugin's registration with Zellij as a shared plugin |
| `entries.rs` | Which floating panes the bar lists, in what order, and what it shows of each |
| `draw.rs` | Laying out the line: the launcher's button, the window of entries around the focused one with its markers, and the tray |
| `flags.rs` | The four flags an entry can show and their formats |
| `frame.rs` | The minimize, maximize and close buttons on pane frames, and which bar puts them on which pane |
| `instance.rs` | What the bars of the instance share: the minimized panes, which panes have a bar, and which bars ask before a frame close |
| `launcher.rs` | The launcher's button, its menu, and the user's own `launch_<n>_*` entries |
| `tray.rs` | The tray's items, and fitting them by dropping items from the left |
| `clock.rs` | The tray's clock: its `strftime` subset, its offset from UTC, and when it ticks |
| `mouse.rs` | Focusing, minimizing and restoring panes, the wheel, double-click renaming, middle-click closing, the pane menu and the tray's clicks |
| `host.rs` | The `BarHost` alias, permissions, pipe names, and the `BarSpec` |
| `theme.rs` | The entry, launcher and tray options each shared theme sets |
| `settings/` | zjtaskbar's option tables and `Schema` |
| `tests.rs`, `images.rs` | The tests, and the documentation images they draw and compare |

### Dependencies

Dependency versions, the toolchain floor (`rust-version`) and any `[patch.crates-io]` override live once in the root `Cargo.toml`. Each crate's own manifest names its dependencies with `workspace = true`. The zjhints version is the one in `plugins/zjhints/Cargo.toml`, and it is what a release is tagged with.

## The loop

```bash
make dev       # build + install into ~/.local/share/zellij/plugins/
```

Then start a new Zellij session. Zellij caches plugins per session, and detaching and reattaching does not reload, so the change simply will not appear.

Before pushing:

```bash
make check     # fmt, clippy, tests, release build (what CI runs)
```

A change to `tooling/` or `builds.toml` also needs the tests of the build tooling, which `make check` does not run. They need Python 3.11 or newer and nothing else:

```bash
python3 -m unittest discover -s tooling
```

## Keeping zellij-tile in step with Zellij

`zellij-tile`, `zellij-tile-utils` and `zellij-utils` must match the Zellij you are running. This is not a nicety, and getting it wrong does not fail the build.

Zellij decodes a binding's actions at the plugin boundary with `.filter_map(|a| a.try_into().ok())`. An action the plugin's older `zellij-tile` does not recognize is silently dropped, and the binding arrives truncated. The symptom is a hint showing the wrong label: everything compiles, tests pass, and nothing logs an error.

So if you upgrade Zellij, bump the crates to match. `update-deps.yml` proposes this as its own pull request once a new Zellij minor is out, but deliberately never merges it for you. See [docs/automation.md](docs/automation.md).

## What good changes look like

### Tests

They run in CI, and again in `builds.yml` before it publishes anything. Anything with logic worth reasoning about, such as ordering, fitting, or label resolution, should come with coverage. The render path is testable end to end: build a synthetic keymap, lay it out with `lay_out_hints_for_mode`, and assert on the visible text. The `rendered` helper in `plugins/zjhints/src/tests.rs` does this, and the tests there show the pattern. Code in zjkit gets its tests in zjkit, against a small test plugin of its own rather than zjhints.

### Comments that explain why

The codebase leans toward explaining reasoning rather than restating the code. If something looks odd but is deliberate, say what would go wrong otherwise. Several of the stranger-looking decisions here exist because of Zellij behavior that is invisible from the code.

### Conventional commit subjects

Release notes are generated from them by git-cliff, so the prefix decides where a change lands in the changelog:

```text
feat(hints): add a thing
fix: stop the bar bleeding color past a truncation
docs: explain the curated list
test: cover the drop ordering
ci: pin actions to SHAs
chore(deps): bump unicode-width
```

### Documentation

New config options need a row in the configuration reference, [docs/configuration.md](docs/configuration.md) for zjhints, [docs/zjtabs/configuration.md](docs/zjtabs/configuration.md) for zjtabs and [docs/zjtaskbar/configuration.md](docs/zjtaskbar/configuration.md) for zjtaskbar, and a prose section explaining them on the page for that topic. A test in each plugin passes its reference and its `Schema` to `zjkit::settings::reference`, so an option in the code and not on the page, or the reverse, fails the build.

### Images

The images in the README and under `docs/` are SVGs in `docs/img/` for zjhints, `docs/img/zjtabs/` for zjtabs and `docs/img/zjtaskbar/` for zjtaskbar, each drawn by the `images.rs` of its plugin, such as `plugins/zjtabs/src/images.rs`, from the code that draws the bar and the settings screen, with the SVG engine in `crates/zjkit/src/images.rs`, so they cannot drift from what the plugin does and need no Zellij or screenshot to make. Do not edit them by hand. After a change that alters how the bar or the settings screen looks, regenerate them and commit the result with the change:

```bash
make images    # rewrites the generated SVGs in docs/img, docs/img/zjtabs and docs/img/zjtaskbar
```

`make images` knows the SVGs a plugin draws by a comment on their second line that names the plugin. It rewrites those, removes the ones that plugin no longer draws, and leaves every other file in `docs/img` alone, including another plugin's images, so an image made by hand can live there too.

The images are tested like everything else. Without `ZJKIT_WRITE_IMAGES` set, `cargo test --all-features` draws them again and compares them with the files, so a stale image fails the test job in CI with a message to run `make images`. `builds.yml` runs the same tests, so a stale image blocks nightly, release and EA builds as well.

To add an image, add it to `images()` in the plugin's `images.rs`, run `make images`, and reference it from the page it belongs on.

The engine is behind zjkit's `images` feature, which only tests enable. A plugin keeps its own drawings, and its test passes them, its name and its image directory to `zjkit::images::write_or_compare_images`. The name goes into the mark of each SVG. The drawing uses Zellij's default keybindings, the Catppuccin Mocha theme for `color_theme "zellij"`, and a fixed cell size, and zjtaskbar's clock shows a fixed sample time, so the output is the same on every machine and every day. Powerline arrows are drawn as triangles and pill ends as half circles, because the image has no Nerd Font.

#### Animated images

An animation is a list of frames, each a `Screen` and how long it shows, written as one SVG that loops them with CSS alone. GitHub renders SVGs in an `<img>`, which runs neither scripts nor most SMIL, but does run CSS animations. To add one:

1. Build an `Animation` from `Frame`s of the same size, naming the frame that stands for the whole: `Animation::new(frames, still)`.
2. Add `animation.to_svg(title)` to `images()` as with a still image, and run `make images`.
3. In the page, give it alt text that tells the whole sequence, since a reader with motion turned off sees only the still frame.

Each row of the screen is written once per distinct look and shown only during its frames, so a frame that changes a few rows costs a few rows. Keep the frames that change everything, such as a resize, few. The session demos in the README, drawn by `plugins/zjhints/src/images/session.rs`, run 16 to 26 seconds each and take 30 to 110 KB, the fitting demo the most for its resize. Under `prefers-reduced-motion: reduce` the animation stops on the still frame, and so does a viewer that applies CSS but does not animate it. A viewer that ignores the `<style>` block altogether draws every frame on top of the others, so check a new image in a browser.

The session demo draws Zellij's pane frames and pane contents by hand. The bar, its hover preview and the settings screen come from zjhints, and the tab bar is zjtabs at its defaults, so change the hand-drawn parts there when Zellij's look changes.

#### Images that draw both plugins

The session demos are zjhints' images: they live in `docs/img`, carry zjhints' mark, and only zjhints' test writes, compares or removes them. Their tab bar comes from `zjtabs::draw_for_image`, which zjhints' tests reach through a dev-dependency on zjtabs with its `images` feature on. Neither release build includes it.

So a change to how zjtabs looks can leave zjhints' images stale as well as its own. `make images` redraws both plugins' images, and `cargo test --all-features` fails on any that are stale, whichever plugin owns them.

## Pull requests

1. Branch from `main`.
2. Make the change, with tests.
3. Run `make check`.
4. Open the pull request. Fill in the template: the "why" matters more than the "what", which the diff already shows.

CI runs formatting, clippy, tests, a wasm build, a flake MSRV check, the tests of the build tooling, and `cargo audit`. All must pass.

You do not need to bump `version` in `Cargo.toml`. A release is made by the maintainer pushing a `vX.Y.Z` tag, which sets the version for that build. See [docs/automation.md](docs/automation.md#cutting-a-release).

Small, focused pull requests are easier to take than large ones. If a change grew while you were making it, splitting it is usually worth the effort.

## Reporting bugs

Hint rendering issues are much easier to act on with the specifics:

- Your Zellij version (`zellij --version`) and the plugin version
- The relevant part of your `config.kdl` (both the plugin block and the keybinds involved)
- What the bar shows versus what you expected
- Whether `discover_hints` is on

Rendering bugs in this project have a habit of being something other than they appear: a glyph that does not render in one terminal, a cached plugin from a previous session, a version mismatch showing up as a wrong label. Concrete details save a lot of guessing.

## Code of conduct

Participation is covered by the [Code of Conduct](CODE_OF_CONDUCT.md).
