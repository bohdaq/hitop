# Launch texts (drafts)

Drafts for the owner to edit and post. Nothing here has been posted anywhere. Post once Phase 1 is
live: CLI 0.3.0 on crates.io, desktop 0.2.0 on GitHub Releases, the site deployed with the
comparison pages and guides, and the demo GIF recorded (`vhs site/demo/demo.tape`).

| File | Where | Notes |
| --- | --- | --- |
| `show-hn.md` | news.ycombinator.com/submit | Title plus a first comment from the author. Weekday morning US time. |
| `reddit-rust.md` | r/rust | Lead with the Rust details. Flair: "🛠️ project". |
| `reddit-webdev.md` | r/webdev | Lead with the Postman migration and the web app. Check the subreddit's self-promotion rules first. |
| `this-week-in-rust.md` | PR to rust-lang/this-week-in-rust | One line under "Project/Tooling Updates" in the next draft issue. |
| `product-hunt.md` | producthunt.com | Tagline, description, first comment, gallery list. |

Facts to keep true when editing: no account; MIT (sync server AGPL, not launched); the CLI and
the free desktop builds have an anonymous weekly usage ping that is off unless turned on, and
the web app and App Store builds don't contain it; HTTP only (GraphQL, WebSocket and gRPC are
planned, not shipped); Postman scripts run through a `pm.*` layer, and `pm.sendRequest`,
`postman.setNextRequest`, `require()` and OAuth 2.0 are not supported yet.
