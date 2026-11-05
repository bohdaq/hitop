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
    version: 1.2.0
    path: api
    env: staging
  env:
    HITOP_SECRET_STAGING_TOKEN: ${{ secrets.STAGING_TOKEN }}
```

More in [Run collections in CI](/terms/run-collections-in-ci.html), [Run HITOP in GitHub Actions](/terms/run-hitop-in-github-actions.html) and [Run HITOP in GitLab CI](/terms/run-hitop-in-gitlab-ci.html).

## From the web app, without git on your machine

Signed in to HITOP Cloud in the [web app](/app/), open **Sync**, then **GitHub** → **Connect GitHub**. The HITOP Cloud GitHub App asks only for the repositories you choose. Then **Link to GitHub** on a collection, pick the repository and a folder (`shop`, or empty for the root):

- An empty folder gets the collection as the files above (`hitop.yaml` and `requests/`). A folder that already holds a hitop collection is opened as it is.
- **Every save is pushed** as a commit (saves within a few seconds go into one), with your name as the author. Other files in the repository are never touched, and secret values and environments are never committed.
- Commits your team pushes on GitHub come back into the app within seconds.
- When the same request changed both in the app and on GitHub, the app shows both versions side by side: **Keep mine**, **Take GitHub's** or **Keep both** (yours is added as "… (mine)"). Nothing is pushed until you choose. Changes to different requests, or to different keys of `hitop.yaml`, merge by themselves.
- **Unlink** stops pushing; the files stay in the repository.

The collection is kept in your HITOP Cloud library, which the server can read. For collections that must stay end-to-end encrypted, use a [shared workspace](/terms/share-a-workspace-with-your-team.html) instead, or the desktop app's Git panel on a folder.

## Merge conflicts

Two people editing different requests touch different files, so most merges are clean. `hitop.yaml` lists the request order only when it differs from the alphabetical one; if both sides reorder requests, resolve that list like any other YAML.
