---
title: "Migrate from Postman"
description: "Move a Postman collection, its environments and globals to a HITOP collection folder, keep secrets in the OS keychain, check the import report and run it in CI."
---

## What you need

- Your collection exported from Postman as **Collection v2.1** (a `.json` file).
- Each environment you use, exported from Postman (one `.json` file each), and your globals if the collection uses them.
- The `hitop` CLI: `cargo install hitop`, a binary from [GitHub Releases](https://github.com/bohdaq/hitop/releases), or the image `ghcr.io/bohdaq/hitop:1.0.0`. The [web app](/app/) imports the same files if you'd rather not install anything.

## Import into a folder

Give `hitop import` every file at once. It tells collections, environments and globals apart by their content:

```
hitop import shop.postman_collection.json \
  staging.postman_environment.json local.postman_environment.json \
  workspace.postman_globals.json --to ./shop-api
```

You get a collection folder:

```
shop-api/
  hitop.yaml               # name, collection variables, default auth
  requests/
    auth/log-in.yaml       # one file per request; Postman folders become directories
    orders/list-orders.yaml
  environments/
    staging.yaml
    local.yaml
  globals.yaml
```

Postman's `{{variable}}` becomes `${variable}`. Folders, collection and request auth (Bearer, Basic, API key), headers, query parameters, bodies (raw, form, multipart) and both kinds of script come across.

## Secrets

Postman variables of type **secret** are marked `secret: true` in the environment file, and their values never go into the folder. On a machine with a keychain, `hitop import` stores them there (macOS Keychain, Secret Service on Linux, Windows Credential Manager). Elsewhere it tells you the name to use instead.

Give hitop a secret in any of these ways, first match wins:

1. An environment variable `HITOP_SECRET_<ENV>_<NAME>`, for example `HITOP_SECRET_STAGING_PASSWORD`. This is the one for CI.
2. `--secret password=...` on the command line.
3. The OS keychain: `hitop secret set password --env Staging -c ./shop-api`.
4. A prompt, when hitop runs in a terminal.

Secret values are shown as `••••` in output, history and reports.

## Read the import report

The import ends with a list of things to check, each with its place in the collection and, for scripts, the line:

```
'Shop API': 2 things to check (1 script line with unsupported calls)
  Orders / Refund (post-request script) line 4: pm.sendRequest in `pm.sendRequest(url, cb);`
  Admin (auth): oauth2 auth
```

Scripts run with a `pm.*` layer: `pm.test`, `pm.expect` (chai style), `pm.response` (`code`, `json()`, `text()`, `headers`, `responseTime`, `to.have.status`), `pm.request`, `pm.environment`, `pm.collectionVariables`, `pm.globals`, `pm.variables`, and the older `tests[...]` and `postman.setEnvironmentVariable` forms. Not supported yet: `pm.sendRequest`, `postman.setNextRequest`, `require()` and bundled libraries such as CryptoJS, lodash or cheerio, and OAuth 2.0 auth. Rewrite those lines, or move the step into a request of its own.

## Run it

Run the folder with an environment. The order of requests is the order in Postman:

```
hitop run ./shop-api --env Staging
```

A login request that saves its token works as it did in Postman. Its test script:

```
pm.environment.set("token", pm.response.json().token);
```

and the collection's Bearer auth `${token}` sends it with every later request. Switch to another environment with `--env Local`; values from `--var name=value` override both.

Variables resolve in this order, later ones win: globals, collection, environment, data row (`--data`), `--var`, values set by scripts during the run, secrets.

## In CI

Commit the folder, then run it in the pipeline with JUnit and JSON reports:

```
hitop run ./shop-api --env Staging --reporter cli,junit,json --out reports
```

Exit code 0 means every request passed, 1 a request or check failed, 2 a usage or configuration error. On GitHub use the [hitop action](/terms/run-hitop-in-github-actions.html); on GitLab, the [template](/terms/run-hitop-in-gitlab-ci.html). Each JUnit testcase names its request file (`file="shop-api/requests/auth/log-in.yaml"`), so test report tools can point at it.

## Keep Postman during the switch

`hitop export` writes Postman v2.1 again, so both tools can run the same collection for a while. Once the folder is in your repository, make changes there and review them as diffs: see [Collections in git](/terms/collections-in-git.html).
