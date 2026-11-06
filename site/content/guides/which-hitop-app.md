---
title: "Which HITOP app"
description: "What the HITOP web app, the desktop app and the CLI each do: protocols, collection folders, Git, secrets, HITOP Cloud and CI, in one table."
---

## In short

- **Web app** ([hitop.rws8.tech/app](/app/)): nothing to install. Public APIs go through the HITOP request proxy, which keeps nothing.
- **Desktop app** (the new UI, in testing; the Mac App Store app is the earlier one): requests go straight from your computer, so internal and localhost APIs work, and so do gRPC and a Git panel.
- **CLI** (`hitop`): runs collections in CI with reports, and serves them to AI assistants over MCP.

All three read and write the same [collection folder](/guides/collections-in-git/).

## Side by side

| | Web app | Desktop app | CLI |
| --- | --- | --- | --- |
| **HTTP** | Yes (public hosts through the proxy) | Yes, straight from your computer | Yes |
| **GraphQL** (schema completion) | Yes | Yes | Yes |
| **WebSocket** | Yes, without handshake headers (a browser can't send them) | Yes | Yes (`hitop ws`) |
| **gRPC** | No | Yes | Yes (`hitop grpc`) |
| **Server-Sent Events** (live) | Yes (`Accept: text/event-stream`) | No (the whole body at the end) | No |
| **Collection folders** (YAML) | Open a folder in Chromium browsers; `.zip` everywhere | Yes | Yes |
| **Git** | Saves a collection to GitHub on every change (HITOP Cloud) | Git panel: status, diff, commit, pull, push, branches | Use git |
| **Secrets** | Kept in this browser; HITOP Cloud seals them on the server | OS keychain; Vault/OpenBao, AWS, Azure Key Vault and 1Password references | Same as the desktop |
| **HITOP Cloud** | Cloud library, organizations, shared collections, GitHub | Cloud library, organizations, shared collections | `hitop sync` for shared collections |
| **Postman import** | Yes | Yes | Yes (`hitop import`) |
| **OpenAPI import and sync** | Yes | Yes | Yes (`hitop openapi`) |
| **Mock server** (HITOP Cloud, paid plans) | Yes (Share, Mock) | Yes (Share, Mock) | No |
| **Docs page** (one HTML file) | Yes (Share, File) | Yes (Share, File) | Yes (`hitop docs`) |
| **CI reports** (JUnit, JSON, HTML) | No | No | Yes (`hitop run`) |
| **Ask AI** (your own Anthropic key) | Yes: tests, explanations, a request from text | Yes | No |
| **MCP for AI assistants** | HITOP Cloud: `sync.hitop.rws8.tech/mcp` ([guide](/guides/hitop-cloud-for-ai-assistants/)) | Same | Yes (`hitop mcp` on a folder) |
| **Works offline** | No | Yes | Yes |

## Which one to pick

- Trying HITOP, or a public API: the web app.
- Internal APIs, gRPC, or secrets from a vault: the desktop app.
- CI, scheduled runs, or an AI assistant working on your collections: the CLI.
