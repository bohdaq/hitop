# TODO: gaps against competitors

Compared with Postman, Insomnia, Bruno and Yaak (site/content/guides/hitop-vs-*.md), 2026-11-06.
Highest value for the effort first.

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
