# Contributing

Thanks for taking a look. This is a fork of [b0o/zjstatus-hints](https://github.com/b0o/zjstatus-hints) maintained for personal use, so it moves when it needs to rather than on a schedule, but issues and pull requests are welcome.

If the change belongs upstream more than here, consider opening it there instead; this fork tracks its own direction and may not take everything.

## Getting set up

You need Rust 1.96.0 with the `wasm32-wasip1` target. The toolchain is pinned in `rust-toolchain.toml`, so rustup will fetch the right one automatically.

```sh
git clone https://github.com/myah-mitchell/zjstatus-hints
cd zjstatus-hints
make build
```

**Tests need OpenSSL headers.** They build for the host rather than wasm, which pulls in `zellij-utils` → `isahc` → `curl` → `openssl-sys`. Wasm builds exclude that chain entirely, so this only bites when you run tests:

```sh
sudo apt install libssl-dev    # Debian/Ubuntu
sudo dnf install openssl-devel # Fedora
```

The first host build compiles all of `zellij-utils` and takes a while. After that it is fast.

## The loop

```sh
make dev       # build + install into ~/.local/share/zellij/plugins/
```

Then **start a new Zellij session**. Zellij caches plugins per session, and detaching and reattaching does not reload, so the change simply will not appear.

Before pushing:

```sh
make check     # fmt, clippy, tests, release build (what CI runs)
```

## Keeping zellij-tile in step with Zellij

`zellij-tile` and `zellij-tile-utils` must match the Zellij you are running. This is not a nicety, and getting it wrong does not fail the build.

Zellij decodes a binding's actions at the plugin boundary with `.filter_map(|a| a.try_into().ok())`. An action the plugin's older `zellij-tile` does not recognize is silently **dropped**, and the binding arrives truncated. The symptom is a hint showing the wrong label: everything compiles, tests pass, and nothing logs an error.

So if you upgrade Zellij, bump the crates to match. `update-deps.yml` proposes this as its own pull request once a new Zellij minor is out, but deliberately never merges it for you. See [docs/AUTOMATION.md](docs/AUTOMATION.md).

## What good changes look like

**Tests.** There are 81 and they run in CI. Anything with logic worth reasoning about (ordering, fitting, label resolution) should come with coverage. The render path is testable end to end: build a synthetic keymap, call `render_hints_for_mode`, and assert on the visible text. Look at the existing tests in `src/main.rs` for the pattern.

**Comments that explain why.** The codebase leans toward explaining reasoning rather than restating the code. If something looks odd but is deliberate, say what would go wrong otherwise: several of the stranger-looking decisions here exist because of Zellij behavior that is invisible from the code.

**Conventional commit subjects.** Release notes are generated from them by git-cliff, so the prefix decides where a change lands in the changelog:

```
feat(hints): add a thing
fix: stop the bar bleeding color past a truncation
docs: explain the curated list
test: cover the drop ordering
ci: pin actions to SHAs
chore(deps): bump unicode-width
```

**Documentation.** New config options need a bullet in the README's Configuration list *and* a prose section explaining them. The list is checked against the options the code actually reads.

## Pull requests

1. Branch from `main`.
2. Make the change, with tests.
3. Run `make check`.
4. Open the pull request. Fill in the template: the "why" matters more than the "what", which the diff already shows.

CI runs formatting, clippy, tests, a wasm build, a flake MSRV check, and `cargo audit`. All must pass.

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
