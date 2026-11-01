<!--
hitop run report, version 1
Copyright 2026 Bohdan Tsap
This specification is licensed under the Creative Commons Attribution 4.0
International License (CC BY 4.0): https://creativecommons.org/licenses/by/4.0/
You may share and adapt it, including commercially, if you give credit.
The hitop code that implements it is MIT licensed.
-->

# hitop run report, version 1

Reference implementation: `cli/crates/hitop-core/src/report.rs`.

`hitop run <collection> --reporter json --out <dir>` writes `<dir>/report.json`
(and `--reporter junit` writes `<dir>/junit.xml`, section 4, and
`--reporter html` writes `<dir>/report.html`, section 5). Tools that read
the result of a run, such as the GitHub Action's job summary, read this file.

## 1. Stability

- `reportVersion` is `1`. Within version 1 fields may be **added**, never
  removed, renamed or given another meaning or type. Readers must ignore keys
  they don't know.
- A change that would break a reader bumps `reportVersion`. A reader that
  sees a higher version than it knows should say so rather than guess.
- Keys are camelCase. Times are milliseconds (integers). Timestamps are
  RFC 3339 in UTC with whole seconds (`2026-11-01T12:00:00Z`).
- A key documented as "or null" is always present, with `null` when it has
  no value.
- Secret values never appear in a report: wherever one would (URL, error,
  assertion name or message) it is `••••`.

## 2. Example

```json
{
  "reportVersion": 1,
  "tool": { "name": "hitop", "version": "0.3.0" },
  "startedAt": "2026-11-01T12:00:00Z",
  "durationMs": 182,
  "passed": false,
  "bailed": false,
  "summary": {
    "collections": 1,
    "iterations": 1,
    "requests": { "total": 2, "passed": 1, "failed": 1 },
    "assertions": { "total": 3, "passed": 2, "failed": 1 }
  },
  "collections": [
    {
      "name": "Shop API",
      "environment": "staging",
      "passed": false,
      "iterations": [
        {
          "iteration": 1,
          "startedAt": "2026-11-01T12:00:00Z",
          "durationMs": 180,
          "data": null,
          "passed": false,
          "requests": [
            {
              "name": "Get user",
              "method": "GET",
              "url": "https://api.example.com/users/1?key=••••",
              "passed": true,
              "status": 200,
              "responseTimeMs": 41,
              "durationMs": 47,
              "error": null,
              "failure": null,
              "assertions": [
                { "name": "status is 200", "passed": true, "message": null }
              ],
              "file": "shop-api/requests/get-user.yaml"
            },
            {
              "name": "Create order",
              "method": "POST",
              "url": "https://api.example.com/orders",
              "passed": false,
              "status": 200,
              "responseTimeMs": 120,
              "durationMs": 131,
              "error": null,
              "failure": "status is 201: got 200",
              "assertions": [
                { "name": "status is 201", "passed": false, "message": "got 200" },
                { "name": "id", "passed": true, "message": null }
              ],
              "file": "shop-api/requests/orders/create-order.yaml"
            }
          ]
        }
      ]
    }
  ]
}
```

## 3. Fields

### Report

| Key | Type | Meaning |
| --- | --- | --- |
| `reportVersion` | integer | `1`. |
| `tool` | object | `name` (`"hitop"`) and `version` of the hitop that ran. |
| `startedAt` | string | When the run started. |
| `durationMs` | integer | The whole run. |
| `passed` | boolean | Every request passed. Same as `summary.requests.failed == 0`; `hitop run` exits 0 exactly when this is true. |
| `bailed` | boolean | `--bail` stopped the run at the first failure; later requests and iterations are not in the report. |
| `summary` | object | `collections` and `iterations` (counts), `requests` and `assertions` (each `total`, `passed`, `failed`). |
| `collections` | array | One entry per collection run (`hitop run --all` runs several), in order. |

### Collection

| Key | Type | Meaning |
| --- | --- | --- |
| `name` | string | Collection name. |
| `environment` | string or null | The environment's name (`--env`, or the `name` of `--env-file`). |
| `passed` | boolean | Every iteration passed. |
| `iterations` | array | One entry per iteration: `--iterations <n>`, or one per `--data` row; one without either. |

### Iteration

| Key | Type | Meaning |
| --- | --- | --- |
| `iteration` | integer | 1-based. |
| `startedAt` | string | When the iteration started. |
| `durationMs` | integer | The iteration. |
| `data` | object or null | The `--data` row it ran with: variable name to value, all strings. |
| `passed` | boolean | Every request in it passed. |
| `requests` | array | The requests run, in order. |

### Request

A request **passes** when it was sent, answered with a status below 400, and
every assertion passed.

| Key | Type | Meaning |
| --- | --- | --- |
| `name` | string | Request name. |
| `method` | string | HTTP method. |
| `url` | string | The URL as sent (variables filled in, secrets masked). |
| `passed` | boolean | See above. |
| `status` | integer or null | HTTP status; null when no response came. |
| `responseTimeMs` | integer or null | Time until the response headers; null without a response. |
| `durationMs` | integer | The whole step: scripts, request and response body. |
| `error` | string or null | Why the request wasn't sent or answered: connection, timeout (`--timeout`), a missing variable, a script error. |
| `failure` | string or null | One line on why the request failed (the `error`, the failed assertions joined by `; `, or `HTTP <status>`); null when it passed. |
| `assertions` | array | The post-request script's `assert.*` results, in order. |
| `file` | string or null | The request's file when the collection ran from a collection folder: the folder as given on the command line (without a leading `./`, `/` as separator), then `requests/<file>.yaml` (`api/requests/orders/create-order.yaml`); null for a collection in global storage. Added in hitop 0.3.0; reports from earlier versions don't have the key. |

### Assertion

| Key | Type | Meaning |
| --- | --- | --- |
| `name` | string | What was checked (`status is 200`, or the name given to `assert.equal`/`assert.ok`). |
| `passed` | boolean | |
| `message` | string or null | What was found instead (`got 500`); null when it passed. |

Response bodies and headers are not part of the report.

## 4. JUnit XML

`junit.xml` follows the common JUnit schema read by CI systems:

- Root `<testsuites name="hitop" tests failures errors time timestamp>`.
- One `<testsuite name tests failures errors skipped time timestamp>` per
  collection and iteration. With several iterations its name is
  `<collection> (iteration <n>)`. Its `<properties>` hold `iteration`,
  `environment` and the data row as `data.<name>`.
- One `<testcase name classname time>` per request; `classname` is the
  collection name, `time` in seconds. When the request has a `file` (see
  the Request table) the testcase also has `file="<path>"`, so CI tools can
  put annotations on the request file; run hitop from the repository root
  with a relative folder path to get repository-relative paths. Added in
  hitop 0.3.0, within version 1.
- A failed request holds exactly one child: `<error type="RequestError">`
  when no response came, otherwise `<failure type="AssertionError">` (an
  assertion failed) or `<failure type="HttpStatus">` (status 400 or above).
  `message` is the request's `failure` line; the element's text is the
  request line followed by every assertion, `✓` passed and `✗` failed with
  its message. A passing request lists the same in `<system-out>`.
- `timestamp` is UTC without a zone suffix (`2026-11-01T12:00:00`).

## 5. HTML report

`report.html` is the same report (sections 2 and 3) as one page for
people: summary, every collection and iteration (with its `--data` row),
every request with its method, status, timings, URL, file, error, failure
line and assertions. Failed requests start expanded; a "Show failed
requests only" checkbox hides the rest.

- **Self-contained.** Inline CSS, no script, no images, fonts or links to
  load. A `Content-Security-Policy` of `default-src 'none'; style-src
  'unsafe-inline'` means a browser opens no connection for it, so it works
  offline and as a CI artifact. Light and dark follow
  `prefers-color-scheme`.
- **Escaped.** Every string from the report (names, URLs, messages, data
  rows, file paths) is HTML-escaped (`& < > " '`); control characters
  other than tab and newlines become U+FFFD. Only whole numbers from the
  report go into a `style` attribute.
- **Masked.** It holds what `report.json` holds: secret values are `••••`,
  response bodies and headers are not in it.
- **Deterministic.** The page is a function of the report alone (no clock,
  no random ids): the same report gives the same bytes.
- The markup and styles are not an interface: tools should read
  `report.json`. Added after hitop 0.3.0.
