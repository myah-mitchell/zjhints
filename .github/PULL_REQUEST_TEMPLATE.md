<!-- markdownlint-disable-file MD041 (this is a body fragment GitHub inlines, not a document) -->
<!--
Thanks for the pull request. The sections below are a prompt, not a form:
delete what does not apply. For a one-line fix, the summary alone is fine.
-->

## What this changes

<!-- The "why" more than the "what"; the diff already shows the what. -->

## Why

<!--
What problem does this solve? If it fixes an issue, link it:
  Fixes #123
-->

## How it was verified

<!--
Rendering bugs here have a habit of being something other than they appear:
a glyph that does not render in one terminal, a plugin cached from a previous
session, a version mismatch showing up as a wrong label. Saying what you
actually observed helps.
-->

- [ ] `make check` passes (fmt, clippy, tests, release build)
- [ ] Tried it in a new Zellij session: plugins are cached per session, so detach/reattach does not reload
- [ ] Added or updated tests for anything with logic worth reasoning about

Modes exercised, if the change affects rendering:

<!-- e.g. Normal, Pane, Tab, Locked; with discover_hints both on and off -->

## Documentation

- [ ] New config options have a bullet in the README's Configuration list and a prose section
- [ ] Not applicable

## Anything else

<!--
Trade-offs, alternatives you rejected, things you are unsure about, or parts
you would like a closer look at. Uncertainty flagged here is more useful than
uncertainty discovered later.
-->

---

<!--
A note on commit subjects: release notes are generated with git-cliff, so the
conventional-commit prefix decides where a change appears in the changelog.

  feat:  fix:  docs:  test:  ci:  chore(deps):  refactor:  perf:

Pull requests are merged with a merge commit, not squashed, so each of your
commits keeps its own subject and is parsed into the changelog individually,
not just the PR title. Give each commit a conventional prefix, not only the
PR title. See CONTRIBUTING.md.
-->
