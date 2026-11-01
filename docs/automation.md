# Automation

How this repository builds, tests, updates itself and publishes, and the one-time setup GitHub needs before any of it works.

If you are reading this to get things running, start at [First-time setup](#first-time-setup). It is four tasks and takes about ten minutes.

## Contents

- [The shape of it](#the-shape-of-it)
- [Placeholders](#placeholders)
- [Workflows](#workflows)
- [Builds](#builds)
- [Dependencies](#dependencies)
- [First-time setup](#first-time-setup)
- [Merging](#merging)
- [Installing what is published](#installing-what-is-published)
- [Maintenance](#maintenance)
- [When something breaks](#when-something-breaks)

## The shape of it

```text
 weekly (Mon 04:00 UTC)             you                         result
 --------------------               ---                         ------
 update-deps.yml
   cargo update, nix flake update
   test + build
   open pull request  ---------->  email arrives
                                   wait a day or two
                                   click Merge        ------>  merged,
                                                                no release yet

                                   ...whenever you decide to release...
                                   git tag v0.6.0
                                   git push origin v0.6.0
                                                        |
 builds.yml <-------------------------------------------+
   sees a tag that builds.toml's `release` table matches
   test + build, with 0.6.0 written into Cargo.toml for the build only
   publish v0.6.0, becomes `latest`
   move the `zellij-<line>` tag to it too

 builds.yml (05:00 UTC, or run by hand)
   if main has moved since the last nightly:
   build main
   publish nightly-<date> as a prerelease
   delete nightly releases beyond the newest seven

 builds.yml (a push to main that changes builds.toml)
   build the tables that changed, such as an EA stack
   publish each one

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
                                                                you push a tag
```

Merging an automated pull request only ever lands the update on `main`. Releasing is a separate, deliberate step you take on your own schedule; see [Versioning](#versioning).

### Why not auto-merge on approval

Automated pull requests are opened by a GitHub App with its own bot identity (see [First-time setup](#first-time-setup)), not your account, so you can review and approve them like anyone else's. The pause is kept anyway: auto-merge would merge the moment CI went green rather than waiting for you to look, and on dependency updates specifically that is a worse trade than one click.

## Placeholders

Several commands and tag names below take a value you supply, written in angle brackets where it appears.

| Placeholder | Where its value comes from |
| --- | --- |
| `<line>` | A Zellij minor this project has targeted, such as `0.44` |
| `<label>` | The name of an EA build, such as `nested-sessions` for the table `ea-nested-sessions` |
| `<name>` | The name of a `[build.<name>]` table in `builds.toml`, such as `release` or `ea-nested-sessions` |
| `<date>` | The build date as `yymmdd` in UTC, such as `261101`, with `.2`, `.3` added for a second build the same day |
| `<worktree>` | The directory `tooling/builds.py assemble` prints when a stack does not merge |
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
| `ci.yml` | pull requests and pushes to `main` | rustfmt, clippy, tests, wasm build, flake MSRV check, tests of the build tooling, `cargo audit` |
| `update-deps.yml` | 04:00 UTC Mondays, or manually | updates Cargo and flake.lock dependencies, opens/updates a pull request; a second job opens a separate one when a Zellij minor is out |
| `builds.yml` | a push to `main` that changes `builds.toml`, a pushed tag, 05:00 UTC daily, or manually | builds and publishes what `builds.toml` describes (see [Builds](#builds)) |
| `cleanup-caches.yml` | a pull request closes | deletes that PR's Actions caches |

The `cargo test` step of `ci.yml` also checks the images in `docs/img`. It draws each one again from the bar and settings screen code and fails when a file differs, so a change that alters how they look has to commit the regenerated images with it. Run `make images` to write them. See [Images](../CONTRIBUTING.md#images).

`builds.yml` and `update-deps.yml` run the same `cargo test`, so a stale image also stops a nightly build, a release or an EA build. When one of those fails on `the_images_in_the_docs_are_up_to_date`, run `make images` on the branch it built and merge the result.

The `builds tooling` job of `ci.yml` runs the unit tests of `tooling/builds.py`, the script that plans and assembles builds. Run the same tests locally with Python 3.11 or newer, which is all the script needs:

```bash
python3 -m unittest discover -s tooling
```

## Builds

`builds.yml` builds and publishes whatever `builds.toml` describes. Releases, nightlies and EA builds are all tables in that one file, and one workflow handles every kind.

### builds.toml

`builds.toml` sits at the repository root and is read from `main`. Each `[build.<name>]` table is one series of releases. The header comment of the file documents every key; this is the short version.

| Key | What it sets |
| --- | --- |
| `tag` | The release's tag: a fixed prefix, then any of `{version}`, `{date}` and `{zellij}` |
| `base` | What the build starts from, usually `main` |
| `stack` | Branches merged onto the base in order; empty builds the base as it is |
| `on` | `"change"` (default) builds when the table changes; `"nightly"` also builds each night that its inputs moved |
| `version` | A version like `"1.2.3"`, or `"tags"` to take the version from a pushed tag |
| `prerelease` | Marks the release as a prerelease; the default is false for a table with a `version` and true for one without |
| `publish` | `false` tests and compiles but never releases |
| `rolling` | `true` moves the same tag to each new build and replaces its release |
| `alias` | A second tag moved to each new build, with a release of its own that is never the latest |
| `keep` | For a `{date}` tag, how many releases of the series to keep |

In `tag`, `{date}` is `<date>` and `{zellij}` is the major and minor of the `zellij-tile` requirement in the build's `Cargo.toml`, such as `0.46`. A tag that already exists is skipped, except that a `{date}` tag gets a `.2`, `.3` and so on for a second build the same day.

A ref in `base` or `stack` is a branch of this repository: `main`, `feat/x` or `origin:feat/x`. A pull request is listed by its branch name. One from a fork cannot be stacked, because a build runs the code it builds, tests included, in the context of `main`, where it could write to the Actions cache that `ci.yml` restores on `main`. To build a fork's change, push its branch to this repository after reviewing it.

The file ships with a `release` table and a `nightly` table, and a commented-out EA example.

### What starts a build

| Event | Builds |
| --- | --- |
| A push to `main` that changes `builds.toml` | Each table that changed |
| A pushed tag | Each `version = "tags"` table whose `tag` pattern the tag matches |
| 05:00 UTC daily | Each `on = "nightly"` table whose inputs have moved since its newest release; those inputs are the commits of its base and stack |
| *Actions > Builds > Run workflow* | The tables you name, or all of them |

A run publishes a build only after its tests pass and the plugin compiles. Builds are independent: one that fails to merge, test or compile is reported and left out, and the rest still publish.

Each release's notes list the base, the stack and the commit built, the Zellij line, a KDL `plugin location=` snippet for loading it, and the changelog since the previous release in the same series. A hidden comment at the end records the build's inputs, which is how the nightly schedule tells whether anything moved.

### Tags and releases

- `vX.Y.Z`: one per release, permanent. `/releases/latest/download/<asset>` resolves to the newest of these.
- `nightly-<date>`: one per night that `main` moved, such as `nightly-261101`, and `nightly-261101.2` for a second build the same day. A prerelease, so it never becomes `latest`. The newest seven are kept; older releases and their tags are deleted after each publish.
- `ea-<label>-<date>`: the same scheme for an EA build, with `keep` set by its table. A prerelease unless its table gives a `version`. See [EA builds](#ea-builds).
- `zellij-<line>`: one per Zellij minor this project has ever targeted, such as `zellij-0.44`, moved by `builds.yml` every time a release ships for that line. It is a real release rather than a prerelease, just aliased, but it is never the latest, so it does not contend with `latest`. See [Versioning](installation.md#versioning) for why this exists.

A tag named `nightly`, or `ea-<label>` without a date, is not updated by any build. One that still exists on GitHub is a leftover.

There is deliberately no tag named `latest`. GitHub already tracks the newest release, and a real tag by that name would have to be force-pushed on every release for no gain.

### Versioning

The version of a release is the tag you push. Neither pull request from `update-deps.yml` touches `version` in `Cargo.toml`, and neither releases anything. The routine one only moves the output of `cargo update` and `nix flake update`, and the zellij-upgrade one only widens the zellij-tile, zellij-tile-utils and zellij-utils requirements. Merging either lands the update on `main`; nothing publishes until you decide it should.

The `release` table writes the version from the tag into `Cargo.toml` for that build only. The `Cargo.toml` on `main` is never changed by a release, and editing its `version` cuts nothing.

Patch, minor, or major is your call each time, as the name of the tag. A Zellij line move landed via a `zellij-upgrade` pull request is a natural candidate for a minor bump specifically; tag it once you have confirmed hints render right. See [Versioning](installation.md#versioning) for the convention this project uses.

### Cutting a release

Releasing is pushing a `vX.Y.Z` tag on a commit that is on `main`. Merge what you want to ship, let CI go green, then:

```bash
git tag v0.6.0
git push origin v0.6.0
```

`builds.yml` builds the tagged commit, runs the tests and compiles the plugin. It then publishes `v0.6.0` as the latest release with notes from `git-cliff` covering the commits since the previous `v` tag, and moves the `zellij-<line>` alias tag and its release to it. A tag on a commit that is not on `main` is rejected.

Running the `release` table by hand with the publish box ticked, or changing it in `builds.toml`, builds the newest `v` tag on `main` instead. It skips that tag if it already has a release, so this is how you retry a release whose run failed after the tag was pushed.

### Trying a build without releasing

A trial tests and compiles a build and publishes nothing, so a stack can be checked before it costs a release.

1. Open *Actions > Builds* and click **Run workflow**, or run `gh workflow run builds.yml -f builds=<name> -f publish=false`.
2. Enter the table names in the builds field, comma-separated, or leave `all`.
3. Leave the publish box unticked and start the run.

The plugin is attached to the run page as the artifact `<name>--zjhints`. A trial builds a table even when its tag has already been released. Tick the publish box to release for real.

### EA builds

An EA build hands work in progress to testers: one or more branches stacked onto `main`. Use it for a branch that still needs a `[patch.crates-io]` override like the one under [Depending on an unreleased upstream fix](#depending-on-an-unreleased-upstream-fix), or any other work not ready for a real release.

Add a table named `ea-<label>` to `builds.toml`:

```toml
[build.ea-nested-sessions]
tag = "ea-nested-sessions-{date}"
base = "main"
stack = ["feat/nested-sessions"]
prerelease = true
keep = 3
```

Merging that change to `main` builds and publishes it. The branches in `stack` are merged onto `main` in order, so list them in the order they should apply. A build that merges anything is also pushed to a branch named `build/<name>`, which is rewritten on every build. Check it out to see exactly what was built, but never commit to it.

A stack may not change anything under `.github/workflows/`; such a build is refused. `build/<name>` is pushed with the built-in token, which starts no workflows, so a branch in a stack cannot run a workflow of its own with this repository's secrets. Land workflow changes on `main` first.

To rebuild with whatever the branches have gained since, run the workflow by hand naming the table with publish ticked, or set `on = "nightly"` so it rebuilds each night that its inputs moved. Pick a new label to keep two EA builds around side by side, such as `ea-nested-sessions` and `ea-zellij-0.46`.

Install one with `make ea LABEL=<label>`. See [Installing what is published](#installing-what-is-published).

Remove the table once the work is merged. That stops new builds, and releases already published stay until you delete them.

### When a stack does not merge

When a ref in a stack conflicts with the base or with an earlier ref, that build's `assemble` step fails and says which ref and files conflict. The other builds in the run carry on. The conflict has to be resolved once, by hand, and recorded so that every later build replays the resolution.

1. In a clone of zjhints with an `origin` remote, run `python3 tooling/builds.py assemble <name>`. It leaves the merge in progress in a worktree and prints the path as `<worktree>`.
2. Resolve the conflicting files in `<worktree>`, and remove every conflict marker.
3. Run `python3 tooling/builds.py record --worktree <worktree>`. It saves each resolution under `tooling/rerere/`.
4. Commit the new files under `tooling/rerere/` and push them to `main`.
5. Run `git worktree remove --force <worktree>` to clean up.

Later builds copy those files into the repository they merge in, and git replays any that match. A resolution stops matching when either side of the conflict changes, so a stale one is harmless, and you delete it whenever convenient.

## Dependencies

How dependencies are updated and held back, and how to build against an upstream fix that has not shipped.

### How dependencies are chosen

`cargo update` only moves within the range each `Cargo.toml` entry allows. For `0.x` crates that is patch releases, so `zellij-tile = "0.45.0"` accepts 0.45.1 but never 0.46.0.

That is the policy on purpose.

> [!WARNING]
> A zellij-tile newer than the Zellij you run breaks hints silently. Zellij's plugin boundary decodes a binding's actions with `.filter_map(|a| a.try_into().ok())`, so an action the plugin does not recognize is dropped and the binding arrives truncated. The plugin builds, tests pass, hints render with wrong labels, and there is no error anywhere.

`.github/scripts/check_deps.py` asks crates.io what exists and reports anything held back. When a Zellij crate has a new minor, the second job in `update-deps.yml`, named zellij-upgrade, proposes taking it. It opens its own pull request on its own branch, `deps/zellij-upgrade`, widening the zellij-tile, zellij-tile-utils and zellij-utils requirements to the new version and re-resolving just those crates. It is labeled `needs-zellij-upgrade` and its title says so too.

That pull request is never safe to merge on a green build alone. CI passing only means the plugin still compiles and its own tests pass; it says nothing about whether the Zellij you run matches. Upgrade Zellij first, confirm hints still render right, then merge. Merging alone does not publish anything (see [Versioning](#versioning)). When you are ready to release, push a `vX.Y.Z` tag on `main`. `builds.yml` then publishes it as a normal release and moves the matching [`zellij-<line>` tag](#tags-and-releases) to it.

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
zellij-utils = { git = "https://github.com/<you>/zellij", tag = "<tag-or-branch>" }
```

This only works if the crate's own declared version still satisfies the requirement your `Cargo.toml` already has. Cargo patches the source a requirement resolves from, not the requirement itself. That declared version lives in the crate's own `Cargo.toml`, or in `[workspace.package].version` for a workspace like zellij's. If the fork bumped its version past what you require, widen the requirement to match at the same time.

Then regenerate the lock against the new source and confirm it builds:

```bash
cargo update -p zellij-tile -p zellij-tile-utils -p zellij-utils
make check && cargo test --all-features
```

`zellij-utils` needs a patch entry of its own, pointing at the same tag, because zjhints depends on it directly for the settings screen's config-file builders. The workflow that widens `zellij-tile` past a Zellij minor moves `zellij-utils` to the same version.

To hand a branch with a patch to testers before the fix ships, stack it in an [EA build](#ea-builds).

#### What a patch does not disturb

The Zellij line `builds.yml` puts in a release's `zellij-<line>` tag and `check_deps.py` both read the version requirement string in `[dependencies]`, never the patch, so tags, releases, and versioning all keep working unmodified while a patch is active.

`check_deps.py` and the `zellij-upgrade` job go quiet for the patched crate specifically: the locked version already looks at or ahead of whatever crates.io has, so there is nothing for it to propose. That is expected, not a sign the check is broken.

#### Removing a patch once the fix ships

Delete the `[patch.crates-io]` block, confirm the `[dependencies]` requirement still matches, widening it to the real published version if the fork was ahead, then run `cargo update -p zellij-tile -p zellij-tile-utils -p zellij-utils` to move the lock back onto crates.io. From there it is a normal dependency again, and a normal release once you push a tag. See [Versioning](#versioning).

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
   rustfmt   clippy   test   build (wasm)   flake (msrv)   builds tooling
   ```

7. Leave **Require branches to be up to date** unticked, or a busy day means rebasing before every merge.

> [!NOTE]
> Status check names must match the `name:` of each job in `ci.yml`, and they only appear in the picker after a workflow has run once. Open a pull request and let CI run before coming back here.

Required approvals is 0 above because the status checks are the gate doing the actual work. Unlike a personal-access-token setup it does not have to be: the app's pull requests belong to its own `[bot]` identity, not yours, so GitHub will let you approve them like anyone else's. Set it to 1 instead if you want the extra click of an explicit approval before merging.

Also decide whether to tick **Do not allow bypassing the above settings**. Leaving it unticked lets you push directly to `main` when you need to; ticking it means even you go through a pull request.

### 4. Get the email

1. Open *Settings > Notifications* from your avatar menu.
2. Under *Subscriptions > Watching*, make sure email is enabled.
3. On the repository page, click **Watch** and choose **All Activity**, or **Custom > Pull requests** for less noise.

Test it by opening *Actions > Update dependencies* and clicking **Run workflow**. If dependencies are already current the job stops early without opening anything, which is also a useful signal that the plumbing works.

## Merging

The email arrives when the weekly run finds updates. What to look at:

- The pull request body lists what moved and what was held back.
- A pull request titled "Zellij … is out" is the `zellij-upgrade` one, not the routine one. Do not merge it until the Zellij you run matches and you have confirmed hints still render right. Its body says the same. A red Checks tab on this one specifically can mean the plugin needs real source changes for the new zellij-tile, not just a version bump.
- Otherwise, CI runs on the pull request. Tests and a release build also passed before it was opened, so a red pull request means CI found something the update job did not.

Click **Merge**. That lands the update on `main`, and the next nightly build picks it up. It does not publish a release on its own; see [Cutting a release](#cutting-a-release) for that step, which is separate and entirely up to you.

Nothing merges on its own, so leaving one open for a few days costs nothing.

To stop one, click **Close**. The branch is reused, so the next run reopens with whatever is current, and nothing is lost by closing one you dislike.

## Installing what is published

```bash
make latest              # newest tagged release
make nightly             # newest dated nightly build of main
make zellij VERSION=0.44 # newest release built for that Zellij line
make ea LABEL=<label>    # newest build of an EA table in builds.toml
```

All of these fetch straight into the Zellij plugin path. Start a new session to load it: Zellij caches plugins per session, and detaching does not reload.

`make nightly` and `make ea` look up the newest dated release through the GitHub API, so they need network access to `api.github.com` as well as to the download.

Pointing your Zellij config at a nightly or EA URL does not work well. Every build has a URL of its own, and Zellij caches remote plugins by URL, so it keeps serving whatever it downloaded first. Fetch locally instead.

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
| Release does not publish after merging an update PR | Expected: merging never releases; push a tag, see [Cutting a release](#cutting-a-release) |
| Release does not publish after pushing a tag | The tag must be `vX.Y.Z` on a commit that is on `main`, and the tag's release must not exist yet; the *Builds* summary of the run says which build was skipped or failed to assemble |
| Editing `version` in `Cargo.toml` released nothing | Expected: the tag decides the version; see [Versioning](#versioning) |
| Build fails at `assemble` with a conflict | A ref in the stack does not merge; see [When a stack does not merge](#when-a-stack-does-not-merge) |
| `cargo test` fails to link | OpenSSL headers missing; the workflows install `libssl-dev`, locally use your package manager |
| Nightly is stale | Either `main` has not moved since the last nightly, or the scheduled run did not happen; scheduled workflows are paused after 60 days of repository inactivity |
| `zellij-upgrade` pull request never appears | `zellij_minor_available` only goes true once crates.io has the new `zellij-tile`/`zellij-tile-utils`, which can lag a Zellij release by a day or so |
| `zellij-<line>` did not move after a release | Check `Cargo.toml`'s `zellij-tile` requirement in the tagged commit; the alias follows whatever line was pinned there, not the newest one available |
| An EA build fails to resolve a Zellij crate | The stack has no `[patch.crates-io]` override for a requirement crates.io cannot satisfy yet; see [Depending on an unreleased upstream fix](#depending-on-an-unreleased-upstream-fix) |
| `make ea` finds no build | The table's `tag` is not of the form `ea-<label>-{date}`, or the table has not been built yet |

The stale-nightly row above is worth knowing in full: GitHub disables scheduled workflows in repositories with no activity for 60 days, and emails you when it does. Any push re-enables them.
