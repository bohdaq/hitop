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
| **Price** | Free: CLI, web app, desktop builds on GitHub, also at work. Mac App Store app $4.99 and iOS $2.99, paid once; [HITOP Cloud](/pricing.html) (optional): free for 3 people, Pro $6 a month, Team $10 per seat a month (billed yearly) | Free for personal use; at work Individual $79 per year or $399 once, Business $149 per user per year |
| **License** | MIT, binaries included | Source MIT; licensing applies to the prebuilt binaries |
| **Account** | None for the apps and the CLI; HITOP Cloud is a sign-in (email, Google or GitHub) | None for the trial; a purchased license is tied to a Yaak account |
| **Files for Git** | The collection folder is the source: `hitop.yaml`, one YAML file per request, `environments/`. With HITOP Cloud, the web app saves a collection to a GitHub repository on every change | Directory sync mirrors a workspace to YAML files, one per resource (`yaak.<id>.yaml`), with Git actions in the app; environments are left out unless marked sharable |
| **CLI** | `hitop`: runs collections for CI with reports and exit codes | `@yaakapp/cli` on npm (written in Rust): manages workspaces, folders, environments and requests, sends workspaces and folders (`--parallel`, `--fail-fast`) |
| **CI reports** | JUnit XML, a versioned JSON report and a self-contained HTML page; GitHub Action, GitLab template, `hitop schedule` for scheduled runs | Not described in the CLI docs |
| **Postman import** | Collections, environments and globals; `pm.*` scripts through a compatibility layer, unsupported calls listed by line. Also OpenAPI 3.0 and 3.1 import and sync | A Postman migration guide; imports Postman, Insomnia, OpenAPI, Swagger and curl |
| **Secrets** | OS keychain or environment variables; references to Vault or OpenBao, AWS Secrets Manager, Azure Key Vault and 1Password | OS keychain, secrets encryption |
| **Protocols** | HTTP everywhere. GraphQL (with schema completion) in the CLI and web app; WebSocket in the CLI and web app (a browser can't send handshake headers); gRPC, unary and server streaming, in the CLI. The desktop app gets all of them with its new UI (the web app in Tauri, in testing) | REST, GraphQL, gRPC, WebSocket, Server-Sent Events |
| **AI tools** | `hitop mcp`: an MCP server over stdio for a collection folder | An MCP server and an agent skill for the CLI (docs) |
| **Plugins** | None | Plugins for auth, template tags and the UI |

## The difference in practice

Yaak's files are a mirror of its app database, written by the app. HITOP's folder is the collection itself: you can write a request file by hand, review it in a pull request and run it with `hitop run` in CI, and the format has a published spec. Yaak has Server-Sent Events, and WebSocket and gRPC in its desktop app, plus a plugin system, today.

## Sources

- Yaak pricing and license: https://yaak.app/pricing
- Yaak repository: https://github.com/mountain-loop/yaak (MIT source, protocols, keychain, plugins)
- Yaak CLI: https://yaak.app/docs/getting-started/cli-usage
- Yaak directory and Git sync: https://yaak.app/docs/collaboration/local-directory-git-sync
- Yaak docs index (migration guides, MCP server): https://yaak.app/docs
- HITOP: https://github.com/bohdaq/hitop (MIT)
