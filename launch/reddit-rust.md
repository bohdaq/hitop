# r/rust (draft, not posted)

**Title:** hitop 1.0: an API client and CI runner in Rust, with collections as YAML files in your repo

**Body:**

I've been building hitop, an HTTP API client whose main surface is a single Rust binary. 1.0.0 is
out, with the collection format frozen as stable:

- Collections are folders of YAML files, one per request, so they live in the repo and change in
  pull requests. YAML via `yaml_serde`; a proptest round trip, a v1 compatibility suite and
  `cargo fuzz` targets for the YAML reader and the Postman and OpenAPI importers keep it stable.
- `hitop run` runs a folder with pre and post request JavaScript (boa_engine 0.22, so no Node or V8
  in the binary), environments, data files and iterations, and writes JUnit and a versioned JSON
  report. Exit codes 0/1/2.
- `hitop import` reads Postman exports, including a `pm.*` compatibility layer written in JS and
  run by boa.
- Secrets go to the OS keychain through `keyring` 4 (or env vars in CI) and are masked everywhere.
- `hitop mcp` is an MCP server over stdio built on `rmcp`.
- `hitop ws` (tokio-tungstenite) and `hitop grpc` (tonic with prost-reflect and protox, so
  `.proto` files are parsed at run time with no `protoc`, or the schema comes from reflection).
- `hitop import openapi` and `hitop openapi sync` (a three-way merge per field, so your edits
  survive a new spec version).
- reqwest 0.12 blocking with rustls, static musl builds for Linux, a distroless image on ghcr.io.
  The same core crate (`hitop-core`) compiles to wasm32 for the web app's format code and powers
  the Tauri desktop app.

Install: `cargo install hitop` (or `cargo binstall hitop`, or the release binaries).

Repo: https://github.com/bohdaq/hitop · crate: https://crates.io/crates/hitop

Feedback on the crate split (`hitop-core` as a library) and on boa for user scripts is very welcome.
