#!/usr/bin/env python3
"""Plans and assembles the builds described in builds.toml.

The same script runs in CI (.github/workflows/builds.yml) and locally, so a
stack that fails to merge in CI can be reproduced, resolved and recorded on
your own machine. Run it with --help, or see docs/automation.md.

Subcommands:

  plan       Decide which builds a workflow run should make.
  assemble   Merge one build's stack onto its base and print what came out.
  record     Save the conflict resolutions in a worktree so CI can replay them.
  prune      List the releases a build's `keep` setting says to delete.

The script never talks to the GitHub API. Where it needs to know what has been
released, the caller passes a file of releases (see load_releases).

Python 3.11 or newer, standard library only.
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_CONFIG = REPO_ROOT / "builds.toml"
DEFAULT_RERERE = REPO_ROOT / "tooling" / "rerere"

TRIGGERS = ("change", "nightly")
PLACEHOLDERS = {"version", "date", "zellij"}
KNOWN_KEYS = {
    "tag", "base", "stack", "on", "version", "prerelease", "publish",
    "keep", "rolling", "alias",
}
NAME_RE = re.compile(r"^[a-z0-9][a-z0-9-]*$")
PLACEHOLDER_RE = re.compile(r"\{([^}]*)\}")
# What a literal `version` may be: semver, with an optional prerelease or build
# suffix. It ends up in a tag name and in Cargo.toml, so nothing looser.
VERSION_RE = re.compile(r"^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$")
# `version = "tags"` reads the version out of a tag, so it only takes the plain
# X.Y.Z that release tags use.
TAG_VERSION_PATTERN = r"\d+\.\d+\.\d+"
# A literal `version` may carry a suffix, so its own releases must match too.
LITERAL_VERSION_PATTERN = r"\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?"
TAGS = "tags"
# Release notes end with this comment, recording what the build was made from,
# so a later scheduled run can tell whether anything has moved since.
INPUTS_MARKER = "zjhints-build-inputs"
INPUTS_RE = re.compile(r"<!-- " + INPUTS_MARKER + r" (\{.*?\}) -->")
# Where assemble fetches everything, so nothing it does moves a branch, tag or
# remote-tracking ref the reader has.
FETCH_NAMESPACE = "refs/zjhints-builds"


class Fail(Exception):
    """An error meant for the reader, printed without a traceback."""


class Skip(Exception):
    """The build has nothing to do this run. Reported, not an error."""

    def __init__(self, message: str, tag: str = ""):
        super().__init__(message)
        self.tag = tag


def git(*args: str, cwd: Path | str | None = None, check: bool = True) -> str:
    result = subprocess.run(
        ["git", *args], cwd=cwd, text=True, capture_output=True
    )
    if check and result.returncode != 0:
        raise Fail(
            f"git {' '.join(args)} failed:\n{result.stderr.strip() or result.stdout.strip()}"
        )
    return result.stdout.strip()


def git_ok(*args: str, cwd: Path | str | None = None) -> bool:
    """Runs a git command that answers yes or no through its exit status."""
    return subprocess.run(["git", *args], cwd=cwd, capture_output=True).returncode == 0


# Config


def load_config(text: str, source: str) -> dict[str, dict]:
    try:
        data = tomllib.loads(text)
    except tomllib.TOMLDecodeError as e:
        raise Fail(f"{source} is not valid TOML: {e}")
    builds = data.get("build", {})
    if not isinstance(builds, dict):
        raise Fail(f"{source}: [build] must be a table of [build.<name>] tables")
    for name, build in builds.items():
        validate(name, build, source)
    return builds


def validate(name: str, build: dict, source: str) -> None:
    where = f"{source}: [build.{name}]"
    if not NAME_RE.match(name):
        raise Fail(f"{where}: names are lowercase letters, digits and hyphens")
    if not isinstance(build, dict):
        raise Fail(f"{where}: must be a table")
    unknown = set(build) - KNOWN_KEYS
    if unknown:
        raise Fail(f"{where}: unknown key(s) {', '.join(sorted(unknown))}")
    for key in ("tag", "base"):
        if not isinstance(build.get(key), str) or not build[key]:
            raise Fail(f"{where}: `{key}` is required")
    stack = build.get("stack", [])
    if not isinstance(stack, list) or not all(isinstance(s, str) for s in stack):
        raise Fail(f"{where}: `stack` is a list of refs")
    trigger = build.get("on", "change")
    if trigger not in TRIGGERS:
        raise Fail(f"{where}: `on` is one of {', '.join(TRIGGERS)}")
    for key in ("prerelease", "publish", "rolling"):
        if key in build and not isinstance(build[key], bool):
            raise Fail(f"{where}: `{key}` is true or false")

    used = placeholders(build["tag"])
    if used - PLACEHOLDERS:
        raise Fail(
            f"{where}: unknown placeholder(s) in tag: {', '.join(sorted(used - PLACEHOLDERS))}"
        )
    if not fixed_prefix(build["tag"]):
        raise Fail(f"{where}: the tag must start with a fixed prefix, like `v` or `nightly-`")

    version = build.get("version")
    if "version" in build and not isinstance(version, str):
        raise Fail(f"{where}: `version` is a string")
    if "version" in used and version is None:
        raise Fail(f"{where}: the tag uses {{version}}, so `version` is required")
    if version == TAGS:
        if "version" not in used:
            raise Fail(f'{where}: version = "tags" reads the version out of the tag, '
                       f"so the tag must use {{version}}")
        if build.get("rolling"):
            raise Fail(f'{where}: version = "tags" releases tags that already exist, '
                       f"so it cannot be rolling")
    elif version is not None and not VERSION_RE.match(version):
        raise Fail(f'{where}: `version` is "tags" or a version like 1.2.3, not `{version}`')

    rolling = build.get("rolling", False)
    if rolling and "date" in used:
        raise Fail(f"{where}: a rolling tag is moved rather than renamed, so it cannot use {{date}}")
    if trigger == "nightly" and "date" not in used and not rolling:
        raise Fail(f"{where}: a nightly build's tag needs {{date}} to stay unique, or rolling = true")

    if "keep" in build:
        keep = build["keep"]
        if not isinstance(keep, int) or isinstance(keep, bool) or keep < 1:
            raise Fail(f"{where}: `keep` is a whole number of releases, at least 1")
        if "date" not in used:
            raise Fail(f"{where}: `keep` prunes a series of dated releases, so the tag needs {{date}}")

    if "alias" in build:
        alias = build["alias"]
        if not isinstance(alias, str) or not alias:
            raise Fail(f"{where}: `alias` is a tag pattern")
        alias_used = placeholders(alias)
        if alias_used - {"version", "zellij"}:
            raise Fail(f"{where}: an alias may only use {{version}} and {{zellij}}")
        if not fixed_prefix(alias):
            raise Fail(f"{where}: the alias must start with a fixed prefix, like `zellij-`")
        if "version" in alias_used and version is None:
            raise Fail(f"{where}: the alias uses {{version}}, so `version` is required")

    for ref in [build["base"], *stack]:
        parse_ref(ref, where)


def placeholders(pattern: str) -> set[str]:
    return set(PLACEHOLDER_RE.findall(pattern))


def fixed_prefix(pattern: str) -> str:
    """The literal text before the first placeholder."""
    return pattern.split("{", 1)[0]


def fill(pattern: str, values: dict[str, str]) -> str:
    return PLACEHOLDER_RE.sub(lambda m: values[m.group(1)], pattern)


def series_regex(pattern: str, version: str = TAG_VERSION_PATTERN) -> re.Pattern:
    """Matches every tag a build's tag pattern can produce.

    A {date} tag may carry the .2, .3 suffix choose_tag adds for a second build
    the same day. The first {version} is captured as the `version` group, and
    matches `version`: plain X.Y.Z unless the caller allows a suffix.
    """
    parts, named = [], False
    for piece in re.split(r"(\{[^}]*\})", pattern):
        if piece == "{version}":
            parts.append(version if named else f"(?P<version>{version})")
            named = True
        elif piece == "{date}":
            parts.append(r"\d{6}")
        elif piece == "{zellij}":
            parts.append(r"\d+\.\d+")
        else:
            parts.append(re.escape(piece))
    suffix = r"(?:\.\d+)?" if "{date}" in pattern else ""
    return re.compile("^" + "".join(parts) + suffix + "$")


def parse_ref(ref: str, where: str = "") -> str:
    """Turns a config ref into the refspec source to fetch from origin.

    feat/x, origin:feat/x  the branch feat/x

    Only branches of this repository, which only people with push access can
    write. A build runs its code's tests in the context of main, where it
    could reach main's Actions cache, so code from a fork's pull request has
    no place in one.
    """
    remote, sep, rest = ref.partition(":")
    if not sep:
        remote, rest = "origin", ref
    if remote != "origin" or not rest:
        raise Fail(f"{where}: `{ref}` is not `branch` or `origin:branch`")
    if rest.startswith("pull/"):
        raise Fail(f"{where}: `{ref}`: pull requests cannot be stacked. List a pull "
                   f"request from this repository by its branch name")
    return f"refs/heads/{rest}"


# Releases


def load_releases(path: str | None) -> list[dict]:
    """Reads the repository's releases from a file.

    Either a JSON array or one JSON object per line, each with at least
    `tag_name`, and `body` and `created_at` where known. builds.yml writes it
    with `gh api --paginate repos/<repo>/releases --jq '.[] | {...}'`. No file
    means no releases are known, which is what a local run gets.
    """
    if not path:
        return []
    text = Path(path).read_text().strip()
    if not text:
        return []
    if text.startswith("["):
        releases = json.loads(text)
    else:
        releases = [json.loads(line) for line in text.splitlines() if line.strip()]
    for r in releases:
        if not isinstance(r, dict) or not isinstance(r.get("tag_name"), str):
            raise Fail(f"{path}: every release needs a `tag_name`")
    return releases


def series_releases(build: dict, releases: list[dict]) -> list[dict]:
    """The build's releases, newest first."""
    literal = build.get("version") not in (None, TAGS)
    regex = series_regex(build["tag"], LITERAL_VERSION_PATTERN if literal else TAG_VERSION_PATTERN)
    found = [r for r in releases if regex.match(r["tag_name"])]
    return sorted(found, key=lambda r: r.get("created_at") or "", reverse=True)


def release_inputs(release: dict) -> dict | None:
    m = INPUTS_RE.search(release.get("body") or "")
    return json.loads(m.group(1)) if m else None


def inputs_comment(inputs: dict) -> str:
    return f"<!-- {INPUTS_MARKER} {json.dumps(inputs, separators=(',', ':'))} -->"


# plan


def cmd_plan(args: argparse.Namespace) -> None:
    config = load_config(Path(args.config).read_text(), args.config)
    names = plan(config, args)
    for n in names:
        print(f"{n}: will build", file=sys.stderr)
    print(json.dumps(names))


def plan(config: dict[str, dict], args: argparse.Namespace) -> list[str]:
    if args.event == "workflow_dispatch":
        wanted = [n.strip() for n in args.only.split(",") if n.strip()] or ["all"]
        if wanted == ["all"]:
            return list(config)
        missing = [n for n in wanted if n not in config]
        if missing:
            raise Fail(f"no build named {', '.join(missing)} in {args.config}")
        return wanted

    if args.event == "schedule":
        return [n for n, b in config.items() if b.get("on", "change") == "nightly"]

    if args.event != "push":
        raise Fail(f"unknown event {args.event}")

    if args.ref.startswith("refs/tags/"):
        tag = args.ref.removeprefix("refs/tags/")
        names = [
            n for n, b in config.items()
            if b.get("version") == TAGS and series_regex(b["tag"]).match(tag)
        ]
        if not names:
            print(f"{tag}: no build takes its version from this tag", file=sys.stderr)
        return names

    before = {}
    if args.before and set(args.before) != {"0"}:
        text = git("show", f"{args.before}:builds.toml", cwd=REPO_ROOT, check=False)
        # Only compared, never built from, so it is not validated: a table the
        # current rules would reject still counts as what was there before.
        # A file that no longer parses counts as no tables, so all build.
        try:
            before = tomllib.loads(text).get("build", {}) if text else {}
        except tomllib.TOMLDecodeError:
            before = {}
        if not isinstance(before, dict):
            before = {}
    for n in before:
        if n not in config:
            print(f"{n}: removed from the config, nothing to build", file=sys.stderr)
    return [n for n, b in config.items() if before.get(n) != b]


# assemble


def cmd_assemble(args: argparse.Namespace) -> None:
    config = load_config(Path(args.config).read_text(), args.config)
    if args.name not in config:
        raise Fail(f"no build named {args.name} in {args.config}")
    result = assemble(args.name, config[args.name], args)
    if result["tag"]:
        print(f"{args.name}: {result['sha'][:9]}, would be tagged {result['tag']}", file=sys.stderr)
    print(json.dumps(result))


def assemble(name: str, build: dict, args: argparse.Namespace) -> dict:
    repo = Path(args.repo).resolve()
    if not git("remote", "get-url", "origin", cwd=repo, check=False):
        raise Fail(f"{repo} has no `origin` remote")
    releases = load_releases(args.releases)
    released = {r["tag_name"] for r in releases}
    space = f"{FETCH_NAMESPACE}/{name}"

    refs = [build["base"], *build.get("stack", [])]
    resolved = []
    for i, ref in enumerate(refs):
        dst = f"{space}/{i}"
        git("fetch", "--quiet", "--no-tags", "origin", f"+{parse_ref(ref)}:{dst}", cwd=repo)
        resolved.append({"ref": ref, "sha": git("rev-parse", dst, cwd=repo)})
    base, stack = resolved[0], resolved[1:]

    tag, version = "", build.get("version", "")
    if version == TAGS:
        tag, version, tag_sha = version_from_tags(build, base, args.tag_ref, repo)
        # The release is the tagged commit, whatever `base` has moved on to.
        base = {"ref": tag, "sha": tag_sha}
        if tag in released and not args.trial:
            raise Skip(f"{tag} is already released. Push a new tag to release again.", tag)

    inputs = {"base": base["sha"], "stack": [s["sha"] for s in stack]}
    newest = series_releases(build, releases)
    if args.only_if_moved and newest and release_inputs(newest[0]) == inputs:
        raise Skip(f"nothing has moved since {newest[0]['tag_name']}")

    worktree = Path(args.worktree) if args.worktree else Path(
        tempfile.mkdtemp(prefix=f"zjhints-build-{name}-")
    )
    merge_stack(name, base, stack, repo, worktree, args)
    sha = git("rev-parse", "HEAD", cwd=worktree)
    zellij = zellij_line(worktree / "Cargo.toml")
    values = {
        "version": version,
        "zellij": zellij,
        "date": args.date or datetime.datetime.now(datetime.timezone.utc).strftime("%y%m%d"),
    }

    if not tag:
        try:
            tag = choose_tag(build, values, existing_tags(repo))
        except Skip as e:
            if not args.trial:
                git("worktree", "remove", "--force", str(worktree), cwd=repo)
                raise
            # Nothing will be published, so the tag only labels the trial.
            print(f"{name}: {e.tag} exists, but this is a trial, so it builds anyway",
                  file=sys.stderr)
            tag = e.tag

    # What the release notes count changes from. A rolling tag is its own
    # series, so its previous release is the one it is about to replace.
    history = [r for r in series_releases(build, releases)
               if r["tag_name"] != tag or build.get("rolling")]
    return {
        "name": name,
        "sha": sha,
        "tag": tag,
        "previous": history[0]["tag_name"] if history else "",
        "version": version,
        "zellij": zellij,
        "alias": fill(build["alias"], values) if "alias" in build else "",
        "rolling": bool(build.get("rolling", False)),
        "keep": build.get("keep", 0),
        # A build with no version is not a release anyone should get as the
        # latest one, so only a versioned build defaults to a full release.
        "prerelease": bool(build.get("prerelease", "version" not in build)),
        "publish": bool(build.get("publish", True)),
        "base": base,
        "stack": stack,
        "inputs": inputs_comment(inputs),
        "worktree": str(worktree),
    }


def version_from_tags(build: dict, base: dict, tag_ref: str | None, repo: Path) -> tuple[str, str, str]:
    """Picks the tag a `version = "tags"` build releases.

    The pushed tag when there is one, otherwise the newest matching tag that
    `base` contains. Returns the tag, the version read out of it, and the
    commit it points at.
    """
    regex = series_regex(build["tag"])
    git("fetch", "--quiet", "--no-tags", "--force", "--prune", "origin",
        f"+refs/tags/*:{FETCH_NAMESPACE}/tags/*", cwd=repo)

    def commit_of(tag: str) -> str:
        return git("rev-parse", f"{FETCH_NAMESPACE}/tags/{tag}^{{commit}}", cwd=repo)

    if tag_ref:
        tag = tag_ref.removeprefix("refs/tags/")
        m = regex.match(tag)
        if not m:
            raise Fail(f"`{tag}` does not match this build's tag, `{build['tag']}`")
        if not git_ok("rev-parse", "--verify", "--quiet", f"{FETCH_NAMESPACE}/tags/{tag}", cwd=repo):
            raise Fail(f"{tag} is not a tag on origin")
        sha = commit_of(tag)
        if not git_ok("merge-base", "--is-ancestor", sha, base["sha"], cwd=repo):
            raise Fail(f"{tag} points at {sha[:9]}, which is not on {build['base']}. "
                       f"Tag a commit that is, so a release is always something {build['base']} has.")
        return tag, m.group("version"), sha

    names = git("for-each-ref", "--format=%(refname:lstrip=3)",
                f"{FETCH_NAMESPACE}/tags/", cwd=repo).splitlines()
    candidates = []
    for tag in names:
        m = regex.match(tag)
        if not m:
            continue
        sha = commit_of(tag)
        if git_ok("merge-base", "--is-ancestor", sha, base["sha"], cwd=repo):
            candidates.append((version_key(m.group("version")), tag, m.group("version"), sha))
    if not candidates:
        raise Skip(f"no tag matching `{build['tag']}` is on {build['base']} yet. "
                   f"Push one to release.")
    _, tag, version, sha = max(candidates)
    return tag, version, sha


def version_key(version: str) -> tuple[int, ...]:
    return tuple(int(p) for p in version.split("."))


def merge_stack(name: str, base: dict, stack: list[dict], repo: Path, worktree: Path,
                args: argparse.Namespace) -> None:
    rerere = Path(args.rerere)
    common = Path(git("rev-parse", "--path-format=absolute", "--git-common-dir", cwd=repo))
    if rerere.is_dir():
        # Added to rather than replaced, so resolutions recorded locally but not
        # yet saved into the repository are not thrown away.
        shutil.copytree(rerere, common / "rr-cache", dirs_exist_ok=True,
                        ignore=shutil.ignore_patterns("README.md"))

    if worktree.exists() and any(worktree.iterdir()):
        raise Fail(f"{worktree} is not empty")
    git("worktree", "add", "--quiet", "--detach", str(worktree), base["sha"], cwd=repo)
    print(f"{name}: {base['ref']} at {base['sha'][:9]}, in {worktree}", file=sys.stderr)

    rr = ["-c", "rerere.enabled=true", "-c", "rerere.autoUpdate=true"]
    for item in stack:
        if git_ok("merge-base", "--is-ancestor", item["sha"], "HEAD", cwd=worktree):
            print(f"  {item['ref']} ({item['sha'][:9]}) is already in the base; it can come out of the stack",
                  file=sys.stderr)
            item["merged"] = False
            continue
        msg = f"Stack {item['ref']} ({item['sha'][:9]}) onto {name}"
        merge = subprocess.run(
            ["git", *rr, "merge", "--quiet", "--no-ff", "--no-edit", "-m", msg, item["sha"]],
            cwd=worktree, text=True, capture_output=True,
        )
        if merge.returncode != 0:
            unmerged = git("diff", "--name-only", "--diff-filter=U", cwd=worktree).splitlines()
            if not unmerged and git_ok("rev-parse", "--verify", "--quiet", "MERGE_HEAD", cwd=worktree):
                git(*rr, "commit", "--quiet", "--no-edit", cwd=worktree)
                print(f"  {item['ref']}: conflicts resolved from recorded resolutions", file=sys.stderr)
            else:
                raise Fail(conflict_help(name, item, unmerged, worktree, merge.stderr))
        item["merged"] = True
        print(f"  merged {item['ref']} ({item['sha'][:9]})", file=sys.stderr)


def conflict_help(name: str, item: dict, unmerged: list[str], worktree: Path, stderr: str) -> str:
    files = "\n".join(f"    {f}" for f in unmerged) or "    (none listed)\n" + stderr
    head = (
        f"merging {item['ref']} ({item['sha'][:9]}) into build `{name}` conflicts in:\n"
        f"{files}\n\nNo recorded resolution covers it. "
    )
    record = (
        f"  python3 tooling/builds.py record --worktree {'<that worktree>' if os.environ.get('CI') else worktree}\n"
        f"then commit the new files under tooling/rerere/ and push. Later builds\n"
        f"replay the resolution, for as long as the conflict looks the same."
    )
    if os.environ.get("CI"):
        return head + (
            f"To record one, on your own machine, from a zjhints checkout:\n"
            f"  python3 tooling/builds.py assemble {name}\n"
            f"resolve the files in the worktree it names, then run\n" + record
        )
    return head + (
        f"The merge is left in progress in\n  {worktree}\n"
        f"Resolve the files there, then run\n" + record + "\n"
        f"Remove the worktree afterwards with `git worktree remove --force {worktree}`."
    )


def zellij_line(cargo_toml: Path) -> str:
    """major.minor of the zellij-tile requirement: the Zellij this build targets."""
    data = tomllib.loads(cargo_toml.read_text())
    requirement = data.get("dependencies", {}).get("zellij-tile")
    if isinstance(requirement, dict):
        requirement = requirement.get("version")
    if not isinstance(requirement, str):
        raise Fail(f"no zellij-tile version in {cargo_toml}")
    m = re.search(r"(\d+)\.(\d+)", requirement)
    if not m:
        raise Fail(f"cannot read a version out of zellij-tile = \"{requirement}\" in {cargo_toml}")
    return f"{m.group(1)}.{m.group(2)}"


def existing_tags(repo: Path) -> set[str]:
    return {
        line.split("refs/tags/", 1)[1].removesuffix("^{}")
        for line in git("ls-remote", "--tags", "origin", cwd=repo).splitlines()
    }


def choose_tag(build: dict, values: dict[str, str], existing: set[str]) -> str:
    tag = fill(build["tag"], values)
    if not git_ok("check-ref-format", f"refs/tags/{tag}"):
        raise Fail(f"`{tag}` is not a valid tag name")
    if tag not in existing or build.get("rolling"):
        return tag
    if "{date}" in build["tag"]:
        n = 2
        while f"{tag}.{n}" in existing:
            n += 1
        return f"{tag}.{n}"
    # Not an error: the first push of a config, or a change to some other key,
    # reaches here for a release that already went out. Reported, and the build
    # is left out of the run.
    raise Skip(f"{tag} is already released. Bump `version` for this build in builds.toml "
               f"to cut a new one.", tag)


# record


def cmd_record(args: argparse.Namespace) -> None:
    worktree = Path(args.worktree).resolve()
    if not git_ok("rev-parse", "--verify", "--quiet", "MERGE_HEAD", cwd=worktree):
        raise Fail(f"{worktree} is not in the middle of a merge, so there is nothing to record")
    # Resolved files may or may not be staged; what counts is that the working
    # tree no longer has conflict markers in it, since that is what rerere reads.
    unmerged = git("diff", "--name-only", "--diff-filter=U", cwd=worktree).splitlines()
    marked = [f for f in unmerged if has_markers(worktree / f)]
    if marked:
        raise Fail("these files still have conflict markers, resolve them first:\n"
                   + "\n".join(f"    {f}" for f in marked))
    # MERGE_RR names the rr-cache entries of this merge's conflicts. The cache
    # itself is shared by every worktree of the clone, and may hold the
    # reader's own resolutions from unrelated merges, which are not saved.
    # Read first: rerere empties MERGE_RR once it has recorded them.
    merge_rr = Path(git("rev-parse", "--path-format=absolute", "--git-path", "MERGE_RR", cwd=worktree))
    ids = {line.split("\t", 1)[0] for line in
           (merge_rr.read_text().split("\0") if merge_rr.is_file() else []) if "\t" in line}
    # Records the resolution of every conflict in the worktree as it stands now.
    git("-c", "rerere.enabled=true", "rerere", cwd=worktree)
    common = Path(git("rev-parse", "--path-format=absolute", "--git-common-dir", cwd=worktree))
    cache = common / "rr-cache"
    dest = Path(args.rerere)
    saved = 0
    for entry in sorted(cache / i for i in ids if (cache / i).is_dir()):
        # Only complete entries: a preimage with no postimage is a conflict that
        # was seen but never resolved, and replaying it would resolve nothing.
        if any(p.name.startswith("postimage") for p in entry.iterdir()):
            target = dest / entry.name
            if not target.exists():
                shutil.copytree(entry, target)
                saved += 1
                print(f"saved {entry.name}", file=sys.stderr)
    if not saved:
        print("nothing new to save", file=sys.stderr)
    else:
        print(f"\n{saved} resolution(s) saved under {dest}. Commit them.", file=sys.stderr)


def has_markers(path: Path) -> bool:
    if not path.is_file():
        return False
    with path.open("rb") as f:
        return any(line.startswith((b"<<<<<<< ", b">>>>>>> ")) for line in f)


# prune


def cmd_prune(args: argparse.Namespace) -> None:
    config = load_config(Path(args.config).read_text(), args.config)
    if args.name not in config:
        raise Fail(f"no build named {args.name} in {args.config}")
    for tag in prune(config[args.name], load_releases(args.releases), args.published):
        print(tag)


def prune(build: dict, releases: list[dict], published: str) -> list[str]:
    """The tags of the build's releases beyond its newest `keep`, oldest last.

    `published` is the release just made, which counts as the newest even when
    the list of releases was read before it existed.
    """
    keep = build.get("keep")
    if not keep:
        return []
    others = [r["tag_name"] for r in series_releases(build, releases) if r["tag_name"] != published]
    return others[keep - 1:]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--config", default=str(DEFAULT_CONFIG))
    sub = parser.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("plan", help="list the builds a workflow run should make, as JSON")
    p.add_argument("--event", required=True, choices=["push", "workflow_dispatch", "schedule"])
    p.add_argument("--ref", default="", help="for push: the ref pushed, e.g. refs/tags/v1.2.3")
    p.add_argument("--before", default="", help="for a branch push: the commit before the push")
    p.add_argument("--only", default="all", help="for workflow_dispatch: comma-separated names, or all")
    p.set_defaults(func=cmd_plan)

    p = sub.add_parser("assemble", help="merge a build's stack and print the result as JSON")
    p.add_argument("name")
    p.add_argument("--repo", default=".", help="a zjhints clone with an `origin` remote")
    p.add_argument("--worktree", help="where to assemble; a new temporary directory by default")
    p.add_argument("--rerere", default=str(DEFAULT_RERERE), help="recorded resolutions to replay")
    p.add_argument("--releases", help="the repository's releases, as JSON (see load_releases)")
    p.add_argument("--tag-ref", help='for version = "tags": the tag that was pushed')
    p.add_argument("--date", help="yymmdd to use for {date}; today, UTC, by default")
    p.add_argument("--only-if-moved", action="store_true",
                   help="skip when the newest release of this build was made from the same inputs")
    p.add_argument("--trial", action="store_true",
                   help="nothing will be published: build even if the tag is already released")
    p.set_defaults(func=cmd_assemble)

    p = sub.add_parser("record", help="save the conflict resolutions in a worktree")
    p.add_argument("--worktree", required=True)
    p.add_argument("--rerere", default=str(DEFAULT_RERERE), help="where to save them")
    p.set_defaults(func=cmd_record)

    p = sub.add_parser("prune", help="list the releases beyond a build's `keep`, one tag per line")
    p.add_argument("name")
    p.add_argument("--releases", required=True, help="the repository's releases, as JSON")
    p.add_argument("--published", default="", help="the tag just released, counted as newest")
    p.set_defaults(func=cmd_prune)

    args = parser.parse_args()
    try:
        args.func(args)
    except Skip as e:
        # A JSON line on stdout so builds.yml can tell a skip from a failure.
        print(f"{getattr(args, 'name', '')}: {e}", file=sys.stderr)
        print(json.dumps({"name": getattr(args, "name", ""), "skip": str(e)}))
    except Fail as e:
        print(f"error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
