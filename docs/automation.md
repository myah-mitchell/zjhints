# Automation

How this repository builds, tests, updates itself and publishes, and the one-time setup GitHub needs before any of it works.

If you are reading this to get things running, start at [First-time setup](#first-time-setup). It is four tasks and takes about ten minutes.

## Contents

- [The shape of it](#the-shape-of-it)
- [Placeholders](#placeholders)
- [Workflows](#workflows)
- [First-time setup](#first-time-setup)
- [Merging](#merging)
- [Installing what is published](#installing-what-is-published)
- [Maintenance](#maintenance)
- [When something breaks](#when-something-breaks)

## The shape of it

```text
 nightly (04:00 UTC)                you                         result
 --------------------               ---                         ------
 update-deps.yml
   cargo update, nix flake update
   test + build
   open pull request  ---------->  email arrives
                                   wait a day or two
                                   click Merge        ------>  merged,
                                                                no release yet

                                   ...whenever you decide to release...
                                   bump `version` in Cargo.toml
                                   push to main (or push a v*.*.* tag)
                                                        |
 release.yml <------------------------------------------+
   sees a new version in Cargo.toml (or the pushed tag)
   test + build
   tag v0.5.0
   publish release, becomes `latest`
   move the `zellij-<line>` tag to it too

 nightly.yml (05:00 UTC, and on every push to main)
   build main
   move the `nightly` tag
   replace the nightly prerelease

 update-deps.yml's second job, only when a Zellij minor is out
 -----------------------------------------------------------
   widen zellij-tile past the minor
   test + build (may fail - that is a real signal here)
   open a separate pull request, titled to say "do not merge yet"
                                   you upgrade your running Zellij first
                                   confirm hints still render right
                                   click Merge        ------>  merged,
                                                                same path as above:
                                                                no release until
                                                                you bump the version
```

Merging an automated pull request only ever lands the update on `main`. Releasing is a separate, deliberate step you take on your own schedule; see [Versioning](#versioning).

### Why not auto-merge on approval

Automated pull requests are opened by a GitHub App with its own bot identity (see [First-time setup](#first-time-setup)), not your account, so you can review and approve them like anyone else's. The pause is kept anyway: auto-merge would merge the moment CI went green rather than waiting for you to look, and on dependency updates specifically that is a worse trade than one click.

## Placeholders

Several commands and tag names below take a value you supply, written in angle brackets where it appears.

| Placeholder | Where its value comes from |
| --- | --- |
| `<line>` | A Zellij minor this project has targeted, such as `0.44` |
| `<label>` | An EA channel name you choose when you run `beta.yml` |
| `<branch>` | The branch, tag, or commit you want `beta.yml` to build |
| `<asset>` | A file published on a release, currently only `zjhints.wasm` |
| `<you>` | The GitHub account owning the fork you are patching against |
| `<tag-or-branch>` | The ref in that fork to build from |
| `<owner>` | The GitHub account owning an action you are pinning |
| `<repo>` | That action's repository name |
| `<tag>` | The version tag on that action you are replacing |
| `<sha>` | The commit that tag resolves to |

## Workflows

| File | Runs on | Does |
| --- | --- | --- |
| `ci.yml` | every push and pull request | rustfmt, clippy, tests, wasm build, flake MSRV check, `cargo audit` |
| `update-deps.yml` | 04:00 UTC daily, or manually | updates Cargo and flake.lock dependencies, opens/updates a pull request; a second job opens a separate one when a Zellij minor is out |
| `nightly.yml` | 05:00 UTC daily, pushes to `main`, or manually | rebuilds `main`, moves the `nightly` release |
| `release.yml` | pushes to `main`, `v*.*.*` tags, or manually | publishes a release when `Cargo.toml` names an untagged version |
| `beta.yml` | manually only | builds any ref you name and publishes/replaces an `ea-<label>` prerelease (see [EA/beta releases](#eabeta-releases)) |
| `cleanup-caches.yml` | a pull request closes | deletes that PR's Actions caches |

### Tags and releases

- `vX.Y.Z`: one per release, permanent. `/releases/latest/download/<asset>` resolves to the newest of these.
- `nightly`: a single moving tag, force-updated each night. Published as a prerelease so it never becomes `latest`.
- `zellij-<line>`: one per Zellij minor this project has ever targeted, such as `zellij-0.44`, force-updated by `release.yml` every time a release ships for that line. It is a real release rather than a prerelease, just aliased, but `make_latest: false` keeps it from contending with `latest`. See [the README's Versioning section](../README.md#versioning) for why this exists.
- `ea-<label>`: one per EA/beta channel you have ever named, force-updated by `beta.yml` each time you run it with that label. Always a prerelease. See [EA/beta releases](#eabeta-releases).

There is deliberately no tag named `latest`. GitHub already tracks the newest release, and a real tag by that name would have to be force-pushed on every release for no gain.

### Versioning

Every version bump is yours to make, deliberately. Neither pull request from `update-deps.yml` touches `version` in `Cargo.toml`. The routine one only moves the output of `cargo update` and `nix flake update`, and the zellij-upgrade one only widens the zellij-tile and zellij-tile-utils requirement. Merging either lands the update on `main`; nothing publishes until you decide it should.

To release, edit `version` in `Cargo.toml` and push that to `main`. You can do this on the update branch before merging it, on a follow-up commit, or directly on `main` if your ruleset allows it. `release.yml` picks it up from there, and only checks whether the version in `Cargo.toml` has been tagged yet, not how it got there. Pushing a `v*.*.*` tag yourself works too, and does not require touching `Cargo.toml` or waiting on a push to `main` at all.

Patch, minor, or major is your call each time. A Zellij line move landed via a `zellij-upgrade` pull request is a natural candidate for a minor bump specifically; make it while merging, once you have confirmed hints render right. See [the README's Versioning section](../README.md#versioning) for the convention this project uses.

### How dependencies are chosen

`cargo update` only moves within the range each `Cargo.toml` entry allows. For `0.x` crates that is patch releases, so `zellij-tile = "0.45.0"` accepts 0.45.1 but never 0.46.0.

That is the policy on purpose.

> [!WARNING]
> A zellij-tile newer than the Zellij you run breaks hints silently. Zellij's plugin boundary decodes a binding's actions with `.filter_map(|a| a.try_into().ok())`, so an action the plugin does not recognize is dropped and the binding arrives truncated. The plugin builds, tests pass, hints render with wrong labels, and there is no error anywhere.

`.github/scripts/check_deps.py` asks crates.io what exists and reports anything held back. When a Zellij crate has a new minor, the second job in `update-deps.yml`, named zellij-upgrade, proposes taking it. It opens its own pull request on its own branch, `deps/zellij-upgrade`, widening the zellij-tile and zellij-tile-utils requirement to the new version and re-resolving just those two crates. It is labeled `needs-zellij-upgrade` and its title says so too.

That pull request is never safe to merge on a green build alone. CI passing only means the plugin still compiles and its own tests pass; it says nothing about whether the Zellij you run matches. Upgrade Zellij first, confirm hints still render right, then merge. Merging alone does not publish anything (see [Versioning](#versioning)). Bump `version` in `Cargo.toml` when you do. `release.yml` then publishes it as a normal release and moves the matching [`zellij-<line>` tag](#tags-and-releases) to it.

#### flake.lock moves alongside it

`nix flake update` has no equivalent restraint: Nix flake inputs carry no semver range to stay within, so every input always moves to whatever is current, `nixpkgs`, `rust-overlay` and `crane` included. The workflow only opens the pull request after confirming `nix build .#default` still resolves a toolchain meeting `Cargo.toml`'s `rust-version`; a failure there fails the run instead of landing a broken lock. See `flake.nix` and [issue #11][gh-11].

[gh-11]: https://github.com/myah-mitchell/zjhints/issues/11

### Depending on an unreleased upstream fix

Sometimes a branch needs a zellij-tile or zellij-tile-utils fix that exists in a fork or an unmerged upstream pull request but has not shipped to crates.io yet. Widening the `[dependencies]` requirement to a version crates.io does not have yet just fails the build, with `cargo` reporting "candidate versions found" and listing everything except the one you asked for. Lowering it back defeats the point of taking the fix at all.

Instead, add a `[patch.crates-io]` block at the end of `Cargo.toml`, pointing the affected crates at the fork or branch that has the fix, and leave the `[dependencies]` requirement exactly as it already is:

```toml
[patch.crates-io]
zellij-tile = { git = "https://github.com/<you>/zellij", tag = "<tag-or-branch>" }
zellij-tile-utils = { git = "https://github.com/<you>/zellij", tag = "<tag-or-branch>" }
```

This only works if the crate's own declared version still satisfies the requirement your `Cargo.toml` already has. Cargo patches the source a requirement resolves from, not the requirement itself. That declared version lives in the crate's own `Cargo.toml`, or in `[workspace.package].version` for a workspace like zellij's. If the fork bumped its version past what you require, widen the requirement to match at the same time.

Then regenerate the lock against the new source and confirm it builds:

```bash
cargo update -p zellij-tile -p zellij-tile-utils
make check && cargo test --all-features
```

`zellij-utils` does not need its own patch entry. Zellij's workspace depends on it via a path, written as `zellij-utils = { path = "zellij-utils/", version = "..." }` in zellij's `Cargo.toml`, so it resolves from the same git checkout automatically.

#### What a patch does not disturb

`release.yml`'s Zellij-compatibility-line logic and `check_deps.py` both read the version requirement string in `[dependencies]`, never the patch, so tags, the release job, and versioning all keep working unmodified while a patch is active.

`check_deps.py` and the `zellij-upgrade` job go quiet for the patched crate specifically: the locked version already looks at or ahead of whatever crates.io has, so there is nothing for it to propose. That is expected, not a sign the check is broken.

#### Removing a patch once the fix ships

Delete the `[patch.crates-io]` block, confirm the `[dependencies]` requirement still matches, widening it to the real published version if the fork was ahead, then run `cargo update -p zellij-tile -p zellij-tile-utils` to move the lock back onto crates.io. From there it is a normal dependency again, and a normal release once you bump `version`. See [Versioning](#versioning).

### EA/beta releases

`beta.yml` builds and tests whatever ref you give it and publishes it as a prerelease. It never touches `Cargo.toml`'s `version`, and never goes near `release.yml`. Use it to hand an early build to testers: a branch that still needs a `[patch.crates-io]` override like the one above, or any other work in progress not ready for a real release.

Open *Actions > Beta* and click **Run workflow**, or run `gh workflow run beta.yml -f ref=<branch> -f label=<label>`. Two inputs:

| Input | What it does |
| --- | --- |
| `ref` | The branch, tag, or commit to build |
| `label` | Names the channel. The tag becomes `ea-<label>` |

The `ea-<label>` tag is force-moved each time you run it with that label, the same way `nightly` is moved each night. Reuse a label to replace that channel's release, after pushing a fix for instance; pick a new one to keep two EA builds around side by side, such as `ea-nested-sessions` and `ea-zellij-0.46`.

Install one with `make ea LABEL=<label>`. See [Installing what is published](#installing-what-is-published).

Because it only runs on `workflow_dispatch`, nothing about this competes with `nightly.yml` or `release.yml`. It is a side channel you reach for on demand, and the same recipe works for the next unreleased-dependency situation, not just this one.

## First-time setup

### 1. Create the automation app

A pull request opened with the built-in `GITHUB_TOKEN` does not start any workflows. GitHub does this to prevent loops, but with required status checks on `main` it means those checks sit pending forever and the pull request can never be merged. A token is what avoids that, and a GitHub App installation token specifically, rather than a personal access token, is what keeps the resulting pull requests from being attributed to your own account.

An App is registered once on your account and can be installed on every repo you maintain. If you already have one from another project, skip to installing it on this repo rather than registering a second one.

1. Open *Settings > Developer settings > GitHub Apps* on your account, not the repository, and click **New GitHub App**.
2. Name it something you would recognize across repos, such as `myah-mitchell-bot`. This becomes its `[bot]` handle on every pull request and commit it makes.
3. In *Homepage URL*, enter anything; your GitHub profile is fine.
4. Under *Webhook*, untick **Active**. Nothing here needs one.
5. Under *Permissions > Repository permissions*, set *Contents* to **Read and write**, and *Pull requests* to **Read and write**.
6. Under *Where can this GitHub App be installed?*, choose **Only on this account** for a personal-repo bot, or the wider option if you prefer.
7. Click **Create GitHub App**. On the app's page, note the *App ID*, then click **Generate a private key**, which downloads a `.pem` file shown only once.
8. Click **Install App** in the left sidebar, choose your account, and select `zjhints`. You can add more repos later from the same page without repeating steps 1 to 7.
9. In the repository, open *Settings > Secrets and variables > Actions* and click **New repository secret** twice: once for `AUTOMATION_APP_ID`, holding the App ID from step 7, and once for `AUTOMATION_APP_PRIVATE_KEY`, holding the full contents of the `.pem` file.

`update-deps.yml` checks for both secrets first and fails with an explanation if either is missing, rather than opening a pull request whose checks can never pass.

### 2. Let Actions open pull requests

1. Open *Settings > Actions > General > Workflow permissions*.
2. Select **Read and write permissions**.
3. Tick **Allow GitHub Actions to create and approve pull requests**.
4. Click **Save**.

### 3. Protect `main`

1. Open *Settings > Rules > Rulesets* and click **New branch ruleset**.
2. In *Name*, enter `main`.
3. Set *Enforcement status* to **Active**.
4. Under *Target branches*, choose **Include default branch**.
5. Tick **Require a pull request before merging**, and set *Required approvals* to **0**.
6. Tick **Require status checks to pass**, then add each of these checks:

   ```text
   rustfmt   clippy   test   build (wasm)   flake (msrv)
   ```

7. Leave **Require branches to be up to date** unticked, or a busy day means rebasing before every merge.

> [!NOTE]
> Status check names must match the `name:` of each job in `ci.yml`, and they only appear in the picker after a workflow has run once. Push a branch and let CI run before coming back here.

Required approvals is 0 above because the status checks are the gate doing the actual work. Unlike a personal-access-token setup it does not have to be: the app's pull requests belong to its own `[bot]` identity, not yours, so GitHub will let you approve them like anyone else's. Set it to 1 instead if you want the extra click of an explicit approval before merging.

Also decide whether to tick **Do not allow bypassing the above settings**. Leaving it unticked lets you push directly to `main` when you need to; ticking it means even you go through a pull request.

### 4. Get the email

1. Open *Settings > Notifications* from your avatar menu.
2. Under *Subscriptions > Watching*, make sure email is enabled.
3. On the repository page, click **Watch** and choose **All Activity**, or **Custom > Pull requests** for less noise.

Test it by opening *Actions > Update dependencies* and clicking **Run workflow**. If dependencies are already current the job stops early without opening anything, which is also a useful signal that the plumbing works.

## Merging

The email arrives when the nightly finds updates. What to look at:

- The pull request body lists what moved and what was held back.
- A pull request titled "Zellij … is out" is the `zellij-upgrade` one, not the routine one. Do not merge it until the Zellij you run matches and you have confirmed hints still render right. Its body says the same. A red Checks tab on this one specifically can mean the plugin needs real source changes for the new zellij-tile, not just a version bump.
- Otherwise, CI runs on the pull request. Tests and a release build also passed before it was opened, so a red pull request means CI found something the update job did not.

Click **Merge**. That lands the update on `main`, and the next nightly build picks it up. It does not publish a release on its own; see [Versioning](#versioning) for that step, which is separate and entirely up to you.

Nothing merges on its own, so leaving one open for a few days costs nothing.

To stop one, click **Close**. The branch is reused, so the next run reopens with whatever is current, and nothing is lost by closing one you dislike.

## Installing what is published

```bash
make latest              # newest tagged release
make nightly             # tonight's build of main
make zellij VERSION=0.44 # newest release built for that Zellij line
make ea LABEL=<label>    # a specific EA/beta channel someone published
```

All of these fetch straight into the Zellij plugin path. Start a new session to load it: Zellij caches plugins per session, and detaching does not reload.

Pointing your Zellij config at a nightly URL does not work well. Zellij caches remote plugins by URL, so it keeps serving whatever it downloaded first. Fetch locally instead.

## Maintenance

### Pin the actions

All of them are pinned to commit SHAs. A version tag can be moved by whoever controls the action, so an unpinned `uses:` is a supply chain gap. If a new workflow step adds one without network access to verify a SHA, it should carry a `# TODO: pin to a SHA` comment until this fixes it:

```bash
gh api repos/<owner>/<repo>/commits/<tag> --jq .sha
```

Then replace `@<tag>` with `@<sha> # <tag>`, keeping the version in the trailing comment. Dependabot updates the pinned ones monthly.

### Rotate the private key

If the key is ever exposed, generate a new one on the app's settings page, update the `AUTOMATION_APP_PRIVATE_KEY` secret, then delete the old key from the app. Unlike a personal access token, this key has no expiry to track, so there is nothing to rotate on a schedule.

### Watch for needs-zellij-upgrade

Pull requests labeled `needs-zellij-upgrade` accumulate rather than merge, and they are the ones that matter.

## When something breaks

| Symptom | Cause |
| --- | --- |
| Update workflow fails immediately | `AUTOMATION_APP_ID`/`AUTOMATION_APP_PRIVATE_KEY` missing, or the app is not installed on this repo |
| Pull request opens but no checks run | Opened with `GITHUB_TOKEN`; the app token is not being picked up |
| Merge button is blocked | A required status check has not passed; check the PR's Checks tab |
| Release does not publish after merging an update PR | Expected: merging never bumps `version`; see [Versioning](#versioning) |
| Release does not publish after a version bump | Version in `Cargo.toml` already tagged; check the run's `Resolve version` step |
| `cargo test` fails to link | OpenSSL headers missing; the workflows install `libssl-dev`, locally use your package manager |
| Nightly is stale | Check the `nightly.yml` schedule ran; scheduled workflows are paused after 60 days of repository inactivity |
| `zellij-upgrade` pull request never appears | `zellij_minor_available` only goes true once crates.io has the new `zellij-tile`/`zellij-tile-utils`, which can lag a Zellij release by a day or so |
| `zellij-<line>` did not move after a release | Check `Cargo.toml`'s `zellij-tile` requirement at that commit; the tag follows whatever line was pinned at release time, not the newest one available |
| `beta.yml` fails at "Sanitize the label" | The `label` input had no `[a-z0-9-]` characters left after sanitizing; pick a label with at least one letter or digit |
| `beta.yml`'s build fails to resolve a Zellij crate | The ref you built does not have a `[patch.crates-io]` override for a requirement crates.io cannot satisfy yet (see [Depending on an unreleased upstream fix](#depending-on-an-unreleased-upstream-fix)) |

The stale-nightly row above is worth knowing in full: GitHub disables scheduled workflows in repositories with no activity for 60 days, and emails you when it does. Any push re-enables them.
