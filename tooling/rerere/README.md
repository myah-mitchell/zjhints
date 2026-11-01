# Recorded conflict resolutions

Each directory here is one recorded conflict, in the layout of git's own `rr-cache`. It is named by a hash of the conflicting hunks, and holds what the file looked like mid-conflict (`preimage`) and once resolved (`postimage`).

`tooling/builds.py assemble` copies them into the repository it merges in, and git replays any that match. Add to them with `tooling/builds.py record`, as described in [docs/automation.md](../../docs/automation.md#when-a-stack-does-not-merge). A resolution stops matching once either side of the conflict changes. It is then never used again, so stale ones are harmless; delete them whenever convenient.
