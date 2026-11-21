---
title: "Variables and environments"
description: "Use ${name} variables in HITOP requests: collection variables, environments such as local and staging, globals, secrets and the order they win in, in the apps and with hitop run."
---

## Variables in a request

Write `${name}` anywhere in a request's URL, query parameters, headers, body or auth fields, and HITOP fills in the value when it sends:

```
${baseUrl}/users/${userId}
Authorization: Bearer ${token}
```

In the apps, hover over a `${name}` to see its value and where it came from; a name with no value is marked.

## Where values come from

| Scope | Holds | Where you set it |
| --- | --- | --- |
| Globals | Values for every collection | Environments page, Globals; `globals.yaml` in a folder |
| Collection variables | Values for one collection, such as `baseUrl` | The collection's Variables page; `variables` in `hitop.yaml` |
| Environment | One set per place you test: `local`, `staging`, `production` | Environments page, and the picker next to Send; `environments/<name>.yaml` |
| Set by a script | Values a script sets with `pm.environment.set` | [Scripts](/guides/scripts-before-and-after-requests/) |
| Secrets | Tokens and passwords, marked secret | See below |

When two scopes have the same name, the later one in this order wins: globals, collection, environment, data row and `--var` (CLI runs only), set by a script, secrets. So a collection can keep `baseUrl` for local development and the `staging` environment can replace it.

## Environments in a collection folder

A collection folder keeps one YAML file per environment:

```yaml
# environments/staging.yaml
variables:
  baseUrl: https://staging.example.com
  token:
    secret: true
```

Pick one in the app, or pass it to the CLI:

```
hitop run ./my-api --env staging
hitop run ./my-api --env staging --var userId=42
```

`--env-file <yaml>` takes an environment from a file outside the folder, and `--data rows.csv` runs the collection once per row with the row's columns as variables.

## Secrets

Mark a variable secret and its value never goes into the folder, an export or a run report; it is shown as `••••`. The file keeps only the name (`secret: true`).

- In the CLI the value comes from the variable `HITOP_SECRET_<ENV>_<NAME>`, `--secret NAME=value`, or the OS keychain (`hitop secret set --env staging token`); on a terminal hitop asks for it.
- The desktop app keeps the value in the OS keychain, and the web app in this browser only.
- A value can also be a reference to Vault, AWS Secrets Manager, Azure Key Vault or 1Password, such as `${secret:vault:kv/shop#token}`: see [Use secrets from Vault, AWS, Azure and 1Password](/guides/use-secrets-from-vault-aws-azure-and-1password/).

## Variables in scripts

`getVariable('baseUrl')` reads the value `${baseUrl}` would get. Postman scripts use `pm.environment.get`, `pm.variables.get` and `pm.globals.get`; `pm.environment.set` sets a value for the rest of the run.
