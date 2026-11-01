---
title: "Import an OpenAPI spec"
description: "Turn an OpenAPI 3.0 or 3.1 spec into a HITOP collection folder, read the import report, and sync the folder when the spec changes without losing your scripts and edits."
---

## What you need

- The spec as a YAML or JSON file, or its URL. OpenAPI 3.0 and 3.1 are supported. A Swagger 2.0 spec has to be converted first (for example with `swagger2openapi`).
- The `hitop` CLI: `cargo install hitop`, a binary from [GitHub Releases](https://github.com/bohdaq/hitop/releases), or the image `ghcr.io/bohdaq/hitop`. The [web app](/app/) and the desktop app import specs too.

## Import into a folder

```
hitop import openapi https://petstore3.swagger.io/api/v3/openapi.json --to ./petstore
```

A file works the same way (`hitop import openapi ./openapi.yaml --to ./petstore`), and `hitop import ./openapi.yaml --to ./petstore` finds the spec by its content. You get a collection folder with a folder per tag:

```
petstore/
  hitop.yaml               # baseUrl and path parameter variables, the spec's record
  requests/
    pet/find-pet-by-id.yaml
    pet/add-a-new-pet-to-the-store.yaml
    store/place-an-order-for-a-pet.yaml
    user/create-user.yaml
```

Add `--flat` to put every request at the top instead.

## What comes over

| In the spec | In the collection |
| --- | --- |
| The first server | The `baseUrl` collection variable; URLs start with `${baseUrl}` |
| Path parameters | `${name}` in the URL and a collection variable, filled with the spec's example when it has one |
| Query and header parameters | Required ones are on, optional ones are kept but off |
| JSON request body | The spec's example, or one built from the schema (recursive schemas stop where they loop) |
| Form and multipart bodies | Form fields; binary properties become file fields |
| HTTP bearer and basic, API key in a header or query | Collection auth, or the request's own when an operation differs |
| Response examples | Saved examples, to compare responses with |
| `$ref`s inside the spec | Followed, including `components` |

Each request remembers the operation it was made from (its operationId and `METHOD /path`), so a later sync can find it again, even after you rename it.

## Read the import report

The import ends with what was mapped, what wasn't, and what to check:

```
✓ Imported 'Swagger Petstore - OpenAPI 3.0' into petstore (19 requests)
  OpenAPI 3.0.4: 19 operations → 19 requests in 3 folders, 0 saved examples
  variables: baseUrl, orderId, petId, username
  auth: api_key → apikey header (2 uses)
! 2 things not imported:
  POST /pet/{petId}/uploadImage: request body application/octet-stream: attach it yourself
  security scheme petstore_auth: OAuth 2: get a token yourself and set bearer auth (7 uses)
! 1 thing to check:
  security scheme api_key: set api_key in an environment (as a secret)
```

Not imported are OAuth 2 and OpenID Connect schemes, API keys in cookies, cookie parameters, bodies that aren't JSON, form, XML or text, callbacks, webhooks, and `$ref`s that point into other files. The auth variables (`${api_key}` here) are yours to set; keep them as secrets in an environment:

```
mkdir -p petstore/environments
printf 'variables:\n  api_key:\n    secret: true\n' > petstore/environments/staging.yaml
hitop secret set api_key --env staging -c ./petstore    # the value goes to the OS keychain
hitop run ./petstore --env staging
```

In CI, give it as `HITOP_SECRET_STAGING_API_KEY` instead.

## Sync when the spec changes

When the API gets a new version, sync the folder with the new spec (a file or its URL). Look first:

```
hitop openapi sync ./openapi-v2.yaml ./shop --dry-run
```

```
Sync 'Shop' with ./openapi-v2.yaml (dry run: nothing is written)
  + GET /pets/{id} (getPet)  Get a pet → requests/pets/get-a-pet.yaml
  ~ GET /pets (listPets)  All my pets: updated url
  ~ POST /pets (addPet)  Add a pet: kept your body (the spec changed it too)
  - DELETE /pets/{id} (deletePet)  Delete a pet: removed from the spec: kept (--prune deletes it)
  2 added, 2 changed, 1 removed, 0 unchanged
```

Then run it without `--dry-run` to write the changes. What a sync does:

- **New operations** are added as new request files.
- **Changed operations**: each field the spec changed (URL, query parameters, headers, body, auth, the spec's examples) is updated if you left it as imported. If you edited it, your version stays and the sync says so.
- **Header and query parameter rows** (from hitop 1.1) are merged one by one: a header you added or edited stays, while the spec's new, changed or removed headers still apply to the rows you didn't touch. The diff lists each row.
- **Yours, always**: request names, file names and folders you moved them to, scripts and assertions, examples you saved, auth you set on a request, and environments.
- **Removed operations** are listed and kept. Add `--prune` to delete their files.

Commit the folder before a sync and the change is one reviewable diff in git.

## In the web app and the desktop app

Open **Import** in the sidebar: paste a spec (YAML or JSON) into the box, or choose **Load an OpenAPI spec from a URL**. The spec is added as a new collection, and the same report appears. A spec on a public host is fetched through HITOP's proxy on the web; the desktop app fetches it directly.

A collection imported from a spec has **Sync with OpenAPI** on its page. The spec's URL is filled in; **Preview** shows the same diff as `--dry-run`, and **Apply** saves it. Check "Delete requests whose operation is gone from the spec" to prune. A collection opened from a folder saves the result to its files.
