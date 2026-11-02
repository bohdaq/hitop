---
title: "Run HITOP in GitLab CI"
description: "A .gitlab-ci.yml template for hitop: run a collection folder on every pipeline, show each request in the merge request test report and keep secrets in CI/CD variables."
---

## The template

Copy this job into `.gitlab-ci.yml` and change the folder and environment:

```yaml
api-tests:
  stage: test
  image:
    name: ghcr.io/bohdaq/hitop:1.1.0-debug
    entrypoint: [""]
  variables:
    HITOP_KEYCHAIN: "off"
  script:
    - hitop --version
    - hitop run ./my-api --env staging --reporter cli,junit,json --out hitop-reports
  artifacts:
    when: always
    paths:
      - hitop-reports/
    reports:
      junit: hitop-reports/junit.xml
```

The job fails when a request fails, the merge request shows every request as a test in its test report, and the reports stay as job artifacts.

From hitop 1.0.0, add `html` to `--reporter` (`cli,junit,json,html`) and `hitop-reports/report.html` is kept with the other artifacts: one page per run that opens offline from the job's artifact browser. To run the job on a timetable, add a pipeline schedule under **Build → Pipeline schedules**; the job above runs in scheduled pipelines unchanged.

## Why the debug image

GitLab runs a job's `script:` through a shell inside the job image. The plain `ghcr.io/bohdaq/hitop:<version>` image is distroless: it holds the hitop binary and nothing else, not even a shell, so GitLab can't run a script in it. Every release therefore also has a `<version>-debug` tag: the same static binary on the distroless debug base, which adds a small busybox shell and runs as root, so it can write reports into the project directory. `entrypoint: [""]` replaces the image's `hitop` entrypoint so GitLab can start its shell.

Use the plain tag everywhere you call hitop directly, for example `docker run`; use the debug tag for GitLab and other runners that need a shell.

Pin the version in the tag. A new hitop release never changes your pipeline until you change that line.

## Secrets

An environment file lists a secret by name only (`token: {secret: true}`), so the value never lands in git. Add the value as a CI/CD variable (Settings, CI/CD, Variables; mark it masked and protected) named `HITOP_SECRET_<ENV>_<NAME>`, in upper case with every other character turned into `_`. For `token` in the environment `staging`, that is `HITOP_SECRET_STAGING_TOKEN`. GitLab passes it to the job as an environment variable and hitop reads it from there, so the script needs no change.

A variable with another name works through `--secret`:

```yaml
  script:
    - hitop run ./my-api --env staging --secret "token=$API_TOKEN" --reporter cli,junit --out hitop-reports
```

hitop shows secret values as `••••` in the job log and in both reports. Values given with `--var` are printed like any other value, so keep secrets out of them.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Every request passed. |
| 1 | A request or a check failed. |
| 2 | Usage or configuration error, such as an unknown environment or a missing secret. |

GitLab fails the job on any code other than 0. To let failures through without failing the pipeline, use `allow_failure: {exit_codes: [1]}`: code 2 still fails, because it means the run never really started.

## Test a service in the pipeline

GitLab `services:` are reached by their alias, not `localhost`. Point the collection at the alias with `--var`:

```yaml
api-tests:
  image:
    name: ghcr.io/bohdaq/hitop:1.1.0-debug
    entrypoint: [""]
  services:
    - name: registry.example.com/my-api:latest
      alias: api
  script:
    - hitop run ./api-tests --var base=http://api:8080 --reporter cli,junit --out hitop-reports
  artifacts:
    when: always
    reports:
      junit: hitop-reports/junit.xml
```

## Your own image instead

If the job needs other tools too, use any image with a shell and add the static Linux binary from [GitHub Releases](https://github.com/bohdaq/hitop/releases). It runs on every distribution:

```yaml
api-tests:
  image: alpine:3.22
  variables:
    HITOP_VERSION: 1.1.0
    HITOP_KEYCHAIN: "off"
  before_script:
    - apk add --no-cache curl
    - base=https://github.com/bohdaq/hitop/releases/download/cli-v$HITOP_VERSION
    - curl -fsSLO "$base/hitop-x86_64-unknown-linux-musl.tar.gz"
    - curl -fsSLO "$base/hitop-x86_64-unknown-linux-musl.tar.gz.sha256"
    - sha256sum -c hitop-x86_64-unknown-linux-musl.tar.gz.sha256
    - tar xzf hitop-x86_64-unknown-linux-musl.tar.gz
    - mv hitop-x86_64-unknown-linux-musl/hitop /usr/local/bin/
  script:
    - hitop run ./my-api --env staging --reporter cli,junit --out hitop-reports
  artifacts:
    when: always
    reports:
      junit: hitop-reports/junit.xml
```

On ARM runners use `aarch64` in place of `x86_64`. Every `hitop run` flag works the same in GitLab; see [Run collections in CI](/terms/run-collections-in-ci.html) for reports, data files and timeouts.
