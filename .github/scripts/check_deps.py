#!/usr/bin/env python3
"""Report what `cargo update` changed, and what it deliberately did not.

`cargo update` only moves within the semver range each Cargo.toml entry
allows. For a `0.x` dependency that means patch releases only, so
`zellij-tile = "0.44.3"` will accept 0.44.4 but never 0.45.0 — exactly the
policy this project wants, since a zellij-tile that outpaces the running
Zellij breaks hints silently rather than failing to build.

That leaves one gap: nobody is told a new minor exists. This script queries
crates.io for each direct dependency, reports the versions cargo took, and
flags the ones held back — loudly for the Zellij crates, since acting on
those means upgrading Zellij first. It also reports exactly which version was
held back for those two crates specifically, so a caller can propose the bump
itself (see the zellij-upgrade job in update-deps.yml) without re-querying
crates.io.

Writes a markdown summary to stdout and sets outputs on $GITHUB_OUTPUT.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import urllib.request

# Bumping these means matching the running Zellij, so they are never taken
# past a minor automatically.
PINNED_TO_ZELLIJ = {"zellij-tile", "zellij-tile-utils"}

CRATES_IO = "https://crates.io/api/v1/crates/{}"
USER_AGENT = "zjhints-ci (github actions dependency check)"


def direct_dependencies(cargo_toml: str) -> dict[str, str]:
    """Crate name -> version requirement, from the [dependencies] table."""
    deps: dict[str, str] = {}
    in_table = False
    for line in cargo_toml.splitlines():
        stripped = line.strip()
        if stripped.startswith("["):
            in_table = stripped == "[dependencies]"
            continue
        if not in_table or not stripped or stripped.startswith("#"):
            continue
        match = re.match(r'^([A-Za-z0-9_-]+)\s*=\s*"([^"]+)"', stripped)
        if match:
            deps[match.group(1)] = match.group(2)
    return deps


def locked_versions(cargo_lock: str) -> dict[str, str]:
    """Crate name -> resolved version, from Cargo.lock."""
    versions: dict[str, str] = {}
    name = None
    for line in cargo_lock.splitlines():
        stripped = line.strip()
        if stripped.startswith('name = "'):
            name = stripped.split('"')[1]
        elif stripped.startswith('version = "') and name:
            versions[name] = stripped.split('"')[1]
            name = None
    return versions


def latest_release(crate: str) -> str | None:
    """Newest non-yanked, non-prerelease version on crates.io."""
    request = urllib.request.Request(
        CRATES_IO.format(crate), headers={"User-Agent": USER_AGENT}
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            payload = json.load(response)
    except Exception as error:  # network flakes must not fail the run
        print(f"::warning::could not reach crates.io for {crate}: {error}", file=sys.stderr)
        return None

    candidates = [
        version["num"]
        for version in payload.get("versions", [])
        if not version.get("yanked") and "-" not in version["num"]
    ]
    return candidates[0] if candidates else None


def as_tuple(version: str) -> tuple[int, ...]:
    return tuple(int(part) for part in re.findall(r"\d+", version)[:3])


def same_minor(left: str, right: str) -> bool:
    return as_tuple(left)[:2] == as_tuple(right)[:2]


def main() -> int:
    before = json.loads(sys.argv[1]) if len(sys.argv) > 1 else {}

    cargo_toml = open("Cargo.toml").read()
    deps = direct_dependencies(cargo_toml)
    after = locked_versions(open("Cargo.lock").read())

    updated: list[str] = []
    held_back: list[str] = []
    zellij_minor_available = False
    # Latest version available for each Zellij crate that is held back a
    # minor or more, so a caller can bump Cargo.toml's requirement to exactly
    # this rather than re-querying crates.io itself.
    zellij_targets: dict[str, str] = {}

    for crate in sorted(deps):
        was, now = before.get(crate), after.get(crate)
        if was and now and was != now:
            updated.append(f"| `{crate}` | {was} | {now} |")

        latest = latest_release(crate)
        if not latest or not now:
            continue
        if as_tuple(latest) > as_tuple(now) and not same_minor(latest, now):
            note = ""
            if crate in PINNED_TO_ZELLIJ:
                zellij_minor_available = True
                zellij_targets[crate] = latest
                note = " — **upgrade Zellij first**"
            held_back.append(f"| `{crate}` | {now} | {latest}{note} |")

    lines: list[str] = []
    if updated:
        lines += [
            "### Updated",
            "",
            "| Crate | From | To |",
            "|---|---|---|",
            *updated,
            "",
        ]
    else:
        lines += ["No dependency versions changed.", ""]

    if held_back:
        lines += [
            "### Available, not applied",
            "",
            "These are a minor or major ahead, so `cargo update` left them alone.",
            "",
            "| Crate | Current | Available |",
            "|---|---|---|",
            *held_back,
            "",
        ]

    if zellij_minor_available:
        lines += [
            "> [!WARNING]",
            "> A new **minor** release of a Zellij crate is out. Do not bump it",
            "> until the Zellij you run is on the matching version. The plugin",
            "> boundary silently drops actions it cannot decode, so a mismatch",
            "> shows up as hints with wrong labels, not as a build failure.",
            "",
        ]

    summary = "\n".join(lines)
    print(summary)

    output = os.environ.get("GITHUB_OUTPUT")
    if output:
        with open(output, "a") as handle:
            handle.write(f"changed={'true' if updated else 'false'}\n")
            handle.write(
                f"zellij_minor_available={'true' if zellij_minor_available else 'false'}\n"
            )
            handle.write(f"zellij_tile_latest={zellij_targets.get('zellij-tile', '')}\n")
            handle.write(
                f"zellij_tile_utils_latest={zellij_targets.get('zellij-tile-utils', '')}\n"
            )
            handle.write("summary<<SUMMARY_EOF\n")
            handle.write(summary + "\n")
            handle.write("SUMMARY_EOF\n")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
