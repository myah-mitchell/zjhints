"""Tests for tooling/builds.py.

Run from the repository root with:

    python3 -m unittest discover -s tooling

Tests that assemble a build make a throwaway `origin` repository and a clone of
it under a temporary directory, so they need git but no network.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import builds  # noqa: E402
from builds import Fail, Skip  # noqa: E402

REAL_CONFIG = Path(__file__).resolve().parent.parent / "builds.toml"

# Commits made in the test repositories never depend on the reader's own git
# configuration or signing setup.
GIT_ENV = {
    "GIT_AUTHOR_NAME": "test", "GIT_AUTHOR_EMAIL": "test@example.com",
    "GIT_COMMITTER_NAME": "test", "GIT_COMMITTER_EMAIL": "test@example.com",
    "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1",
}


def config(text: str) -> dict:
    return builds.load_config(textwrap.dedent(text), "builds.toml")


class ValidateTest(unittest.TestCase):
    def assertRejects(self, text: str, message: str) -> None:
        with self.assertRaises(Fail) as caught:
            config(text)
        self.assertIn(message, str(caught.exception))

    def test_the_shipped_config_is_valid(self):
        loaded = builds.load_config(REAL_CONFIG.read_text(), str(REAL_CONFIG))
        self.assertEqual(loaded["release"]["version"], "tags")

    def test_rejects_unknown_keys(self):
        self.assertRejects('[build.a]\ntag = "a-{date}"\nbase = "main"\ntargets = []\n', "unknown key(s) targets")

    def test_requires_tag_and_base(self):
        self.assertRejects('[build.a]\nbase = "main"\n', "`tag` is required")
        self.assertRejects('[build.a]\ntag = "a-{date}"\n', "`base` is required")

    def test_rejects_bad_names(self):
        self.assertRejects('[build.Release]\ntag = "a-{date}"\nbase = "main"\n', "lowercase")

    def test_rejects_upstream_placeholder_and_refs(self):
        self.assertRejects('[build.a]\ntag = "a-{upstream}"\nbase = "main"\n', "unknown placeholder(s) in tag: upstream")
        self.assertRejects('[build.a]\ntag = "a-{date}"\nbase = "upstream:main"\n', "is not `branch`")

    def test_accepts_branch_and_pull_refs(self):
        self.assertEqual(builds.parse_ref("feat/x"), "refs/heads/feat/x")
        self.assertEqual(builds.parse_ref("origin:feat/x"), "refs/heads/feat/x")
        self.assertEqual(builds.parse_ref("pull/12"), "refs/pull/12/head")
        with self.assertRaises(Fail):
            builds.parse_ref("pull/abc")

    def test_tag_needs_a_fixed_prefix(self):
        self.assertRejects('[build.a]\ntag = "{date}"\nbase = "main"\n', "fixed prefix")

    def test_version_placeholder_needs_version(self):
        self.assertRejects('[build.a]\ntag = "v{version}"\nbase = "main"\n', "`version` is required")

    def test_literal_version_must_look_like_one(self):
        self.assertRejects('[build.a]\ntag = "v{version}"\nversion = "1.2"\nbase = "main"\n', "like 1.2.3")
        config('[build.a]\ntag = "v{version}"\nversion = "1.2.3-rc.1"\nbase = "main"\n')

    def test_tags_mode_needs_version_in_the_tag_and_no_rolling(self):
        self.assertRejects('[build.a]\ntag = "a-{date}"\nversion = "tags"\nbase = "main"\n', "must use {version}")
        self.assertRejects('[build.a]\ntag = "v{version}"\nversion = "tags"\nrolling = true\nbase = "main"\n',
                           "cannot be rolling")

    def test_nightly_needs_a_date_unless_rolling(self):
        self.assertRejects('[build.a]\ntag = "nightly"\nbase = "main"\non = "nightly"\n', "needs {date}")
        config('[build.a]\ntag = "nightly"\nbase = "main"\non = "nightly"\nrolling = true\n')

    def test_rolling_cannot_use_date(self):
        self.assertRejects('[build.a]\ntag = "a-{date}"\nbase = "main"\nrolling = true\n', "cannot use {date}")

    def test_keep_needs_a_dated_tag_and_a_positive_count(self):
        self.assertRejects('[build.a]\ntag = "a"\nbase = "main"\nkeep = 3\n', "needs {date}")
        self.assertRejects('[build.a]\ntag = "a-{date}"\nbase = "main"\nkeep = 0\n', "at least 1")
        self.assertRejects('[build.a]\ntag = "a-{date}"\nbase = "main"\nkeep = true\n', "at least 1")

    def test_alias_placeholders(self):
        self.assertRejects('[build.a]\ntag = "a-{date}"\nbase = "main"\nalias = "b-{date}"\n', "may only use")
        self.assertRejects('[build.a]\ntag = "a-{date}"\nbase = "main"\nalias = "{zellij}"\n', "fixed prefix")

    def test_bad_on_and_bool_keys(self):
        self.assertRejects('[build.a]\ntag = "a-{date}"\nbase = "main"\non = "weekly"\n', "`on` is one of")
        self.assertRejects('[build.a]\ntag = "a-{date}"\nbase = "main"\npublish = "no"\n', "true or false")


class SeriesTest(unittest.TestCase):
    def test_dated_series_takes_same_day_suffixes(self):
        regex = builds.series_regex("ea-x-{date}")
        self.assertTrue(regex.match("ea-x-261101"))
        self.assertTrue(regex.match("ea-x-261101.3"))
        # A longer label is a different series, though it shares the prefix.
        self.assertFalse(regex.match("ea-x-y-261101"))

    def test_version_series_captures_the_version(self):
        m = builds.series_regex("v{version}").match("v1.20.3")
        self.assertEqual(m.group("version"), "1.20.3")
        self.assertIsNone(builds.series_regex("v{version}").match("v1.2"))
        self.assertIsNone(builds.series_regex("v{version}").match("v1.2.3.4"))

    def test_literal_suffixed_version_finds_its_own_releases(self):
        build = {"tag": "ea-x-{version}", "version": "1.0.0-rc.1"}
        found = builds.series_releases(build, [{"tag_name": "ea-x-1.0.0-rc.1"}, {"tag_name": "ea-y-1.0.0"}])
        self.assertEqual([r["tag_name"] for r in found], ["ea-x-1.0.0-rc.1"])
        # A tags build still only takes plain X.Y.Z.
        self.assertEqual(builds.series_releases(RELEASE, [{"tag_name": "v1.0.0-rc.1"}]), [])

    def test_choose_tag(self):
        dated = {"tag": "nightly-{date}"}
        values = {"date": "261101", "version": "", "zellij": "0.46"}
        self.assertEqual(builds.choose_tag(dated, values, set()), "nightly-261101")
        self.assertEqual(builds.choose_tag(dated, values, {"nightly-261101", "nightly-261101.2"}),
                         "nightly-261101.3")
        fixed = {"tag": "v{version}"}
        with self.assertRaises(Skip):
            builds.choose_tag(fixed, {**values, "version": "1.0.0"}, {"v1.0.0"})
        rolling = {"tag": "preview-{zellij}", "rolling": True}
        self.assertEqual(builds.choose_tag(rolling, values, {"preview-0.46"}), "preview-0.46")

    def test_zellij_line(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "Cargo.toml"
            path.write_text('[package]\nname = "x"\n[dependencies]\nzellij-tile = "0.46.0"\n')
            self.assertEqual(builds.zellij_line(path), "0.46")
            path.write_text('[package]\nname = "x"\n[dependencies]\nzellij-tile = { version = "^1.2", features = [] }\n')
            self.assertEqual(builds.zellij_line(path), "1.2")
            path.write_text('[package]\nname = "x"\n')
            with self.assertRaises(Fail):
                builds.zellij_line(path)


class ReleasesTest(unittest.TestCase):
    RELEASES = [
        {"tag_name": "nightly-261030", "created_at": "2026-10-30T05:00:00Z"},
        {"tag_name": "v0.5.0", "created_at": "2026-10-01T00:00:00Z"},
        {"tag_name": "nightly-261101", "created_at": "2026-11-01T05:00:00Z"},
        {"tag_name": "nightly-261031", "created_at": "2026-10-31T05:00:00Z"},
        {"tag_name": "nightly", "created_at": "2026-09-01T05:00:00Z"},
    ]

    def test_load_releases_reads_arrays_and_lines(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "releases.json"
            path.write_text(json.dumps(self.RELEASES))
            self.assertEqual(len(builds.load_releases(str(path))), 5)
            path.write_text("\n".join(json.dumps(r) for r in self.RELEASES) + "\n")
            self.assertEqual(len(builds.load_releases(str(path))), 5)
            path.write_text("")
            self.assertEqual(builds.load_releases(str(path)), [])
            path.write_text('{"name": "x"}\n')
            with self.assertRaises(Fail):
                builds.load_releases(str(path))
        self.assertEqual(builds.load_releases(None), [])

    def test_series_releases_are_newest_first_and_exclude_other_series(self):
        tags = [r["tag_name"] for r in builds.series_releases({"tag": "nightly-{date}"}, self.RELEASES)]
        self.assertEqual(tags, ["nightly-261101", "nightly-261031", "nightly-261030"])

    def test_prune_keeps_the_newest_counting_the_one_just_published(self):
        build = {"tag": "nightly-{date}", "keep": 2}
        self.assertEqual(builds.prune(build, self.RELEASES, "nightly-261102"),
                         ["nightly-261031", "nightly-261030"])
        # The list may already include the release just made.
        self.assertEqual(builds.prune(build, self.RELEASES, "nightly-261101"), ["nightly-261030"])
        self.assertEqual(builds.prune({"tag": "nightly-{date}"}, self.RELEASES, "x"), [])

    def test_inputs_round_trip_through_release_notes(self):
        inputs = {"base": "abc", "stack": ["def"]}
        body = "notes\n\n" + builds.inputs_comment(inputs) + "\n"
        self.assertEqual(builds.release_inputs({"body": body}), inputs)
        self.assertIsNone(builds.release_inputs({"body": "no comment"}))
        self.assertIsNone(builds.release_inputs({"body": None}))


class GitTestCase(unittest.TestCase):
    """A bare `origin` and a clone of it, with main holding a Cargo.toml."""

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)
        self._env = {k: os.environ.get(k) for k in GIT_ENV}
        os.environ.update(GIT_ENV)
        self.origin = self.tmp / "origin.git"
        self.repo = self.tmp / "clone"
        self.run_git("init", "--quiet", "--bare", "--initial-branch=main", str(self.origin))
        self.run_git("clone", "--quiet", str(self.origin), str(self.repo))
        self.run_git("switch", "--quiet", "-c", "main", cwd=self.repo)
        self.commit("Cargo.toml", '[package]\nname = "zjhints"\nversion = "0.1.0"\n'
                    '[dependencies]\nzellij-tile = "0.46.0"\n')
        self.run_git("push", "--quiet", "origin", "main", cwd=self.repo)

    def tearDown(self):
        for k, v in self._env.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        self._tmp.cleanup()

    def run_git(self, *args: str, cwd: Path | None = None) -> str:
        return subprocess.run(["git", *args], cwd=cwd, check=True, text=True,
                              capture_output=True).stdout.strip()

    def commit(self, path: str, text: str, message: str | None = None) -> str:
        (self.repo / path).write_text(text)
        self.run_git("add", path, cwd=self.repo)
        self.run_git("commit", "--quiet", "-m", message or f"change {path}", cwd=self.repo)
        return self.run_git("rev-parse", "HEAD", cwd=self.repo)

    def push(self, *refs: str) -> None:
        self.run_git("push", "--quiet", "--force", "origin", *refs, cwd=self.repo)

    def tag(self, name: str, ref: str = "HEAD") -> None:
        self.run_git("tag", "-a", name, "-m", name, ref, cwd=self.repo)
        self.push(f"refs/tags/{name}")

    def args(self, **overrides) -> argparse.Namespace:
        defaults = dict(repo=str(self.repo), worktree=None, rerere=str(self.tmp / "no-rerere"),
                        releases=None, tag_ref=None, date="261101", only_if_moved=False,
                        trial=False)
        defaults.update(overrides)
        if defaults["worktree"] is None:
            defaults["worktree"] = str(self.tmp / f"wt-{len(list(self.tmp.iterdir()))}")
        return argparse.Namespace(**defaults)

    def releases(self, *entries: dict) -> str:
        path = self.tmp / "releases.json"
        path.write_text(json.dumps(list(entries)))
        return str(path)


RELEASE = {"tag": "v{version}", "version": "tags", "base": "main", "alias": "zellij-{zellij}"}
NIGHTLY = {"tag": "nightly-{date}", "base": "main", "on": "nightly", "keep": 7}


class VersionFromTagsTest(GitTestCase):
    def test_newest_tag_on_base_is_released(self):
        first = self.run_git("rev-parse", "HEAD", cwd=self.repo)
        self.tag("v0.9.0")
        second = self.commit("a.txt", "a")
        self.tag("v0.10.0")
        self.commit("b.txt", "b")
        self.push("main")
        result = builds.assemble("release", RELEASE, self.args())
        self.assertEqual(result["tag"], "v0.10.0")
        self.assertEqual(result["version"], "0.10.0")
        # The tagged commit, not main's newer tip.
        self.assertEqual(result["sha"], second)
        self.assertEqual(result["base"], {"ref": "v0.10.0", "sha": second})
        self.assertEqual(result["alias"], "zellij-0.46")
        self.assertNotEqual(first, second)

    def test_pushed_tag_wins_over_newer_ones(self):
        old = self.commit("a.txt", "a")
        self.tag("v1.0.0")
        self.commit("b.txt", "b")
        self.tag("v1.1.0")
        self.push("main")
        result = builds.assemble("release", RELEASE, self.args(tag_ref="refs/tags/v1.0.0"))
        self.assertEqual((result["tag"], result["sha"]), ("v1.0.0", old))

    def test_tag_off_base_is_rejected(self):
        self.run_git("switch", "--quiet", "-c", "side", cwd=self.repo)
        self.commit("side.txt", "side")
        self.tag("v2.0.0")
        with self.assertRaises(Fail) as caught:
            builds.assemble("release", RELEASE, self.args(tag_ref="refs/tags/v2.0.0"))
        self.assertIn("not on main", str(caught.exception))
        # Nor is it picked up as the newest tag when no tag was pushed.
        with self.assertRaises(Skip):
            builds.assemble("release", RELEASE, self.args())

    def test_pushed_tag_must_match_the_pattern(self):
        self.tag("nightly-261101")
        with self.assertRaises(Fail):
            builds.assemble("release", RELEASE, self.args(tag_ref="refs/tags/nightly-261101"))

    def test_already_released_is_skipped_unless_trial(self):
        self.tag("v1.0.0")
        releases = self.releases({"tag_name": "v1.0.0", "created_at": "2026-10-01T00:00:00Z"})
        with self.assertRaises(Skip) as caught:
            builds.assemble("release", RELEASE, self.args(releases=releases))
        self.assertIn("already released", str(caught.exception))
        result = builds.assemble("release", RELEASE, self.args(releases=releases, trial=True))
        self.assertEqual(result["tag"], "v1.0.0")

    def test_no_matching_tag_is_skipped(self):
        self.tag("zellij-0.46")
        with self.assertRaises(Skip) as caught:
            builds.assemble("release", RELEASE, self.args())
        self.assertIn("Push one to release", str(caught.exception))

    def test_previous_release_is_reported_for_the_changelog(self):
        self.tag("v1.0.0")
        self.commit("a.txt", "a")
        self.tag("v1.1.0")
        self.push("main")
        releases = self.releases({"tag_name": "v1.0.0", "created_at": "2026-10-01T00:00:00Z"},
                                 {"tag_name": "zellij-0.46", "created_at": "2026-10-02T00:00:00Z"})
        result = builds.assemble("release", RELEASE, self.args(releases=releases))
        self.assertEqual((result["tag"], result["previous"]), ("v1.1.0", "v1.0.0"))


class AssembleTest(GitTestCase):
    def test_literal_version_and_dated_tags(self):
        build = {"tag": "fork-v{version}", "version": "1.2.3", "base": "main"}
        result = builds.assemble("fork", build, self.args())
        self.assertEqual((result["tag"], result["version"]), ("fork-v1.2.3", "1.2.3"))
        self.assertFalse(result["prerelease"])
        self.tag("fork-v1.2.3")
        with self.assertRaises(Skip):
            builds.assemble("fork", build, self.args())

        self.tag("nightly-261101")
        result = builds.assemble("nightly", NIGHTLY, self.args())
        self.assertEqual(result["tag"], "nightly-261101.2")
        self.assertTrue(result["prerelease"])
        self.assertEqual(result["keep"], 7)

        # A build with no version is a prerelease unless it says otherwise.
        result = builds.assemble("ea-x", {"tag": "ea-x-{date}", "base": "main"}, self.args())
        self.assertTrue(result["prerelease"])

    def test_skipped_build_leaves_no_worktree(self):
        build = {"tag": "fork-v{version}", "version": "1.2.3", "base": "main"}
        self.tag("fork-v1.2.3")
        args = self.args()
        with self.assertRaises(Skip):
            builds.assemble("fork", build, args)
        self.assertFalse(Path(args.worktree).exists())

    def test_stack_merges_in_order(self):
        base = self.run_git("rev-parse", "HEAD", cwd=self.repo)
        self.run_git("switch", "--quiet", "-c", "feat/a", cwd=self.repo)
        a = self.commit("a.txt", "a")
        self.run_git("switch", "--quiet", "-c", "feat/b", "main", cwd=self.repo)
        b = self.commit("b.txt", "b")
        self.push("feat/a", "feat/b")
        build = {"tag": "ea-x-{date}", "base": "main", "stack": ["feat/a", "origin:feat/b"]}
        result = builds.assemble("ea-x", build, self.args())
        worktree = Path(result["worktree"])
        self.assertTrue((worktree / "a.txt").exists() and (worktree / "b.txt").exists())
        self.assertEqual([s["sha"] for s in result["stack"]], [a, b])
        self.assertTrue(all(s["merged"] for s in result["stack"]))
        self.assertEqual(json.loads(result["inputs"].split(" ", 2)[2].rsplit(" ", 1)[0]),
                         {"base": base, "stack": [a, b]})

    def test_branch_already_in_base_is_flagged(self):
        self.run_git("switch", "--quiet", "-c", "feat/a", cwd=self.repo)
        self.commit("a.txt", "a")
        self.push("feat/a")
        self.run_git("switch", "--quiet", "main", cwd=self.repo)
        self.run_git("merge", "--quiet", "--ff-only", "feat/a", cwd=self.repo)
        self.push("main")
        build = {"tag": "ea-x-{date}", "base": "main", "stack": ["feat/a"]}
        result = builds.assemble("ea-x", build, self.args())
        self.assertFalse(result["stack"][0]["merged"])

    def test_conflict_fails_with_a_recipe(self):
        self.run_git("switch", "--quiet", "-c", "feat/a", cwd=self.repo)
        self.commit("same.txt", "from a\n")
        self.run_git("switch", "--quiet", "main", cwd=self.repo)
        self.commit("same.txt", "from main\n")
        self.push("main", "feat/a")
        build = {"tag": "ea-x-{date}", "base": "main", "stack": ["feat/a"]}
        with self.assertRaises(Fail) as caught:
            builds.assemble("ea-x", build, self.args())
        self.assertIn("same.txt", str(caught.exception))
        self.assertIn("builds.py record", str(caught.exception))

    def test_record_saves_only_this_merges_resolutions(self):
        self.run_git("switch", "--quiet", "-c", "feat/a", cwd=self.repo)
        self.commit("same.txt", "from a\n")
        self.run_git("switch", "--quiet", "main", cwd=self.repo)
        self.commit("same.txt", "from main\n")
        self.push("main", "feat/a")
        # The reader's own resolution of some unrelated merge.
        stray = self.repo / ".git" / "rr-cache" / ("0" * 40)
        stray.mkdir(parents=True)
        (stray / "preimage").write_text("x\n")
        (stray / "postimage").write_text("y\n")
        args = self.args()
        with self.assertRaises(Fail):
            builds.assemble("ea-x", {"tag": "ea-x-{date}", "base": "main", "stack": ["feat/a"]}, args)
        (Path(args.worktree) / "same.txt").write_text("from both\n")
        dest = self.tmp / "saved"
        builds.cmd_record(argparse.Namespace(worktree=args.worktree, rerere=str(dest)))
        saved = [d.name for d in dest.iterdir()]
        self.assertEqual(len(saved), 1)
        self.assertNotIn("0" * 40, saved)
        self.assertTrue((dest / saved[0] / "postimage").is_file())

    def test_unchanged_inputs_skip_when_asked(self):
        sha = self.run_git("rev-parse", "HEAD", cwd=self.repo)
        body = builds.inputs_comment({"base": sha, "stack": []})
        releases = self.releases({"tag_name": "nightly-261031", "body": body,
                                  "created_at": "2026-10-31T05:00:00Z"})
        with self.assertRaises(Skip) as caught:
            builds.assemble("nightly", NIGHTLY, self.args(releases=releases, only_if_moved=True))
        self.assertIn("nothing has moved since nightly-261031", str(caught.exception))
        # Without the flag, as for a manual run or a config change, it builds.
        result = builds.assemble("nightly", NIGHTLY, self.args(releases=releases))
        self.assertEqual(result["previous"], "nightly-261031")
        # Once main moves, the scheduled run builds too.
        self.commit("a.txt", "a")
        self.push("main")
        result = builds.assemble("nightly", NIGHTLY, self.args(releases=releases, only_if_moved=True))
        self.assertEqual(result["tag"], "nightly-261101")

    def test_rolling_tag_rebuilds_and_counts_from_its_own_release(self):
        build = {"tag": "preview", "base": "main", "rolling": True}
        self.tag("preview")
        releases = self.releases({"tag_name": "preview", "created_at": "2026-10-31T05:00:00Z"})
        result = builds.assemble("preview", build, self.args(releases=releases))
        self.assertEqual((result["tag"], result["previous"], result["rolling"]),
                         ("preview", "preview", True))

    def test_stack_can_move_the_zellij_line(self):
        self.run_git("switch", "--quiet", "-c", "feat/zellij", cwd=self.repo)
        self.commit("Cargo.toml", '[package]\nname = "zjhints"\nversion = "0.1.0"\n'
                    '[dependencies]\nzellij-tile = "0.47.0"\n')
        self.push("feat/zellij")
        build = {"tag": "ea-z-{zellij}-{date}", "base": "main", "stack": ["feat/zellij"]}
        result = builds.assemble("ea-z", build, self.args())
        self.assertEqual((result["zellij"], result["tag"]), ("0.47", "ea-z-0.47-261101"))


class PlanTest(GitTestCase):
    CONFIG = textwrap.dedent("""\
        [build.release]
        tag = "v{version}"
        version = "tags"
        base = "main"

        [build.nightly]
        tag = "nightly-{date}"
        base = "main"
        on = "nightly"

        [build.ea-x]
        tag = "ea-x-{date}"
        base = "main"
    """)

    def plan(self, text: str | None = None, **overrides) -> list[str]:
        args = argparse.Namespace(**{"event": "push", "ref": "", "before": "", "only": "all",
                                     "config": "builds.toml", **overrides})
        return builds.plan(builds.load_config(text or self.CONFIG, "builds.toml"), args)

    def test_events(self):
        self.assertEqual(self.plan(event="schedule"), ["nightly"])
        self.assertEqual(self.plan(event="workflow_dispatch"), ["release", "nightly", "ea-x"])
        self.assertEqual(self.plan(event="workflow_dispatch", only="ea-x, nightly"), ["ea-x", "nightly"])
        with self.assertRaises(Fail):
            self.plan(event="workflow_dispatch", only="nope")

    def test_tag_push_plans_only_matching_tags_builds(self):
        self.assertEqual(self.plan(ref="refs/tags/v1.2.3"), ["release"])
        self.assertEqual(self.plan(ref="refs/tags/ea-x-261101"), [])
        self.assertEqual(self.plan(ref="refs/tags/v1.2"), [])

    def test_branch_push_plans_changed_tables(self):
        # A push that created the branch counts every table as changed.
        self.assertEqual(self.plan(ref="refs/heads/main", before="0" * 40), ["release", "nightly", "ea-x"])
        original = builds.REPO_ROOT
        builds.REPO_ROOT = self.repo
        try:
            before = self.commit("builds.toml", self.CONFIG)
            changed = self.CONFIG.replace('tag = "ea-x-{date}"', 'tag = "ea-x-{date}"\nkeep = 2')
            self.assertEqual(self.plan(changed, ref="refs/heads/main", before=before), ["ea-x"])
            self.assertEqual(self.plan(ref="refs/heads/main", before=before), [])
            # The previous file is only compared, so one the current rules
            # reject, or that no longer parses, does not stop the plan.
            before = self.commit("builds.toml", self.CONFIG + 'frobnicate = 1\n')
            self.assertEqual(self.plan(ref="refs/heads/main", before=before), ["ea-x"])
            before = self.commit("builds.toml", "[build\n")
            self.assertEqual(self.plan(ref="refs/heads/main", before=before), ["release", "nightly", "ea-x"])
        finally:
            builds.REPO_ROOT = original


if __name__ == "__main__":
    unittest.main()
