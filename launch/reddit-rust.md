# r/rust (draft, not posted)

**Title:** hitop 0.3: an API client and CI runner in Rust, with collections as YAML files in your repo

**Body:**

I've been building hitop, an HTTP API client whose main surface is a single Rust binary. 0.3.0 is
the first release I'd call ready for real use:

- Collections are folders of YAML files, one per request, so they live in the repo and change in
  pull requests. YAML via `yaml_serde`; a proptest round trip keeps the format stable.
- `hitop run` runs a folder with pre and post request JavaScript (boa_engine 0.22, so no Node or V8
  in the binary), environments, data files and iterations, and writes JUnit and a versioned JSON
  report. Exit codes 0/1/2.
- `hitop import` reads Postman exports, including a `pm.*` compatibility layer written in JS and
  run by boa.
- Secrets go to the OS keychain through `keyring` 4 (or env vars in CI) and are masked everywhere.
- `hitop mcp` is an MCP server over stdio built on `rmcp`.
- reqwest 0.12 blocking with rustls, static musl builds for Linux, a distroless image on ghcr.io.
  The same core crate (`hitop-core`) compiles to wasm32 for the web app's format code and powers
  the Tauri desktop app.

Install: `cargo install hitop` (or `cargo binstall hitop`, or the release binaries).

Repo: https://github.com/bohdaq/hitop · crate: https://crates.io/crates/hitop

Feedback on the crate split (`hitop-core` as a library) and on boa for user scripts is very welcome.
