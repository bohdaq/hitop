---
title: "HITOP vs Yaak"
description: "How HITOP and Yaak compare on licensing, price, Git sync, the CLI, CI reports and Postman import. Facts checked on 2026-11-01, with sources."
---

## In short

Yaak is a fast desktop API client by the author of Insomnia, with REST, GraphQL, gRPC, WebSocket and Server-Sent Events, plugins, and directory sync to YAML files for Git. Its source is MIT; its prebuilt app is free for personal use and needs a license at work. HITOP is a smaller tool: an HTTP client and collection runner around a YAML folder format, with a CLI made for CI and a web app.

**Checked on 2026-11-01** against the pages listed under [Sources](#sources).

## Side by side

| | HITOP | Yaak |
| --- | --- | --- |
| **Price** | Free: CLI, web app, desktop builds on GitHub, also at work. Mac App Store app $4.99 and iOS $2.99, paid once | Free for personal use; at work Individual $79 per year or $399 once, Business $149 per user per year |
| **License** | MIT, binaries included | Source MIT; licensing applies to the prebuilt binaries |
| **Account** | None | None for the trial; a purchased license is tied to a Yaak account |
| **Files for Git** | The collection folder is the source: `hitop.yaml`, one YAML file per request, `environments/` | Directory sync mirrors a workspace to YAML files, one per resource (`yaak.<id>.yaml`), with Git actions in the app; environments are left out unless marked sharable |
| **CLI** | `hitop`: runs collections for CI with reports and exit codes | `@yaakapp/cli` on npm (written in Rust): manages workspaces, folders, environments and requests, sends workspaces and folders (`--parallel`, `--fail-fast`) |
| **CI reports** | JUnit XML and a versioned JSON report; GitHub Action and GitLab template | Not described in the CLI docs |
| **Postman import** | Collections, environments and globals; `pm.*` scripts through a compatibility layer, unsupported calls listed by line | A Postman migration guide; imports Postman, Insomnia, OpenAPI, Swagger and curl |
| **Secrets** | OS keychain or environment variables | OS keychain, secrets encryption |
| **Protocols** | HTTP. GraphQL, WebSocket and gRPC are planned | REST, GraphQL, gRPC, WebSocket, Server-Sent Events |
| **AI tools** | `hitop mcp`: an MCP server over stdio for a collection folder | An MCP server and an agent skill for the CLI (docs) |
| **Plugins** | None | Plugins for auth, template tags and the UI |

## The difference in practice

Yaak's files are a mirror of its app database, written by the app. HITOP's folder is the collection itself: you can write a request file by hand, review it in a pull request and run it with `hitop run` in CI, and the format has a published spec. Yaak has more protocols and a plugin system today.

## Sources

- Yaak pricing and license: https://yaak.app/pricing
- Yaak repository: https://github.com/mountain-loop/yaak (MIT source, protocols, keychain, plugins)
- Yaak CLI: https://yaak.app/docs/getting-started/cli-usage
- Yaak directory and Git sync: https://yaak.app/docs/collaboration/local-directory-git-sync
- Yaak docs index (migration guides, MCP server): https://yaak.app/docs
- HITOP: https://github.com/bohdaq/hitop (MIT)
