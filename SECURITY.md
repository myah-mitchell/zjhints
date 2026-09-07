# Security policy

This page covers which versions of zjstatus-hints receive security fixes, how to report a vulnerability privately, and what counts as in scope for this plugin.

## Supported versions

This is a personal fork maintained on a rolling basis. Fixes go to the latest release; there are no maintenance branches for older ones.

| Version | Supported |
| --- | --- |
| 0.5.x | Yes |
| 0.4.x and older | No, upgrade to 0.5 |
| `nightly` | Built from `main`, unreleased. Fixes land here first. |

## Reporting a vulnerability

> [!IMPORTANT]
> Do not open a public issue for a security problem. Use GitHub's private reporting instead.

1. Open the [Security tab][security] of this repository.
2. Click **Report a vulnerability**.
3. Describe the issue, how to reproduce it, and what an attacker could do.

That opens a private advisory visible only to maintainers, where a fix can be discussed and prepared before anything is disclosed.

If you cannot use that, contact the maintainer through their GitHub profile and say only that you have a security report. Do not include the details in a public channel.

[security]: https://github.com/myah-mitchell/zjstatus-hints/security/advisories/new

### What to expect

- Acknowledgement within a week. This is a personal project, so please allow for that rather than assuming silence means dismissal.
- An assessment of whether it is exploitable and how serious it looks.
- A fix released as a patch version, with credit in the release notes unless you would rather not be named.

## What is in scope

This plugin is a WebAssembly module that Zellij runs. It reads your keybindings, formats a string, and renders it directly or pipes it to zjstatus. Realistic concerns:

- Malicious configuration causing unsafe behavior. Config values are rendered into the status bar, so a crafted value that escapes the intended output or causes a crash would be a bug worth reporting.
- Terminal escape sequence injection. The plugin emits ANSI sequences. A way to get arbitrary sequences into the bar via config or keybinding names could affect the surrounding terminal.
- Dependency vulnerabilities. `cargo audit` runs on every push, but an advisory that has gone unnoticed is worth flagging.
- Supply chain issues in the release pipeline. The workflows publish the wasm binary people install, so problems there affect everyone downstream.

## What is not in scope

- Zellij or zjstatus vulnerabilities. Report those to their own projects, [zellij][zellij-security] and [zjstatus][zjstatus-repo].
- Your own configuration exposing information you put in it. The plugin displays what your config tells it to.
- Anything requiring an attacker to already have write access to your `config.kdl` or your plugin directory. At that point they can run arbitrary code as you regardless.

[zellij-security]: https://github.com/zellij-org/zellij/security
[zjstatus-repo]: https://github.com/dj95/zjstatus

## Verifying what you install

Releases are built by GitHub Actions from a tagged commit (see [docs/automation.md](docs/automation.md)), so the build is reproducible from public source. If you would rather not trust the published binary, build it yourself:

```bash
git checkout v0.5.0
make build
```

The result lands at `target/wasm32-wasip1/release/zjstatus-hints.wasm`.
