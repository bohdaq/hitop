# hitop-core

Core engine for [hitop](https://crates.io/crates/hitop) — the HTTP API client CLI.

Provides:
- **Collection model** — typed structs matching the hitop web app JSON format
- **Variable interpolation** — `${var}` in URLs, headers, and bodies
- **JavaScript scripting** — pre/post-request scripts via [boa_engine](https://crates.io/crates/boa_engine)
- **HTTP execution** — blocking reqwest client with rustls
- **Storage** — XDG/platform-aware persistence (`~/.config/hitop/`)
- **Import/Export** — HITOP native and Postman v2.1 formats
- **Collection runner** — sequential execution with context passing between requests

## Usage

```toml
[dependencies]
hitop-core = "0.1"
```

```rust
use hitop_core::{storage, http, interpolation::interpolate};

let collections = storage::load_collections()?;
let col = &collections[0];
let url = interpolate(&col.requests[0].url, &col.variables)?;
let result = http::execute("GET", &url, &[], "")?;
println!("{}", result.body);
```

## JSON format compatibility

`hitop-core` uses the same JSON schema as the hitop web app's localStorage format.
Collections exported from the web app can be imported directly.
