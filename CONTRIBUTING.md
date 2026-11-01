# Contributing to HITOP

Thanks for helping. Bug reports, fixes and features are all welcome.

## Before you start

- For a bug, open an issue with the steps to reproduce it, what you expected and what happened.
- For a larger change, open an issue first so we can agree on the approach.
- Security problems go through private reporting, not issues: see [SECURITY.md](SECURITY.md).

## Sign off your commits (DCO)

HITOP uses the [Developer Certificate of Origin](https://developercertificate.org/) instead of a
contributor agreement. By adding a `Signed-off-by` line to a commit you certify that you wrote the
change, or have the right to submit it under the project's license.

Sign off with `-s`:

```bash
git commit -s -m "Fix header parsing"
```

That adds a line with your name and email from `git config`:

```text
Signed-off-by: Your Name <you@example.com>
```

A CI check fails pull requests with unsigned commits. To sign off commits you already made:

```bash
git rebase --signoff main
git push --force-with-lease
```

## Licenses

Your contribution is published under the license of the part of the repository it changes:

| Part | License |
| --- | --- |
| Everything except the parts below (CLI, hitop-core, desktop, web app, site, extensions) | MIT ([LICENSE](LICENSE)) |
| `sync-server/` | AGPL-3.0-or-later (its own `LICENSE`) |
| `spec/` (the collection format) | CC BY 4.0 |

New Rust dependencies must pass `cargo deny check licenses` (see `deny.toml`): MIT parts may
only use permissively licensed crates.

## Development

[CLAUDE.md](CLAUDE.md) describes the layout, toolchains and the commands each part needs.
In short:

```bash
(cd cli && cargo test)                       # CLI and hitop-core
(cd server && cargo test)                    # site server and proxy
./deploy/build.sh && (cd webapp && npm run test:unit)
```

Keep pull requests focused, add tests for behaviour you change, and keep existing users' data
readable: stored JSON (web app localStorage, `~/.config/hitop/`) must load after an upgrade.

By taking part you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).
