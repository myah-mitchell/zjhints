# Automation

How this repository builds, tests, updates itself and publishes — and the
one-time setup GitHub needs before any of it works.

If you are reading this to get things running, start at
[First-time setup](#first-time-setup). It is four tasks and takes about ten
minutes.

## The shape of it

```
 nightly (04:00 UTC)                you                         result
 ────────────────────               ───                         ──────
 update-deps.yml
   cargo update, nix flake update
   test + build
   bump patch version
   open pull request  ──────────►  email arrives
                                   wait a day or two
                                   click Merge        ──────►  merged
                                   (CI already green)               │
                                                                    │
 release.yml ◄──────────────────────────────────────────────────────┘
   sees a new version in Cargo.toml
   test + build
   tag v0.2.1
   publish release, becomes `latest`
   move the `zellij-<line>` tag to it too

 nightly.yml (05:00 UTC, and on every push to main)
   build main
   move the `nightly` tag
   replace the nightly prerelease

 update-deps.yml's second job, only when a Zellij minor is out
 ───────────────────────────────────────────────────────────
   widen zellij-tile past the minor
   test + build (may fail - that is a real signal here)
   open a separate pull request, titled to say "do not merge yet"
                                   you upgrade your running Zellij first
                                   confirm hints still render right
                                   click Merge        ──────►  merged,
                                                                same path as above
```

Your only interaction is the Merge click.

> **Why not auto-merge on approval?** GitHub does not let you approve your own
> pull requests, and the bot's are opened with your token, so it counts them as
> yours. On a single-maintainer repository a required approval can never be
> satisfied — only bypassed. Merging by hand is the same one click, and it keeps
> the pause: auto-merge would have merged the moment CI went green rather than
> waiting for you to look.

## Workflows

| File | Runs on | Does |
|---|---|---|
| `ci.yml` | every push and pull request | rustfmt, clippy, tests, wasm build, flake MSRV check, `cargo audit` |
| `update-deps.yml` | 04:00 UTC daily, or manually | updates Cargo and flake.lock dependencies, opens/updates a pull request; a second job opens a separate one when a Zellij minor is out |
| `nightly.yml` | 05:00 UTC daily, pushes to `main`, or manually | rebuilds `main`, moves the `nightly` release |
| `release.yml` | pushes to `main`, `v*.*.*` tags, or manually | publishes a release when `Cargo.toml` names an untagged version |
| `cleanup-caches.yml` | a pull request closes | deletes that PR's Actions caches |

### Tags and releases

- **`vX.Y.Z`** — one per release, permanent. `/releases/latest/download/…`
  resolves to the newest of these.
- **`nightly`** — a single moving tag, force-updated each night. Published as a
  prerelease so it never becomes `latest`.
- **`zellij-<line>`** — one per Zellij minor this project has ever targeted
  (e.g. `zellij-0.44`), force-updated by `release.yml` every time a release
  ships for that line. Not a prerelease — it is a real release, just aliased —
  but `make_latest: false` keeps it from contending with `latest`. See
  [the README's Versioning section](../README.md#versioning) for why this
  exists.

There is deliberately no tag named `latest`. GitHub already tracks the newest
release, and a real tag by that name would have to be force-pushed on every
release for no gain.

### Versioning

Releases are patch bumps: `update-deps.yml` increments the patch number as part
of the pull request it opens, so merging one lands a new version on `main` and
`release.yml` publishes it.

Minor and major bumps are yours to make. Edit `version` in `Cargo.toml`, merge
that to `main`, and the same machinery publishes it — the workflow only asks
whether the version in `Cargo.toml` has been tagged yet, not how it got there.

By convention, a minor bump marks a large feature change and/or a move to a
new Zellij major/minor (e.g. 0.3.x → 0.4.x alongside zellij-tile 0.44 → 0.45)
— see [the README's Versioning section](../README.md#versioning). The
`zellij-upgrade` pull request below still only proposes a patch bump; when
the Zellij move is what's driving it, bumping to a minor instead — once
you've confirmed hints render right — is a manual edit on that branch before
merging.

### How dependencies are chosen

`cargo update` only moves within the range each `Cargo.toml` entry allows. For
`0.x` crates that is patch releases, so `zellij-tile = "0.45.0"` accepts 0.45.1
but never 0.46.0.

That is the policy on purpose. **A `zellij-tile` newer than the Zellij you run
breaks hints silently.** Zellij's plugin boundary decodes a binding's actions
with `.filter_map(|a| a.try_into().ok())`, so an action the plugin does not
recognise is dropped and the binding arrives truncated. The plugin builds,
tests pass, and hints render with wrong labels. There is no error anywhere.

`.github/scripts/check_deps.py` asks crates.io what exists and reports anything
held back. When a Zellij crate has a new minor, `update-deps.yml`'s second job
(`zellij-upgrade`) proposes taking it — its own pull request, on its own
branch (`deps/zellij-upgrade`), widening `zellij-tile`/`zellij-tile-utils`'s
requirement to the new version and re-resolving just those two crates. It is
labelled **`needs-zellij-upgrade`** and its title says so too.

**This one is never safe to merge on a green build alone.** CI passing only
means the plugin still compiles and its own tests pass — it says nothing about
whether *your* running Zellij matches. Upgrade Zellij first, confirm hints
still render right, then merge. Once merged, `release.yml` publishes it as a
normal release and moves the matching `zellij-<line>` tag (see
[Tags and releases](#tags-and-releases)) to it.

**`flake.lock` moves alongside it.** `nix flake update` has no equivalent
restraint — Nix flake inputs carry no semver range to stay within, so every
input (`nixpkgs`, `rust-overlay`, `crane`, …) always moves to whatever is
current. The workflow only opens the pull request after confirming
`nix build .#default` still resolves a toolchain meeting `Cargo.toml`'s
`rust-version` (see `flake.nix`, and [#11][gh-11]) — a failure there fails the
run instead of landing a broken lock.

[gh-11]: https://github.com/myah-mitchell/zjstatus-hints/issues/11

## First-time setup

### 1. Create the automation token

A pull request opened with the built-in `GITHUB_TOKEN` does not start any
workflows. GitHub does this to prevent loops, but with required status checks
on `main` it means those checks sit pending forever and the pull request can
never be merged. A personal access token avoids that.

1. Go to **Settings → Developer settings → Personal access tokens →
   Fine-grained tokens** (on your account, not the repository).
2. **Generate new token**.
3. Name it `zjstatus-hints automation`. Set an expiry you will notice —
   90 days is reasonable; you will get an email before it lapses.
4. Under **Repository access**, choose **Only select repositories** and pick
   `zjstatus-hints`.
5. Under **Permissions → Repository permissions**, set:
   - **Contents**: Read and write
   - **Pull requests**: Read and write
   - **Workflows**: Read and write
6. Generate it and copy the value — it is shown once.
7. In the repository: **Settings → Secrets and variables → Actions →
   New repository secret**. Name it exactly `AUTOMATION_TOKEN`, paste the
   value, save.

`update-deps.yml` checks for this first and fails with an explanation if it is
missing, rather than opening a pull request whose checks can never pass.

### 2. Let Actions open pull requests

**Settings → Actions → General → Workflow permissions**:

- Select **Read and write permissions**.
- Tick **Allow GitHub Actions to create and approve pull requests**.
- Save.

### 3. Protect `main`

**Settings → Rules → Rulesets → New branch ruleset**:

- Name: `main`
- Enforcement status: **Active**
- Target branches: **Include default branch**
- Tick **Require a pull request before merging**
  - Required approvals: **0**
- Tick **Require status checks to pass**
  - Add: `rustfmt`, `clippy`, `test`, `build (wasm)`, `flake (msrv)`
- Leave **Require branches to be up to date** off, or a busy day means
  rebasing before every merge.

> The status check names must match the `name:` of each job in `ci.yml`. They
> only appear in the picker after a workflow has run once, so push a branch and
> let CI run before coming back here.

**Required approvals is 0 deliberately.** GitHub will not let you approve your
own pull requests, and the bot opens its ones with your token, so it treats
those as yours too. Setting 1 would mean nothing could ever merge without
bypassing the rule you just wrote. The status checks are the gate that actually
does work here.

Also decide whether to tick **Do not allow bypassing the above settings**.
Leaving it unticked lets you push directly to `main` when you need to; ticking
it means even you go through a pull request.

### 4. Get the email

**Your avatar → Settings → Notifications**:

- Under **Subscriptions → Watching**, make sure email is enabled.
- On the repository page, **Watch → All Activity** (or **Custom → Pull
  requests** for less noise).

Test it with **Actions → Update dependencies → Run workflow**. If dependencies
are already current the job stops early without opening anything, which is also
a useful signal that the plumbing works.

## Merging

The email arrives when the nightly finds updates. What to look at:

- The pull request body lists what moved and what was held back.
- **If it is titled "Zellij … is out"**, this is the `zellij-upgrade` pull
  request, not the routine one. Do not merge it until the Zellij you run
  matches and you have confirmed hints still render right — its body says the
  same. A red Checks tab on this one specifically can mean the plugin needs
  real source changes for the new zellij-tile, not just a version bump.
- Otherwise, CI runs on the pull request; tests and a release build also
  passed *before* it was opened, so a red pull request means CI found
  something the update job did not.

Click **Merge**. That publishes the release, moves `latest` (and `zellij-<line>`
if this was the `zellij-upgrade` pull request), and rebuilds the nightly.

Nothing merges on its own, so leaving one open for a few days costs nothing.

To stop one: **Close** it. The branch is reused, so the next run reopens with
whatever is current — nothing is lost by closing one you dislike.

## Installing what is published

```sh
make latest              # newest tagged release
make nightly             # tonight's build of main
make zellij VERSION=0.44 # newest release built for that Zellij line
```

All three fetch straight into the Zellij plugin path. Start a new session to
load it — Zellij caches plugins per session, and detaching does not reload.

Pointing your Zellij config at a nightly URL does not work well: Zellij caches
remote plugins by URL, so it keeps serving whatever it downloaded first. Fetch
locally instead.

## Maintenance

**Pin the actions.** All of them are pinned to commit SHAs — a version tag can
be moved by whoever controls the action, so an unpinned `uses:` is a supply
chain gap. If a new workflow step adds one without network access to verify a
SHA, it should carry a `# TODO: pin to a SHA` comment until this fixes it:

```sh
gh api repos/<owner>/<repo>/commits/<tag> --jq .sha
```

Then replace `@<tag>` with `@<sha> # <tag>`, keeping the version in the
trailing comment. Dependabot updates the pinned ones monthly.

**Rotate the token** when the expiry email arrives — regenerate and update the
`AUTOMATION_TOKEN` secret. `update-deps.yml` fails loudly if it lapses.

**Watch for `needs-zellij-upgrade`.** Those accumulate rather than merge, and
they are the ones that matter.

## When something breaks

| Symptom | Cause |
|---|---|
| Update workflow fails immediately | `AUTOMATION_TOKEN` missing or expired |
| Pull request opens but no checks run | Opened with `GITHUB_TOKEN`; the token is not being picked up |
| Merge button is blocked | A required status check has not passed — check the PR's Checks tab |
| Release does not publish after merge | Version in `Cargo.toml` already tagged — check the run's `Resolve version` step |
| `cargo test` fails to link | OpenSSL headers missing; the workflows install `libssl-dev`, locally use your package manager |
| Nightly is stale | Check the `nightly.yml` schedule ran; scheduled workflows are paused after 60 days of repository inactivity |
| `zellij-upgrade` pull request never appears | `zellij_minor_available` only goes true once crates.io has the new `zellij-tile`/`zellij-tile-utils`, which can lag a Zellij release by a day or so |
| `zellij-<line>` did not move after a release | Check `Cargo.toml`'s `zellij-tile` requirement at that commit — the tag follows whatever line was pinned *at release time*, not the newest one available |

That last one is worth knowing: **GitHub disables scheduled workflows in
repositories with no activity for 60 days**, and emails you when it does. Any
push re-enables them.
