---
title: "HITOP vs Bruno"
description: "HITOP and Bruno both keep API collections as files you can commit. How they compare on file format, CLI, CI reports, Postman import and price. Facts checked on 2026-11-01, with sources."
---

## In short

Bruno and HITOP share the main idea: collections are files in a folder, so Git is the sync and the review tool. Bruno is a desktop client with its own `.bru` format (and, since Bruno 3.0, YAML files following its OpenCollection specification), a Node.js CLI and paid editions that add Git actions in the app. HITOP is a smaller tool: a YAML folder format, a single Rust binary for the terminal and CI, a web app and a desktop app.

**Checked on 2026-11-01** against the pages listed under [Sources](#sources).

## Side by side

| | HITOP | Bruno |
| --- | --- | --- |
| **Price** | Free: CLI, web app, desktop builds on GitHub. Mac App Store app $4.99 and iOS $2.99, paid once | Open Source edition $0; Pro $6 and Ultimate $11 per user per month, billed annually |
| **License** | MIT | MIT |
| **Account and cloud** | No account, no cloud | No cloud sync: the project says Bruno is offline only, with no plans to add it |
| **File format** | YAML: `hitop.yaml`, one file per request under `requests/`, `environments/*.yaml` ([spec](https://github.com/bohdaq/hitop/blob/main/spec/collection-format.md), CC BY 4.0) | Bru (`.bru` files, `bruno.json`) or, from Bruno 3.0, YAML with `opencollection.yml` |
| **Git in the app** | Not in the released apps: use Git as you do for code. The desktop's new UI (in testing) has a free Git panel: status, diff, commit, pull, push, branches and conflict resolution | The pricing page lists Git add and commit, push, branching and checkout under the paid editions |
| **CLI** | `hitop`: one Rust binary (crates.io, static Linux builds, Docker image) | `bru` (`npm install -g @usebruno/cli`), Docker image `usebruno/cli` |
| **CI reports** | JUnit XML, a versioned JSON report and a self-contained HTML page | JSON, JUnit and HTML |
| **CI integrations** | GitHub Action `bohdaq/hitop-action`, GitLab CI template | The `bru` CLI and its Docker image in any pipeline |
| **Postman import** | Collections, environments and globals; `pm.*` scripts run through a compatibility layer, unsupported calls listed by line | Collections and environments; common `pm.*` calls translated to Bruno's API, or kept as is; bulk import from a Postman data dump in Ultimate |
| **Secrets** | OS keychain or environment variables, never in files; free references to Vault or OpenBao, AWS Secrets Manager, Azure Key Vault and 1Password | Secret manager integration in paid editions (pricing page) |
| **Protocols** | HTTP everywhere. GraphQL (with schema completion) in the CLI and web app; WebSocket in the CLI and web app (a browser can't send handshake headers); gRPC, unary and server streaming, in the CLI. The desktop app gets all of them with its new UI (the web app in Tauri, in testing) | HTTP, REST, GraphQL, gRPC and more (pricing page feature table) |
| **Runs in a browser** | Yes, the web app at hitop.rws8.tech/app/ (no install) | Desktop app |

## The difference in practice

Both tools let a pull request change a request file and show the diff. The choice is mostly about the rest of the workflow:

- HITOP's runner is a single static binary, so a pipeline needs no Node.js. Bruno's CLI is an npm package or a Docker image.
- HITOP keeps Postman scripts running as they are where it can (`pm.test`, `pm.expect`, `pm.environment.set`); Bruno translates them to its own API on import.
- Bruno's released desktop app has gRPC, WebSocket and an in-app Git client today; HITOP has WebSocket and gRPC in its CLI (WebSocket also in the web app), and its desktop gets them, with the Git panel, in the new UI.

## Sources

- Bruno pricing: https://www.usebruno.com/pricing (editions, prices, feature table)
- Bruno repository: https://github.com/usebruno/bruno (MIT, Bru format, `bru` CLI, "offline-only")
- Bruno CLI: https://docs.usebruno.com/bru-cli/overview (JSON, JUnit and HTML reports)
- Bruno YAML (OpenCollection): https://docs.usebruno.com/opencollection-yaml/overview (Bruno 3.0.0)
- Bruno Postman migration: https://docs.usebruno.com/get-started/import-export-data/postman-migration
- HITOP: https://github.com/bohdaq/hitop (MIT)
