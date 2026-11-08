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

The repository is a Cargo workspace. Run every command from its root: `make`, `cargo test` and `cargo build` cover every crate in it, and the built plugin lands at `target/wasm32-wasip1/release/zjhints.wasm`.

| Path | What it holds |
| --- | --- |
| `crates/zjkit` | The library the plugins share, for code any bar needs rather than code about hints |
| `plugins/zjhints` | The zjhints plugin itself |
| `docs/` | The zjhints documentation and the images its tests draw |

### The shared crate

zjkit holds what a bar needs whatever it shows. A plugin supplies its own names and options to it: popup and visibility message names through the `popup_names!` and `visibility_names!` macros, and its settings through a `settings::schema::Schema`. Host calls the plugin's tests need to stub out stay in the plugin, because `cfg(test)` is per crate and a stub in the plugin does not reach zjkit. The popup (`PreviewPopup::load` and its event handling) and the settings screen (`SettingsScreen::load` and saving) call Zellij directly, so tests drive their pure parts rather than those entry points.

| Module | What it does |
| --- | --- |
| `ansi` | Measures and cuts ANSI-styled text by visible width |
| `config` | Typed reads of a plugin's options: bool, count, fraction and non-empty string |
| `dim` | Fades palette colors and Zellij `Styling` while a session is not where the keyboard goes |
| `fit` | Drops whole items until a line fits, leaving one gap for the indicator |
| `format` | Renders zjstatus-style format strings |
| `mouse` | Click maps, menus and the hover-preview state machine |
| `nesting` | Reads nested-session state from `ModeInfo` |
| `panes` | Focus and widths from a pane manifest |
| `popup` | The floating pane a hover preview opens in |
| `settings` | The settings screen, driven by the plugin's `Schema`, with an add line for each option family, and the check of a `Schema` against its reference page |
| `theme` | Color themes, format themes and symbol sets |
| `visibility` | Hide, show and toggle messages, and collapsing an empty row |

### The zjhints plugin

| File | What it holds |
| --- | --- |
| `main.rs` | The plugin state, its options, and the Zellij event loop |
| `hints.rs` | Which actions get a hint, their labels, and laying a mode out on one line |
| `keys.rs` | Key grouping, key and modifier aliases, and key order |
| `mode_prefix.rs` | The mode drawn ahead of the hints |
| `host.rs` | Permissions, pipe names, and the host calls tests stub out |
| `keypress.rs` | Pressing keys in the focused pane, which is how a click reaches a nested session |
| `popup.rs` | zjhints' names for the hover-preview popup |
| `theme.rs` | The hint options each shared theme sets |
| `settings/` | zjhints' option tables and `Schema` |
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

New config options need a row in the [configuration reference](docs/configuration.md) and a prose section explaining them on the page for that topic. A test in each plugin passes its reference and its `Schema` to `zjkit::settings::reference`, so an option in the code and not on the page, or the reverse, fails the build.

### Images

The images in the README and under `docs/` are SVGs in `docs/img/`, drawn by `plugins/zjhints/src/images.rs` from the code that draws the bar and the settings screen, so they cannot drift from what the plugin does and need no Zellij or screenshot to make. Do not edit them by hand. After a change that alters how the bar or the settings screen looks, regenerate them and commit the result with the change:

```bash
make images    # rewrites the generated SVGs in docs/img
```

`make images` knows the SVGs it draws by a comment on their second line. It rewrites those, removes the ones it no longer draws, and leaves every other file in `docs/img` alone, so an image made by hand can live there too.

The images are tested like everything else. Without `ZJHINTS_WRITE_IMAGES` set, `cargo test --all-features` draws them again and compares them with the files, so a stale image fails the test job in CI with a message to run `make images`. `builds.yml` runs the same tests, so a stale image blocks nightly, release and EA builds as well. To add an image, add it to `images()` in `plugins/zjhints/src/images.rs`, run `make images`, and reference it from the page it belongs on. The drawing uses Zellij's default keybindings, the Catppuccin Mocha theme for `color_theme "zellij"`, and a fixed cell size, so the output is the same on every machine. Powerline arrows are drawn as triangles and pill ends as half circles, because the image has no Nerd Font.

#### Animated images

An animation is a list of frames, each a `Screen` and how long it shows, written as one SVG that loops them with CSS alone. GitHub renders SVGs in an `<img>`, which runs neither scripts nor most SMIL, but does run CSS animations. To add one:

1. Build an `Animation` from `Frame`s of the same size, naming the frame that stands for the whole: `Animation::new(frames, still)`.
2. Add `animation.to_svg(title)` to `images()` as with a still image, and run `make images`.
3. In the page, give it alt text that tells the whole sequence, since a reader with motion turned off sees only the still frame.

Each row of the screen is written once per distinct look and shown only during its frames, so a frame that changes a few rows costs a few rows. Keep the frames that change everything, such as a resize, few. The session demos in the README, drawn by `plugins/zjhints/src/images/session.rs`, run 16 to 26 seconds each and take 30 to 110 KB, the fitting demo the most for its resize. Under `prefers-reduced-motion: reduce` the animation stops on the still frame, and so does a viewer that applies CSS but does not animate it. A viewer that ignores the `<style>` block altogether draws every frame on top of the others, so check a new image in a browser.

The session demo draws Zellij's tab bar, pane frames and pane contents by hand. Only the bar, its hover preview and the settings screen come from the plugin, so change the hand-drawn parts there when Zellij's look changes.

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
