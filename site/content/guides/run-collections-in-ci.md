---
title: "Run collections in CI"
description: "Run a HITOP collection in any CI pipeline with hitop run: JUnit and JSON reports, exit codes, environments, variables, timeouts and data files."
---

## What hitop run does

`hitop run` sends every request in a collection in order, runs the scripts before and after each one, and reports what passed. It works on a collection folder in your repository, so the pipeline tests exactly what is in the commit:

```
hitop run ./my-api --env staging
```

Each request shows its status, its response time and every check from its script after, with a mark for passed or failed and the reason for each failure. The run ends with a summary line such as `Summary: 11/12 requests succeeded, 30/31 assertions passed`.

A request passes when it was sent, the status is below 400, and every check in its script after passed.

## Install it in the pipeline

Use the static binary from GitHub Releases, `cargo install hitop`, or the Docker image:

```
docker run --rm -v "$PWD:/work" -w /work ghcr.io/bohdaq/hitop:1.0.0 run ./my-api --env ci
```

On GitHub, the [hitop action](/terms/run-hitop-in-github-actions.html) does this in one step and adds a job summary. GitLab needs the image's `-debug` tag, which has a shell: see [Run hitop in GitLab CI](/terms/run-hitop-in-gitlab-ci.html).

## Exit codes

Pipelines decide from the exit code, so hitop keeps them simple:

| Code | Meaning |
| --- | --- |
| 0 | Every request passed. |
| 1 | A request or a check failed: a wrong status, a failed assertion, a timeout or a server that did not answer. |
| 2 | Usage or configuration error: an unknown collection or environment, a bad flag, an unreadable environment or data file, a missing secret, or reports that could not be written. |

`hitop run --help` lists them too. Code 2 means the run never really started, so fix the command rather than the API.

## Reports

Pick reporters with `--reporter`, separated by commas, and a folder with `--out` (default `hitop-reports`):

```
hitop run ./my-api --env ci --reporter cli,junit,json --out reports
```

-   **cli:** progress and results in the log (the default).
-   **junit:** `reports/junit.xml`, one test suite per collection and one test case per request. Most CI systems show it as a test report: GitLab `artifacts:reports:junit`, Jenkins, Azure Pipelines, and test report actions on GitHub.
-   **json:** `reports/report.json`, with totals, every request and every check. Its format is versioned (`"reportVersion": 1`) and documented in the [run report spec](https://github.com/bohdaq/hitop/blob/main/spec/run-report.md), so your own scripts can read it safely.
-   **html:** `reports/report.html`, one page for people: totals, every request with its timings, failure and checks. It is self-contained (no script, nothing loaded from the network) and follows the system's light or dark theme, so it works as a downloaded CI artifact. From hitop 1.0.0.

Secret values never reach a report. Wherever one would appear, in a URL, an error or a check, it is shown as `••••`.

## Run on a schedule

`hitop schedule ./my-api --github --env prod` writes a GitHub Actions workflow that runs the folder every day and on demand (see [Run hitop in GitHub Actions](/terms/run-hitop-in-github-actions.html)). On your own machine or a server, `hitop schedule ./my-api --cron "*/30 * * * *" --env local` prints a crontab line instead: it keeps every run's reports in a folder of their own and appends the output to a log; add it with `crontab -e`.

## Environments and variables

Choose the variables for the run:

-   `--env <name>`: one of the folder's `environments/` files.
-   `--env-file <file.yaml>`: an environment file from anywhere, in the same format, for example one your pipeline writes.
-   `--var key=value`: one variable for this run; repeat the flag for more.

Later sources win: collection variables, then the environment, then the data row, then `--var`, then secrets. `--var` values are printed like any other value, so give secrets through `HITOP_SECRET_<ENV>_<NAME>` (for example `HITOP_SECRET_CI_TOKEN` for `token` in environment `ci`) or `--secret NAME=value`:

```
HITOP_SECRET_CI_TOKEN=$API_TOKEN hitop run ./my-api --env ci --var base=https://preview.example.com
```

## Stop early or limit time

-   `--bail`: stop at the first request that fails. Without it every request runs, so one report shows every problem.
-   `--timeout <ms>`: the longest a request may take. A slower one fails with a timeout and the run goes on.

## Data driven runs

`--data` runs the collection once per row of a CSV or JSON file, with the row's values as variables:

```
user,expected
ada,200
mallory,403
```

```
hitop run ./my-api --env ci --data users.csv
```

A request can use `${user}` and a check can use `getVariable('expected')`. A JSON file is an array of objects: `[{"user": "ada"}, {"user": "mallory"}]`. Each row is an iteration: the log shows `Iteration 2 of 2`, the JUnit report has one test suite per iteration with the row as properties, and the JSON report keeps the row with its results.

To repeat a run without data, use `--iterations <n>`.

## Why run HITOP in CI?

-   **Collections in git:** the requests reviewed in a merge request are the ones the pipeline runs.
-   **Clear exit codes:** 0, 1 or 2, nothing to parse.
-   **Standard reports:** JUnit for your CI's test view, versioned JSON for your own tools.
-   **Secrets stay secret:** from environment variables, masked in every log and report.
-   **One small binary:** no runtime to install.
