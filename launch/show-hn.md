# Show HN (draft, not posted)

**Title** (80 characters max):

Show HN: Hitop – an API client whose collections are YAML files in your repo

**URL:** https://github.com/bohdaq/hitop

**First comment (from the author):**

Hi HN, I'm Bohdan. hitop is an HTTP API client built around one idea: a collection should be a
folder of plain files in the repository it tests, and the same binary that runs it on my laptop
should run it in CI.

- A collection is `hitop.yaml` plus one YAML file per request (folders are directories), with keys
  written in a fixed order so diffs stay small. The format has a spec (CC BY 4.0).
- `hitop import` reads Postman collections, environments and globals. Postman scripts keep running
  through a `pm.*` layer (`pm.test`, `pm.expect`, `pm.environment.set`, ...), and the import lists,
  by line, anything it can't run, such as `pm.sendRequest`.
- `hitop run ./api --env staging --reporter junit,json` gives exit codes 0/1/2 and a JUnit file where
  each testcase points at its request file, so CI annotations land on the right file. There's a
  GitHub Action, a GitLab template and a distroless Docker image.
- Secrets are marked in the environment file and never written there: values come from the OS
  keychain or `HITOP_SECRET_<ENV>_<NAME>`, and are masked in output and reports.
- `hitop mcp ./api` is an MCP server, so Claude Code or Cursor can list, run and edit requests
  without seeing secret values.
- There is also a web app (no install; it sends public requests through a small proxy so CORS isn't
  a problem) and a Tauri desktop app.

It's a single Rust binary (reqwest with rustls, boa for JavaScript), MIT licensed, no account. It
does HTTP only for now; GraphQL, WebSocket and gRPC are next.

What I'd most like to hear: which Postman scripts break on import, and what your CI needs that the
report doesn't give you yet.

Comparisons with Postman, Bruno, Insomnia and Yaak, with sources:
https://hitop.rws8.tech/terms/hitop-vs-postman.html
