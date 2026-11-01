---
title: "Use secrets from Vault, AWS, Azure and 1Password"
description: "Reference secrets in HashiCorp Vault or OpenBao, AWS Secrets Manager, Azure Key Vault and 1Password from HITOP requests and environments: resolved at run time, masked everywhere, never written to a file."
---

## What a reference is

A reference points at a secret in a secret manager. Write it where a value goes: in a request's URL, headers, body or auth, or as the value of a collection, environment or global variable.

```
${secret:vault:secret/my-api#token}
${secret:aws:prod/db#password}
${secret:azure:my-vault/api-key}
${secret:op:Dev/Stripe/secret key}
```

When a request runs, hitop reads each reference from its manager and puts the value in. The collection files keep the reference, so a folder in git holds no secret, and every teammate and CI job reads the secret with their own access. The value is treated like any secret variable: it shows as `••••` in output, history, run reports and MCP results, and is never written to a file.

References work in the CLI (`hitop send`, `run`, `requests send`, `ws`, `grpc`, `snippet --show-secrets`, `mcp`) and in the desktop app. The web app in a browser can't reach a secret manager: a request that uses a reference isn't sent, and the note names the references.

## Put references in an environment

The usual place is an environment file, so each environment points at its own secrets:

```yaml
# environments/prod.yaml
variables:
  base: https://api.example.com
  token: ${secret:vault:secret/my-api#token}
  dbPassword: ${secret:aws:prod/db#password}
```

Requests then use `${token}` as before. A reference can also sit inside a longer value (`Bearer ${secret:azure:my-vault/api-token}`) or straight in a request.

## HashiCorp Vault and OpenBao

`${secret:vault:<mount>/<path>#<field>}` reads a KV secret, version 2 or version 1. hitop asks Vault which version the mount is, as the `vault` CLI does, so `secret/my-api` and `secret/data/my-api` both work on KV v2.

```
export VAULT_ADDR=https://vault.example.com
vault login -method=oidc          # leaves the token in ~/.vault-token
hitop run ./my-api --env prod
```

hitop takes the token from `VAULT_TOKEN`, else `~/.vault-token`, and sends `VAULT_NAMESPACE` when set (Vault Enterprise, HCP). OpenBao's `BAO_ADDR`, `BAO_TOKEN` and `BAO_NAMESPACE` work too. Without `#field`, a secret with one field gives that field; with several, the error lists the field names.

## AWS Secrets Manager

`${secret:aws:<name or ARN>}` gives the secret's string; `#key` picks one key of a JSON secret (`${secret:aws:prod/db#password}`).

Credentials come from the usual places, first match wins:

1. `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` and `AWS_SESSION_TOKEN` (what `aws-actions/configure-aws-credentials` sets in GitHub Actions),
2. the profile (`AWS_PROFILE`, else `default`) in `~/.aws/credentials` and `~/.aws/config`: static keys or `credential_process`,
3. the AWS CLI: `aws configure export-credentials`, which covers IAM Identity Center (SSO) after `aws sso login` and assumed roles.

The region comes from the ARN, `AWS_REGION`, `AWS_DEFAULT_REGION` or the profile. `AWS_ENDPOINT_URL_SECRETS_MANAGER` or `AWS_ENDPOINT_URL` point hitop at another endpoint, such as LocalStack. hitop signs the request itself (Signature Version 4), so no AWS SDK is bundled.

## Azure Key Vault

`${secret:azure:<vault>/<name>}` reads the current version; add `/<version>` for a fixed one. A vault name with a dot is a full host name, for national clouds (`${secret:azure:my-vault.vault.azure.cn/api-key}`).

hitop gets a token from a service principal when `AZURE_TENANT_ID`, `AZURE_CLIENT_ID` and `AZURE_CLIENT_SECRET` are set (`AZURE_AUTHORITY_HOST` for other clouds), else from the Azure CLI:

```
az login
hitop run ./my-api --env prod
```

## 1Password

`${secret:op:<vault>/<item>/<field>}` (or `${secret:op://<vault>/<item>/<field>}`, with an optional section before the field) runs `op read` with the 1Password CLI you already use. Sign in first (`op signin`, or the desktop app integration), or set `OP_SERVICE_ACCOUNT_TOKEN` for a service account in CI.

## Check that every reference resolves

```
hitop secret check -c ./my-api
```

lists each reference with the requests, environments and globals that use it, and reads each one. It never prints a value, and exits 1 when one fails, with the reason (`permission denied`, `no secret at ...`, `not signed in`). `--env prod` checks one environment; `--list` only lists.

## Run in CI

Give the job access to the manager the way you do for anything else, then run as usual:

```yaml
- uses: aws-actions/configure-aws-credentials@v4
  with:
    role-to-assume: arn:aws:iam::123456789012:role/api-tests
    aws-region: eu-west-1
- run: hitop run ./my-api --env prod --reporter cli,junit
  env:
    VAULT_ADDR: https://vault.example.com
    VAULT_TOKEN: ${{ secrets.VAULT_TOKEN }}
```

Each reference is read once per run. Calls time out after 15 seconds (60 seconds for `op`, `az` and `aws`); set `HITOP_SECRET_MANAGER_TIMEOUT` in seconds to change it. An error names the reference and the reason, never a value.

## Settings in the desktop app

An app opened from the Dock or a desktop launcher doesn't see the variables your shell exports. From hitop 1.1 the desktop app's Settings page has a **Secret managers** section for them:

- Vault or OpenBao: address (`VAULT_ADDR`), namespace (`VAULT_NAMESPACE`), token (`VAULT_TOKEN`).
- AWS Secrets Manager: region (`AWS_REGION`), profile (`AWS_PROFILE`), access key id, secret access key and session token.
- Azure Key Vault: tenant id and client id (`AZURE_TENANT_ID`, `AZURE_CLIENT_ID`), client secret (`AZURE_CLIENT_SECRET`).
- Where the `op`, `az` and `aws` programs are, when they aren't on the app's `PATH`.

Tokens, keys and the client secret go to your system keychain only (service `hitop`, account `secret-managers/<NAME>`), never to a file. The other values go to `secret-managers.json` in hitop's settings folder (`~/.config/hitop/` on Linux, `~/Library/Application Support/hitop/` on macOS). The CLI reads the same file and keychain items, so a value set once works in both.

These values are handed to hitop's secret manager lookups only; they never become environment variables of the app or of anything it starts. Precedence, from first to last:

1. Environment variables, a group at a time. When the environment sets any variable of a group, the saved values of that group are not used: `VAULT_ADDR`/`BAO_ADDR`; `VAULT_TOKEN`/`BAO_TOKEN`; `VAULT_NAMESPACE`/`BAO_NAMESPACE`; `AWS_REGION`/`AWS_DEFAULT_REGION`; `AWS_PROFILE` with the three AWS keys (a shell profile is never mixed with saved keys); the three `AZURE_*` values; `HITOP_OP_BIN`, `HITOP_AZ_BIN`, `HITOP_AWS_BIN` each on its own. The page says which variables the app's environment already sets.
2. The saved settings and keychain values.
3. What hitop finds by itself: `~/.vault-token`, the profiles in `~/.aws`, the `az` and `op` sign-ins.

The CLI has no per-manager flags, so for it the order is environment, then the saved settings; `--no-secret-managers` still turns everything off. `hitop secret check` says when the settings file is in use.

## Turn it off

`--no-secret-managers` on any command, or `HITOP_SECRET_MANAGERS=off` for the CLI and the desktop app, stops hitop from contacting secret managers. A request that uses a reference then fails with the list of references, instead of sending the reference text.

## Good to know

- Code snippets keep references as `{{secret:...}}` placeholders unless you pass `--show-secrets`; the web app's Code dialog always does.
- `hitop mcp` resolves only references that are already in the workspace's files, so an AI client can't ask for other secrets; values are masked in every tool result.
- Scripts see the resolved values through the variables, as they see other secrets.
- The desktop app reads the environment it was started with. On macOS an app opened from the Dock doesn't see your shell's `VAULT_ADDR`: set it in Settings (above); `~/.vault-token`, `~/.aws` and the `az` and `op` CLIs still work. The Mac App Store build can't run the `az`, `op` and `aws` CLIs from its sandbox, and its settings folder and keychain items are its own.
