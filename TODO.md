# TODO: gaps against competitors

Compared with Postman, Insomnia, Bruno and Yaak (site/content/guides/hitop-vs-*.md), 2026-11-06.
Highest value for the effort first.

## Decisions (owner, 2026-11-06)

- Monitors: HITOP Cloud runs library collections on a schedule, paid plans; shared (end to end
  encrypted) collections keep running from the owner's CI with a runner credential.
- Mock servers: on the sync server at `/mock/<random id>/…`, paid plans, rate limited, never HTML.
- AI in the app: the person's own Anthropic key in Settings, kept on the device; secret values masked
  before anything is sent.
- Hosted MCP: the portal's access tokens (`/v1/me/tokens`), every plan.
- Team environments: yes, server-readable like the library; secret values stay on each device.
- Plugins and the API design editor: deferred.
- Comments and per-request history: HITOP Cloud (library) collections only for now.
- gRPC in the web app and WebSocket handshake headers: a bridge on hitop-server, same origin check
  and rate limits as the HTTP proxy.

## Order and status

| # | Item | Status |
|---|------|--------|
| 1 | Comparison pages (HITOP column) | done: HITOP Cloud in price, account, storage, Git rows |
| 2 | Feature matrix (web, desktop, CLI) | done: guide which-hitop-app |
| 3 | Search across collections, environments, history | done: quick open (Mod+P) once something is typed |
| 4 | First run and empty team org | done: sample collection + Import next to the empty response; empty org says so |
| 5 | Create org and invite from the switcher | |
| 6 | OAuth 2 helpers, then SigV4, Digest | |
| 7 | SSE in the web app | |
| 8 | Published collection docs | |
| 9 | OpenAPI lint on import | deferred with API design (owner) |
| 10 | Team environments | |
| 11 | Hosted MCP | |
| 12 | AI in the app | |
| 13 | Mock servers | |
| 14 | Monitors | |
| 15 | Comments and per-request history | |
| 16 | gRPC and WebSocket headers bridge | |
| 17 | Plugins, API design editor | deferred (owner) |

## Quick wins

- [ ] Update the comparison pages (hitop-vs-postman, -insomnia, -bruno, -yaak, postman-alternative):
      they still say "No account, no cloud" and "Git not in released apps"; HITOP Cloud now has the
      library, organizations, GitHub sync and sharing.
- [ ] OAuth 2 auth helpers: authorization code with PKCE, client credentials, token refresh.
      Then AWS SigV4, Digest, NTLM.
- [ ] Server-Sent Events (SSE) in the web app.
- [ ] Create an organization and invite people from the app's org switcher (today only in the portal).

## Features competitors have

- [ ] Mock servers in HITOP Cloud, built from a collection's saved examples (Postman, Insomnia).
- [ ] Monitors: the cloud runs a collection on a schedule and alerts (run history and alerts already
      exist; the scheduler doesn't: today runs come from your own CI or cron).
- [ ] Published collection docs: a readable static page generated from the YAML (Postman).
- [ ] API design: OpenAPI linting on import, then editing a spec (Insomnia).
- [ ] Plugins: auth, template tags, UI (Yaak).
- [ ] More protocols in the apps: gRPC in the web app, WebSocket handshake headers in the browser,
      Socket.IO, MQTT.

## AI

- [ ] Hosted MCP in HITOP Cloud (`sync.hitop.rws8.tech/mcp/<org>`): the `hitop mcp` tools over the
      cloud library, so an assistant works without a local folder. Shared workspaces stay end to end
      encrypted and out of reach.
- [ ] AI in the app: "Write tests for this request", "Explain this failed run", "Requests from this
      curl or spec".

## Rough edges

- [ ] One feature matrix in the docs for web app, desktop app and CLI (gRPC, Git panel, folders differ),
      then close the gaps between them.
- [ ] First run: a sample collection, "Import from Postman" on the empty screen; an empty team org
      says what to do.
- [ ] Team collaboration: comments on requests, change history per request, roles inside an org
      (today only workspaces have roles).
- [ ] Search across collections, environments and history (the sidebar filter only matches requests).
- [ ] Environments shared with a team org.
