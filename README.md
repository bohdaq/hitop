# HITOP

**The API client your terminal, your CI and your AI agent can all drive. One small Rust binary,
collections as files in your repo, nothing in anyone's cloud.**

HITOP sends HTTP requests, keeps them in collections, scripts them in JavaScript and runs them in
order, with JUnit and JSON reports for CI. A collection is a folder of YAML files, one per request,
so it lives next to your code, changes in pull requests and runs from the commit you are looking at.
No account, MIT licensed.

- **CLI** `hitop`: a single binary ([crates.io](https://crates.io/crates/hitop),
  [GitHub Releases](https://github.com/bohdaq/hitop/releases), `ghcr.io/bohdaq/hitop`)
- **Web app**: [hitop.rws8.tech/app](https://hitop.rws8.tech/app/), nothing to install
- **Desktop**: free builds for macOS, Windows and Linux on
  [GitHub Releases](https://github.com/bohdaq/hitop/releases), or the
  [Mac App Store](https://apps.apple.com/app/hitop-api-client/id6785294166) app

## From a Postman export to a green CI run

```bash
cargo install hitop        # or a static binary from GitHub Releases, or the Docker image

# A Postman collection, its environments and globals, into a folder in your repo
hitop import shop.postman_collection.json staging.postman_environment.json --to ./shop-api

# Run it; a secret comes from the keychain, --secret or HITOP_SECRET_<ENV>_<NAME>
hitop run ./shop-api --env Staging --reporter cli,junit,json --out reports
```

```
▶ Shop API
  ✓ POST Log in 200 (41ms)
      ✓ logged in
  ✓ GET List orders 200 (12ms)
      ✓ status is 200
      ✓ orders from this environment

Summary: 2/2 requests succeeded, 3/3 assertions passed
```

Exit code 0 means every request passed, 1 a request or check failed, 2 a usage or configuration
error. `reports/junit.xml` has one testcase per request with its file, so CI annotations land on
the request that failed.

On GitHub:

```yaml
- uses: bohdaq/hitop-action@v1
  with:
    version: 1.1.0
    path: shop-api
    env: Staging
  env:
    HITOP_SECRET_STAGING_PASSWORD: ${{ secrets.STAGING_PASSWORD }}
```

GitLab CI and any other runner: `ghcr.io/bohdaq/hitop:1.1.0` (distroless) or
`ghcr.io/bohdaq/hitop:1.1.0-debug` (with a shell).

## What's in it

- **Collections as files**: `hitop.yaml`, `requests/*.yaml` (subfolders are folders),
  `environments/*.yaml`, optional `globals.yaml`. Keys in a fixed order, so diffs stay small.
  [Format spec](spec/collection-format.md) (CC BY 4.0), stable since 1.0: version 1 only grows,
  and every reader is tested against [its compatibility suite](spec/compat/v1/).
- **Postman import**: collections v2.0 and v2.1 (folders, auth, scripts), environments and globals.
  Scripts run through a `pm.*` layer (`pm.test`, `pm.expect`, `pm.response`, `pm.environment`,
  `pm.collectionVariables`, `pm.globals`, `pm.variables`); the import lists, by line, anything it
  can't run. Export back to Postman v2.1 at any time.
- **Environments and secrets**: variables per environment; secret values live in the OS keychain
  or come from `HITOP_SECRET_*` variables, never in files, history or reports (masked as `••••`).
- **Scripts**: JavaScript before and after each request: change the request, pass values to the
  next one, assert on the response.
- **CI runner**: `hitop run` with `--env`, `--env-file`, `--var`, `--data` (CSV or JSON rows),
  `--iterations`, `--timeout`, `--bail`; reporters `cli`, `junit`, `json` and `html` (one
  self-contained page) ([report spec](spec/run-report.md)); `hitop schedule` writes a scheduled
  GitHub Actions workflow or a crontab line.
- **MCP server**: `hitop mcp ./my-api` lets Claude Code, Cursor and other MCP clients list, read,
  run, create and update requests and switch environments; secrets are never returned
  ([guide](https://hitop.rws8.tech/terms/use-hitop-with-claude-code-and-cursor.html)).
- **Protocols**: HTTP (auth presets for Bearer, Basic and API key, query parameters, JSON, form and
  multipart bodies, a cookie jar per workspace, request history); GraphQL with variables and schema
  completion (CLI and web app); WebSocket (`hitop ws`, and the web app, where a browser can't send
  handshake headers); gRPC, unary and server streaming, through server reflection or `.proto` files
  (`hitop grpc`). The desktop app gets WebSocket and gRPC with its new UI (the web app in Tauri,
  in testing).
- **OpenAPI**: `hitop import openapi` turns an OpenAPI 3.0 or 3.1 spec into a collection folder;
  `hitop openapi sync` applies a newer spec and keeps your edits
  ([guide](https://hitop.rws8.tech/terms/import-an-openapi-spec.html)).
- **Code snippets and response diff**: `hitop snippet` writes the request as curl, fetch, Python,
  Go, Java or Rust; the web app compares a response with a saved example or an earlier one.
- **Secret managers**: `${secret:vault:...}`, `${secret:aws:...}`, `${secret:azure:...}` and
  `${secret:op:...}` read HashiCorp Vault or OpenBao, AWS Secrets Manager, Azure Key Vault and
  1Password at run time
  ([guide](https://hitop.rws8.tech/terms/use-secrets-from-vault-aws-azure-and-1password.html)).
- **Git panel**: status, diff, commit, pull, push, branches and conflicts for a collection folder,
  in the desktop's new UI (in testing).
- **Privacy**: no account and no cloud. An anonymous weekly usage ping exists in the CLI and the
  free desktop builds, off unless you turn it on (`hitop telemetry status` shows what it would
  send); the web app and App Store builds don't contain it.

## Guides

- [Migrate from Postman](https://hitop.rws8.tech/terms/migrate-from-postman.html)
- [Use HITOP with Claude Code and Cursor](https://hitop.rws8.tech/terms/use-hitop-with-claude-code-and-cursor.html) (`hitop mcp`)
- [Collections in git](https://hitop.rws8.tech/terms/collections-in-git.html)
- [Run collections in CI](https://hitop.rws8.tech/terms/run-collections-in-ci.html),
  [GitHub Actions](https://hitop.rws8.tech/terms/run-hitop-in-github-actions.html),
  [GitLab CI](https://hitop.rws8.tech/terms/run-hitop-in-gitlab-ci.html)
- Comparisons, with sources and the date checked:
  [Postman](https://hitop.rws8.tech/terms/hitop-vs-postman.html),
  [Bruno](https://hitop.rws8.tech/terms/hitop-vs-bruno.html),
  [Insomnia](https://hitop.rws8.tech/terms/hitop-vs-insomnia.html),
  [Yaak](https://hitop.rws8.tech/terms/hitop-vs-yaak.html)
- The CLI's full reference: [cli/crates/hitop/README.md](cli/crates/hitop/README.md);
  [variables](VARIABLES_GUIDE.md), [scripting](SCRIPTING_GUIDE.md)

## Building from source

```bash
cd cli && cargo build --release            # target/release/hitop
cd cli && cargo test                       # CLI and hitop-core tests (local mock servers only)
cd webapp && npm ci && npm run build       # the web app (needs wasm32 and wasm-bindgen-cli 0.2.126)
cd desktop && npm ci && npx tauri build    # the desktop app
```

The repository also holds the site and request proxy (`server/`, `site/`), the self-hostable sync
server (`sync-server/`), and the older React frontend and Firefox and Chrome extensions
(`frontend/`, `extension/`, [EXTENSION_GUIDE.md](EXTENSION_GUIDE.md)), which are frozen.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md): commits are signed off
(`git commit -s`), and security issues go through [SECURITY.md](SECURITY.md).

## License

MIT, see [LICENSE](LICENSE). The sync server (`sync-server/`) is AGPL-3.0-or-later and the
collection format and run report specs (`spec/`) are CC BY 4.0. The name is covered by
[TRADEMARK.md](TRADEMARK.md).
