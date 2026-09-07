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
   open pull request  ──────────►  email arrives
                                   wait a day or two
                                   click Merge        ──────►  merged,
                                                                no release yet

                                   ...whenever you decide to release...
                                   bump `version` in Cargo.toml
                                   push to main (or push a v*.*.* tag)
                                                        │
 release.yml ◄──────────────────────────────────────────┘
   sees a new version in Cargo.toml (or the pushed tag)
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
                                                                same path as above:
                                                                no release until
                                                                you bump the version
```

Merging an automated pull request only ever lands the update on `main`.
Releasing is a separate, deliberate step you take on your own schedule; see
[Versioning](#versioning).

> **Why not auto-merge on approval?** Automated pull requests are opened by a
> GitHub App with its own bot identity (see [First-time setup](#first-time-setup)),
> not your account, so you can review and approve them like anyone else's. The
> pause is kept anyway: auto-merge would merge the moment CI went green rather
> than waiting for you to look, and on dependency updates specifically that
> is a worse trade than one click.

## Workflows

| File | Runs on | Does |
|---|---|---|
| `ci.yml` | every push and pull request | rustfmt, clippy, tests, wasm build, flake MSRV check, `cargo audit` |
| `update-deps.yml` | 04:00 UTC daily, or manually | updates Cargo and flake.lock dependencies, opens/updates a pull request; a second job opens a separate one when a Zellij minor is out |
| `nightly.yml` | 05:00 UTC daily, pushes to `main`, or manually | rebuilds `main`, moves the `nightly` release |
| `release.yml` | pushes to `main`, `v*.*.*` tags, or manually | publishes a release when `Cargo.toml` names an untagged version |
| `beta.yml` | manually only | builds any ref you name and publishes/replaces an `ea-<label>` prerelease (see [EA/beta releases](#eabeta-releases)) |
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
- **`ea-<label>`**: one per EA/beta channel you've ever named, force-updated
  by `beta.yml` each time you run it with that label. Always a prerelease.
  See [EA/beta releases](#eabeta-releases).

There is deliberately no tag named `latest`. GitHub already tracks the newest
release, and a real tag by that name would have to be force-pushed on every
release for no gain.

### Versioning

Every version bump is yours to make, deliberately. Neither `update-deps.yml`
pull request touches `version` in `Cargo.toml`: the routine one only moves
`cargo update`/`nix flake update`'s output, and the `zellij-upgrade` one only
widens the `zellij-tile`/`zellij-tile-utils` requirement. Merging either lands
the update on `main`; nothing publishes until you decide it should.

To release: edit `version` in `Cargo.toml` (on the update branch before
merging it, on a follow-up commit, or directly on `main` if your ruleset
allows it) and push that to `main`. `release.yml` picks it up from there, and
only checks whether the version in `Cargo.toml` has been tagged yet, not how
it got there. Pushing a `v*.*.*` tag yourself works too, and does not require
touching `Cargo.toml` or waiting on a push to `main` at all.

Patch, minor, or major is your call each time; see
[the README's Versioning section](../README.md#versioning) for the convention
this project uses (a minor bump marks a large feature change and/or a move to
a new Zellij major/minor, e.g. 0.3.x → 0.4.x alongside zellij-tile 0.44 →
0.45). A Zellij line move landed via a `zellij-upgrade` pull request is a
natural candidate for a minor bump specifically; make it while merging, once
you've confirmed hints render right.

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
still render right, then merge. Merging alone does not publish anything (see
[Versioning](#versioning)); bump `version` in `Cargo.toml` when you do, and
`release.yml` publishes it as a normal release and moves the matching
`zellij-<line>` tag (see [Tags and releases](#tags-and-releases)) to it.

**`flake.lock` moves alongside it.** `nix flake update` has no equivalent
restraint — Nix flake inputs carry no semver range to stay within, so every
input (`nixpkgs`, `rust-overlay`, `crane`, …) always moves to whatever is
current. The workflow only opens the pull request after confirming
`nix build .#default` still resolves a toolchain meeting `Cargo.toml`'s
`rust-version` (see `flake.nix`, and [#11][gh-11]) — a failure there fails the
run instead of landing a broken lock.

[gh-11]: https://github.com/myah-mitchell/zjstatus-hints/issues/11

### Depending on an unreleased upstream fix

Sometimes a branch needs a `zellij-tile`/`zellij-tile-utils` fix that exists
in a fork or an unmerged upstream PR but has not shipped to crates.io yet.
Widening the `[dependencies]` requirement to a version crates.io does not
have yet just fails the build (`cargo` reports "candidate versions found"
and lists everything *but* the one you asked for). Lowering it back defeats
the point of taking the fix at all.

Instead, add a `[patch.crates-io]` block at the end of `Cargo.toml`, pointing
the affected crate(s) at the fork/branch that has the fix, and leave the
`[dependencies]` requirement exactly as it already is:

```toml
[patch.crates-io]
zellij-tile = { git = "https://github.com/<you>/zellij", tag = "<tag-or-branch>" }
zellij-tile-utils = { git = "https://github.com/<you>/zellij", tag = "<tag-or-branch>" }
```

This only works if the crate's *own* declared version (in its `Cargo.toml`,
or `[workspace.package].version` for a workspace like zellij's) still
satisfies the requirement your `Cargo.toml` already has: Cargo patches the
*source* a requirement resolves from, not the requirement itself. If the
fork bumped its version past what you require, widen the requirement to
match at the same time.

Then regenerate the lock against the new source and confirm it builds:

```sh
cargo update -p zellij-tile -p zellij-tile-utils
make check && cargo test --all-features
```

`zellij-utils` does not need its own patch entry: zellij's workspace
depends on it via a path (see `zellij-utils = { path = "zellij-utils/",
version = "..." }` in zellij's `Cargo.toml`), so it resolves from the same
git checkout automatically.

**This does not disturb the normal pipeline.** `release.yml`'s
Zellij-compatibility-line logic and `check_deps.py` both read the version
*requirement* string in `[dependencies]`, never the patch, so tags, the
release job, and versioning all keep working unmodified while a patch is
active. `check_deps.py`/the `zellij-upgrade` job in `update-deps.yml` will
go quiet for the patched crate specifically: the locked version already
looks at or ahead of whatever crates.io has, so there is nothing for it to
propose. That is expected, not a sign the check is broken.

**Removing it once the fix ships:** delete the `[patch.crates-io]` block,
confirm `[dependencies]`'s requirement still matches (or widen it to the
real published version if the fork was ahead), then
`cargo update -p zellij-tile -p zellij-tile-utils` to move the lock back
onto crates.io. From there it is a normal dependency again, and a normal
release once you bump `version` (see [Versioning](#versioning)).

### EA/beta releases

`beta.yml` builds and tests whatever ref you give it and publishes it as a
prerelease, without touching `Cargo.toml`'s `version` or going anywhere
near `release.yml`. Use it to hand an early build to testers: a branch
that still needs a `[patch.crates-io]` override like the one above, or any
other work in progress not ready for a real release.

Run it from **Actions → Beta → Run workflow**, or `gh workflow run beta.yml
-f ref=<branch> -f label=<short-name>`. Two inputs:

- **`ref`**: the branch, tag, or commit to build.
- **`label`**: names the channel. The tag becomes `ea-<label>`, force-moved
  each time you run it with that label, the same way `nightly` is moved
  each night. Reuse a label to replace that channel's release (e.g. after
  pushing a fix); pick a new one to keep two EA builds around side by side
  (`ea-nested-sessions`, `ea-zellij-0.46`, …).

Install one with `make ea LABEL=<short-name>` (see [Installing what is
published](#installing-what-is-published)).

Because it only runs on `workflow_dispatch`, nothing about this competes
with `nightly.yml` or `release.yml`: it is a side channel you reach for
on demand, and the same recipe works for the next unreleased-dependency
situation, not just this one.

## First-time setup

### 1. Create the automation app

A pull request opened with the built-in `GITHUB_TOKEN` does not start any
workflows. GitHub does this to prevent loops, but with required status checks
on `main` it means those checks sit pending forever and the pull request can
never be merged. A token is what avoids that, and a **GitHub App** installation
token specifically (rather than a personal access token) is what keeps the
resulting pull requests from being attributed to your own account. An App is
registered once on your account and can be installed on every repo you
maintain, so if you already have one from another project, skip to installing
it on this repo instead of registering a second one.

1. Go to **Settings → Developer settings → GitHub Apps** (on your account,
   not the repository) → **New GitHub App**.
2. Name it something you'd recognise across repos, e.g. `myah-mitchell-bot`
   (this becomes its `[bot]` handle on every PR and commit it makes).
3. Homepage URL: anything works; your GitHub profile is fine.
4. **Webhook**: untick **Active**; nothing here needs one.
5. Under **Permissions → Repository permissions**, set:
   - **Contents**: Read and write
   - **Pull requests**: Read and write
6. **Where can this GitHub App be installed?**: your choice; **Only on this
   account** is fine for a personal-repo bot.
7. **Create GitHub App**. On the app's page, note the **App ID**, then
   **Generate a private key**, which downloads a `.pem` file, shown once.
8. **Install App** (left sidebar) → your account → select `zjstatus-hints`
   (and any other repos you want it on now; add more later from the same
   page without repeating steps 1–7).
9. In the repository: **Settings → Secrets and variables → Actions →
   New repository secret**, twice:
   - `AUTOMATION_APP_ID`: the App ID from step 7.
   - `AUTOMATION_APP_PRIVATE_KEY`: the full contents of the `.pem` file.

`update-deps.yml` checks for both of these first and fails with an explanation
if either is missing, rather than opening a pull request whose checks can
never pass.

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

**Required approvals is 0 here**, but unlike a personal-access-token setup,
it does not have to be: the app's pull requests belong to its own `[bot]`
identity, not yours, so GitHub will let you approve them like anyone else's.
0 is still the default above because the status checks are the gate doing the
actual work; set it to 1 instead if you want the extra click of an explicit
approval before merging.

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

Click **Merge**. That lands the update on `main`; the next nightly build picks
it up. It does not publish a release on its own; see [Versioning](#versioning)
for that step, which is separate and entirely up to you.

Nothing merges on its own, so leaving one open for a few days costs nothing.

To stop one: **Close** it. The branch is reused, so the next run reopens with
whatever is current — nothing is lost by closing one you dislike.

## Installing what is published

```sh
make latest              # newest tagged release
make nightly             # tonight's build of main
make zellij VERSION=0.44 # newest release built for that Zellij line
make ea LABEL=<name>     # a specific EA/beta channel someone published
```

All of these fetch straight into the Zellij plugin path. Start a new session
to load it: Zellij caches plugins per session, and detaching does not
reload.

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

**Rotate the private key** if it is ever exposed: on the app's settings page,
generate a new one, update the `AUTOMATION_APP_PRIVATE_KEY` secret, then
delete the old key from the app. Unlike a personal access token, this key has
no expiry to track, so there is nothing to rotate on a schedule.

**Watch for `needs-zellij-upgrade`.** Those accumulate rather than merge, and
they are the ones that matter.

## When something breaks

| Symptom | Cause |
|---|---|
| Update workflow fails immediately | `AUTOMATION_APP_ID`/`AUTOMATION_APP_PRIVATE_KEY` missing, or the app is not installed on this repo |
| Pull request opens but no checks run | Opened with `GITHUB_TOKEN`; the app token is not being picked up |
| Merge button is blocked | A required status check has not passed — check the PR's Checks tab |
| Release does not publish after merging an update PR | Expected: merging never bumps `version`; see [Versioning](#versioning) |
| Release does not publish after a version bump | Version in `Cargo.toml` already tagged; check the run's `Resolve version` step |
| `cargo test` fails to link | OpenSSL headers missing; the workflows install `libssl-dev`, locally use your package manager |
| Nightly is stale | Check the `nightly.yml` schedule ran; scheduled workflows are paused after 60 days of repository inactivity |
| `zellij-upgrade` pull request never appears | `zellij_minor_available` only goes true once crates.io has the new `zellij-tile`/`zellij-tile-utils`, which can lag a Zellij release by a day or so |
| `zellij-<line>` did not move after a release | Check `Cargo.toml`'s `zellij-tile` requirement at that commit — the tag follows whatever line was pinned *at release time*, not the newest one available |
| `beta.yml` fails at "Sanitize the label" | The `label` input had no `[a-z0-9-]` characters left after sanitizing; pick a label with at least one letter or digit |
| `beta.yml`'s build fails to resolve a Zellij crate | The ref you built doesn't have a `[patch.crates-io]` override for a requirement crates.io can't satisfy yet (see [Depending on an unreleased upstream fix](#depending-on-an-unreleased-upstream-fix)) |

That last one is worth knowing: **GitHub disables scheduled workflows in
repositories with no activity for 60 days**, and emails you when it does. Any
push re-enables them.
