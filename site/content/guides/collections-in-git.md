---
title: "Collections in git"
description: "Keep API collections in your repository as YAML files, one per request: review them in pull requests, keep secrets out, and run the folder in CI on every commit."
---

## Why a folder of files

A collection that lives in a cloud workspace changes without a review, and nobody can tell which version of the API it was written for. A collection folder in the repository changes in the same pull request as the code it tests, has a history, and runs in CI from the commit you are looking at.

HITOP's collection folder is plain YAML with one file per request, written with a fixed key order so diffs stay small. The format is a [published spec](https://github.com/bohdaq/hitop/blob/main/spec/collection-format.md) (CC BY 4.0).

## Start a folder

```
hitop init ./api
```

or bring one in: `hitop import collection.json env.json --to ./api` (from Postman, see [Migrate from Postman](/terms/migrate-from-postman.html)), or `hitop convert export.json ./api` from a HITOP JSON export.

```
api/
  hitop.yaml          # format: 1, name, variables, default auth, request order
  requests/
    get-user.yaml
    orders/
      create-order.yaml
  environments/
    dev.yaml
    staging.yaml
  globals.yaml        # optional
```

## A request file

```yaml
name: Create order
method: POST
url: ${baseUrl}/orders
headers:
  - name: Content-Type
    value: application/json
body:
  type: json
  content: |
    {
      "sku": "${sku}",
      "quantity": 2
    }
scripts:
  post: |
    assert.status(201);
    setContext('orderId', getResponseValue('id'));
```

Multi-line bodies and scripts are YAML block scalars, so a change to one line of a script is one line in the diff. Keys hitop doesn't know are kept when it saves the file.

## Edit it any way you like

- By hand in your editor.
- With the CLI: `hitop requests add ./api --name "Get user" --url '${baseUrl}/users/1'`, `hitop variables set ./api baseUrl https://api.example.com`.
- In the web app or the desktop app: **Open folder** (where the browser supports the File System Access API), or import and export the folder as a `.zip`.

## Secrets stay out

In an environment file (`environments/staging.yaml`) a secret variable has no value:

```yaml
variables:
  baseUrl: https://staging.example.com
  token:
    secret: true
```

The value comes from `HITOP_SECRET_STAGING_TOKEN`, `--secret token=...`, the OS keychain (`hitop secret set token --env staging -c ./api`) or a prompt. Nothing hitop writes (files, history, reports) holds the value; output masks it as `••••`.

## Review in pull requests

A pull request that adds an endpoint can add its request file and its checks next to the handler. Reviewers see the URL, headers, body and assertions as text. When the CI run fails, each JUnit testcase carries the request file (`file="api/requests/orders/create-order.yaml"`), so annotation tools can show the failure on that file.

## Run it on every commit

```
hitop run ./api --env staging --reporter cli,junit,json --out reports
```

On GitHub:

```yaml
- uses: bohdaq/hitop-action@v1
  with:
    version: 1.0.0
    path: api
    env: staging
  env:
    HITOP_SECRET_STAGING_TOKEN: ${{ secrets.STAGING_TOKEN }}
```

More in [Run collections in CI](/terms/run-collections-in-ci.html), [Run HITOP in GitHub Actions](/terms/run-hitop-in-github-actions.html) and [Run HITOP in GitLab CI](/terms/run-hitop-in-gitlab-ci.html).

## Merge conflicts

Two people editing different requests touch different files, so most merges are clean. `hitop.yaml` lists the request order only when it differs from the alphabetical one; if both sides reorder requests, resolve that list like any other YAML.
