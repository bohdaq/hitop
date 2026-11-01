# Product Hunt (draft, not submitted)

**Name:** HITOP

**Tagline** (60 characters max): API client for your terminal, your CI and your AI agent

**Topics:** Developer Tools, API, Open Source, GitHub

**Links:** https://hitop.rws8.tech · https://github.com/bohdaq/hitop

**Description** (260 characters max):

Send HTTP requests, keep them as YAML files in your repo, and run them in CI with JUnit reports.
Imports Postman collections and scripts, keeps secrets in your OS keychain, and works with Claude
Code and Cursor over MCP. No account, MIT licensed.

**Gallery:**

1. The demo GIF (`site/demo/demo.gif`): Postman import, a run, a failing run.
2. A request file next to its pull request diff.
3. The GitHub job summary and JUnit annotations from `bohdaq/hitop-action`.
4. The web app with a response.
5. The desktop app.

**First comment (maker):**

Hi Product Hunt! I built HITOP because I wanted my API collections reviewed in pull requests like
the rest of the code, and run in CI by the same tool I use locally.

What it does today:
- Collections are folders of YAML files, one per request.
- `hitop import` brings over Postman collections, environments and globals; test scripts keep
  running, and anything that can't is listed by line.
- `hitop run` in CI with JUnit and JSON reports, a GitHub Action and a Docker image.
- Secrets live in your OS keychain or CI secrets, never in the files.
- `hitop mcp` lets AI coding tools run and edit requests without seeing secrets.
- A web app with no install, and desktop apps.

The CLI speaks HTTP, GraphQL, WebSocket and gRPC, imports OpenAPI specs, and reads secrets from Vault, AWS,
Azure and 1Password. Free and open source (MIT).
I'd love to hear what's missing for your team to switch.
