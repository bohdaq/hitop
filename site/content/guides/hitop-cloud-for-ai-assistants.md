---
title: "HITOP Cloud for AI assistants"
description: "Connect Claude Code, Claude Desktop or Cursor to your HITOP Cloud collections over MCP: list, read, add and change requests without a local folder."
---

## What it does

HITOP Cloud serves your library over the Model Context Protocol at `https://sync.hitop.rws8.tech/mcp`. An AI assistant connected to it can:

- list your collections, their requests and your environments (`list_collections`, `list_requests`, `list_environments`);
- read a request: method, URL, headers, body, auth and scripts (`get_request`);
- add and change requests (`create_request`, `update_request`), which then appear in the HITOP apps and, for a collection saved to GitHub, as a commit.

It doesn't send requests: use the apps or `hitop run` for that. Shared collections are end-to-end encrypted, so the server can't read them and they aren't listed; for those, run `hitop mcp` on a local folder.

## Make an access token

In the HITOP Cloud portal, [Access tokens](https://cloud.hitop.rws8.tech/tokens), create one with the `read` scope, or `read` and `write` so the assistant can change requests. The token is shown once.

## Connect

Claude Code:

```
claude mcp add --transport http hitop https://sync.hitop.rws8.tech/mcp --header "Authorization: Bearer hitop_pat_..."
```

Claude Desktop and Cursor (MCP server settings):

```json
{
  "mcpServers": {
    "hitop": {
      "url": "https://sync.hitop.rws8.tech/mcp",
      "headers": { "Authorization": "Bearer hitop_pat_..." }
    }
  }
}
```

Then ask, for example: "Add a request to Shop API that creates an order, with a test that the status is 201."

## Safety

- A token reaches only your own library, and only with the scopes you gave it. Revoke it in the portal at any time.
- Secret values of environments are never returned: only their names.
- A token can't make, list or revoke tokens, and it can't decrypt shared collections.
