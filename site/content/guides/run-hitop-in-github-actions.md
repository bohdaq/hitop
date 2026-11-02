---
title: "Run HITOP in GitHub Actions"
description: "Test your API on every push with the hitop GitHub Action: run a collection folder from your repository, get a JUnit artifact, a job summary and a failing check when a request fails."
---

## The short version

Keep your collection folder in the repository and add one step to a workflow:

```yaml
name: API tests
on: [push, pull_request]

jobs:
  api:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - uses: bohdaq/hitop-action@v1
        with:
          version: 1.1.0
          path: ./my-api
          env: staging
        env:
          HITOP_SECRET_STAGING_TOKEN: ${{ secrets.API_TOKEN }}
```

The action runs `hitop run` in the `ghcr.io/bohdaq/hitop` image, so nothing is installed on the runner. When it finishes you get three things:

-   **A check that fails** when any request fails, so a pull request shows the problem before it is merged.
-   **A job summary** with totals, every failed request with its failed checks, and a table of all requests.
-   **An artifact** named `hitop-junit` holding `junit.xml`, ready for test report tools.

## Pin the hitop version

`version` is required and names the image tag. A new hitop release never changes your pipeline until you change this line, and the action itself stays on `@v1` for compatible updates. The available tags are listed on the [package page](https://github.com/bohdaq/hitop/pkgs/container/hitop).

## Inputs

| Input | Default | What it does |
| --- | --- | --- |
| `version` | required | hitop version to run. |
| `path` | `.` | The collection folder, relative to the repository root. |
| `env` | | An environment from the folder's `environments/`. |
| `env-file` | | An environment file from anywhere in the repository. |
| `reporter` | `cli,junit,json` | Which reports to write. The summary reads `json`, the artifact is `junit`. |
| `out` | `hitop-reports` | Where the reports go. |
| `secrets` | | Secret variables, one `NAME=value` per line. |
| `vars` | | Plain variables, one `key=value` per line. |
| `args` | | Any other `hitop run` flags, such as `--bail --timeout 5000`. |
| `artifact-name` | `hitop-junit` | Name of the uploaded JUnit artifact; empty skips it. |
| `summary` | `true` | Write the job summary. |

The outputs are `exit-code`, `passed`, `report-dir`, `junit-file` and `report-file`.

## Secrets

An environment file lists a secret by name only (`token: {secret: true}`), so the value never lands in git. Give the value from a repository secret in one of two ways.

As an environment variable named `HITOP_SECRET_<ENV>_<NAME>` in upper case, with every other character turned into `_`. For `token` in the environment `staging` that is `HITOP_SECRET_STAGING_TOKEN`:

```yaml
      - uses: bohdaq/hitop-action@v1
        with:
          version: 1.1.0
          path: ./my-api
          env: staging
        env:
          HITOP_SECRET_STAGING_TOKEN: ${{ secrets.API_TOKEN }}
```

Or with the `secrets` input, one `NAME=value` per line:

```yaml
        with:
          secrets: |
            token=${{ secrets.API_TOKEN }}
```

Either way hitop shows the value as `••••` in the log, the summary and both reports. Values in `vars` are printed like any other value, so keep secrets out of it.

## Exit codes and the step result

The step fails unless hitop exits with 0:

| Code | Meaning |
| --- | --- |
| 0 | Every request passed. |
| 1 | A request or a check failed. |
| 2 | Usage or configuration error, such as an unknown environment or a missing secret. |

To keep going after a failure, for example to publish a test report first, set `continue-on-error: true` on the step and read `steps.<id>.outputs.exit-code` later.

## Show each request as a test

GitHub does not show JUnit files by itself. A test report action turns `junit.xml` into a check run with one test per request, the failure message and the checks that failed. Both `dorny/test-reporter` and `mikepenz/action-junit-report` read hitop's report, and the action's own self test runs both:

```yaml
permissions:
  contents: read
  checks: write

jobs:
  api:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - uses: bohdaq/hitop-action@v1
        id: hitop
        continue-on-error: true
        with:
          version: 1.1.0
          path: ./my-api
      - uses: dorny/test-reporter@v3
        if: always()
        with:
          name: API tests
          path: hitop-reports/junit.xml
          reporter: java-junit
      - if: steps.hitop.outputs.exit-code != '0'
        run: exit 1
```

`dorny/test-reporter` writes its report to the job summary by default; with `use-actions-summary: false` it creates a check run, which needs the `checks: write` permission shown above. `mikepenz/action-junit-report` creates a check run with one annotation per failed request.

Each test is named after the request and grouped by collection; with `--data` or `--iterations` every iteration is its own suite. The failure shows its kind and the request's failure line, such as `AssertionError: status is 201: got 200`, `HttpStatus: HTTP 500` or `RequestError` with the connection error. With `--bail` the run stops at the first failure, and requests that never ran are not in the report: the summary says the run stopped early.

## Keep an HTML report

From hitop 1.0.0, `--reporter html` writes `report.html`: one page with the totals, every request with its status, timings, failure and checks, and a switch to show only what failed. It loads nothing from the network, so it opens straight from a downloaded artifact. The action uploads only the JUnit file, so upload the page yourself; `if: always()` keeps it when the run fails:

```yaml
      - uses: bohdaq/hitop-action@v1
        id: hitop
        with:
          version: 0.4.0
          path: ./my-api
          env: staging
          reporter: cli,junit,json,html
      - uses: actions/upload-artifact@v7
        if: always() && steps.hitop.outputs.report-dir != ''
        with:
          name: hitop-report
          path: ${{ steps.hitop.outputs.report-dir }}/report.html
```

Secret values are `••••` in the page, as in every other report, and response bodies are not in it.

## Run on a schedule

`hitop schedule` writes the workflow for you, at the root of the repository the folder is in:

```
hitop schedule ./my-api --github --env prod
hitop schedule ./my-api --github --env prod --cron "0 */6 * * *" --force
```

The workflow `.github/workflows/hitop-my-api-prod.yml` runs on `schedule:` (every day at 06:00 UTC unless you pass `--cron`) and on `workflow_dispatch:`, so the Actions tab has a "Run workflow" button. It pins the hitop version that wrote it, passes each secret variable of the environment (and of the folder's `globals.yaml`) as `HITOP_SECRET_<ENV>_<NAME>: ${{ secrets.HITOP_SECRET_<ENV>_<NAME> }}`, and prints the `gh secret set` commands that add them. Pick the secrets with `--secrets token,api_key`. No value is ever written to the file. From hitop 1.0.0 it also uploads the HTML report. An existing workflow is kept unless you pass `--force`, and every generated workflow passes [actionlint](https://github.com/rhysd/actionlint) in hitop's own CI.

GitHub runs schedules in UTC and at most every 5 minutes, and turns them off in public repositories with no activity for 60 days. hitop checks the cron expression before it writes anything.

## Test a server the job starts

The container shares the runner's network, so a collection can call a server started earlier in the job on `localhost`:

```yaml
      - run: npm ci && npm start &
      - run: npx wait-on http://localhost:3000/health
      - uses: bohdaq/hitop-action@v1
        with:
          version: 1.1.0
          path: ./api-tests
          vars: |
            base=http://localhost:3000
```

Service containers with a mapped port work the same way.

## Runners

The action needs Docker, which every GitHub hosted Ubuntu runner has. On macOS or Windows runners, download the hitop binary from [GitHub Releases](https://github.com/bohdaq/hitop/releases) and run `hitop run` yourself; the flags are the same, see [Run collections in CI](/terms/run-collections-in-ci.html).
