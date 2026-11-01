---
title: "Use HITOP with Claude Code and Cursor"
description: "Connect hitop to Claude Code, Cursor or any MCP client with hitop mcp: list, read, run, create and update the requests of a collection folder, with secrets masked."
---

## What hitop mcp does

`hitop mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server. An AI coding tool starts it, talks to it over stdin and stdout, and can then work with your API collection the way you do in the terminal:

```
hitop mcp ./my-api
```

The argument is a collection folder (the `hitop.yaml`, `requests/` and `environments/` layout from `hitop init`). Leave it out to serve the collections in hitop's global storage instead, the ones `hitop collections list` shows.

Check that your hitop has it with `hitop mcp --help`; update with `cargo install hitop` or a binary from [GitHub Releases](https://github.com/bohdaq/hitop/releases).

Requests run through the same code as `hitop run`: the same variables, scripts, assertions and cookie jar. The server runs on your machine and sends nothing but your requests (plus the weekly usage ping, if you turned it on).

## The tools

| Tool | What it does |
| --- | --- |
| `list_collections` | The folder's collection, or every collection in global storage. |
| `list_requests` | A collection's requests in run order: name, method, URL, file. |
| `get_request` | One request as saved: method, URL, headers, body, auth and scripts. |
| `run_request` | Sends one request and returns status, headers, body (cut at 64 KiB), timing and assertion results. |
| `run_collection` | Runs every request in order and returns the [run report](https://github.com/bohdaq/hitop/blob/main/spec/run-report.md) (`reportVersion` 1), like `hitop run --reporter json`. |
| `create_request` | Adds a request; in a folder it becomes a new YAML file under `requests/`. |
| `update_request` | Changes the fields you give of a saved request. |
| `list_environments` | Environments and globals with their variables; secret variables by name only. |
| `set_environment` | Picks the environment that later runs use, for this session only. |

There is no delete tool: removing requests stays with you (`hitop requests delete`, or `git rm` in a folder). In a folder every change the assistant makes is an ordinary file change, so `git diff` shows it and `git checkout` undoes it.

## Claude Code

Add a `.mcp.json` file to the root of your project:

```json
{
  "mcpServers": {
    "hitop": {
      "command": "hitop",
      "args": ["mcp", "./my-api"]
    }
  }
}
```

Or add it from the terminal with `claude mcp add hitop -- hitop mcp ./my-api`. Claude Code asks once before it starts a server from a project's `.mcp.json`. Then ask for things like "run the orders requests against staging and tell me which assertions fail" or "add a request for `DELETE /orders/{id}` with a 204 check".

## Cursor

Cursor reads the same `mcpServers` shape from `.cursor/mcp.json` in the project (or `~/.cursor/mcp.json` for every project):

```json
{
  "mcpServers": {
    "hitop": {
      "command": "hitop",
      "args": ["mcp", "./my-api"]
    }
  }
}
```

A relative folder is resolved from the directory the client starts the server in, normally the project root. If the server can't find the folder, give an absolute path. hitop must be on the `PATH` the tool sees; otherwise give its full path as `command`.

## Environments

With a folder, runs start without an environment; with global storage they start with your active environment, as `hitop run` does. The assistant picks one with `set_environment` (or gives `environment` to a single run). That choice lasts for the session and is not saved: your active environment in hitop stays as it is.

Variables resolve as in `hitop run`, later ones winning: globals, collection, environment, the call's `variables`, variables set by scripts, then secrets.

## Secrets

The assistant never sees a secret's value:

-   Secret values come from the OS keychain (`hitop secret set token --env staging`) or from `HITOP_SECRET_<ENV>_<NAME>` environment variables, exactly as for `hitop run`. There is no prompt: stdin belongs to the protocol.
-   Every secret value is shown as `••••` in what the tools return: URLs, response headers and bodies, assertion messages and errors.
-   `list_environments` lists secret variables by name, without values.
-   `create_request` and `update_request` refuse a request that contains a secret's value and ask for `${name}` instead, so values never land in your collection files.

Don't put secret values in `.mcp.json` or `.cursor/mcp.json`: those files are usually committed. Keep them in the keychain, or export `HITOP_SECRET_*` variables in the shell the editor starts from.

A secret only stays hidden from the assistant if hitop knows it is a secret. A token typed straight into a request, or a value a response hands back that is not a secret variable, is just data and is shown.

## Other clients

Any MCP client that starts stdio servers works the same way: the command is `hitop` and the arguments are `mcp` and the folder. To try the tools by hand, the MCP inspector has a command line mode:

```
npx @modelcontextprotocol/inspector --cli hitop mcp ./my-api --method tools/list
```
