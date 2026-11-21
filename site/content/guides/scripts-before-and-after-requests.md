---
title: "Scripts before and after a request"
description: "Change a request before it is sent and check its response after with JavaScript in HITOP: the script API, assertions, values passed between requests, and Postman pm.* scripts."
---

## Where scripts live

Every request has two optional scripts, written in JavaScript:

- **Script before** runs just before the request is sent. It can change the URL, headers and body.
- **Script after** runs when the response arrives. It checks the response and keeps values for the next requests.

In the web app, the desktop app and the extensions they are tabs of the request, next to Body. In a collection folder they are `scripts.pre` and `scripts.post` of the request's YAML file, so `hitop run` in CI runs the same scripts:

```yaml
scripts:
  pre: setHeader('Idempotency-Key', String(Date.now()));
  post: |
    assert.status(201);
    setContext('orderId', getResponseValue('id'));
```

Scripts run in the browser page (in the Chrome extension, in its sandboxed page), and in the CLI and the desktop app in a JavaScript engine built into hitop. They can't read files or reach the network.

## The script before

| Name | What it is |
| --- | --- |
| `url`, `method`, `headers`, `body` | The request about to be sent: `headers` is a list of `{name, value}` |
| `setUrl(url)` | Send to this URL instead |
| `setHeader(name, value)` | Add a header, or change the one with this name |
| `setBody(text)` | Send this body instead |
| `getVariable(name)` | The value `${name}` has for this request |
| `getContext(name)`, `setContext(name, value)` | Values kept between requests (below) |

```js
const token = getContext('token');
if (token) setHeader('Authorization', `Bearer ${token}`);
setHeader('X-Request-Id', `req-${Date.now()}`);
```

## The script after

| Name | What it is |
| --- | --- |
| `statusCode` | The status code, a number |
| `response` | The body parsed as JSON (when it is JSON) |
| `responseText` | The body as text |
| `responseHeaders` | Headers by lowercase name |
| `getResponseValue('data.user.id')` | A value from the JSON body by its path |
| `getResponseHeader(name)` | One header's value |
| `getVariable`, `getContext`, `setContext` | As in the script before |

```js
assert.status(200);
assert.header('content-type', 'application/json');
assert.responseTime(500);
assert.ok(Array.isArray(response.items), 'items is a list');
setContext('firstId', getResponseValue('items.0.id'));
```

## Checks

Every `assert` call is a check with a name, shown under the response and counted in run reports (JUnit and JSON, see [Run collections in CI](/guides/run-collections-in-ci/)). A request with a failed check fails the run.

| Check | Passes when |
| --- | --- |
| `assert.status(201)` | The status is 201 |
| `assert.bodyContains('"ok"')` | The body contains the text |
| `assert.header('etag')` | The header is there |
| `assert.header('content-type', 'json')` | The header contains the text |
| `assert.responseTime(500)` | The response came within 500 ms |
| `assert.equal(actual, expected, name?)` | The two values are equal as JSON |
| `assert.ok(condition, name?)` | The condition is true |

`console.log` (and `info`, `warn`, `error`) prints to the response's Console tab in the apps.

## Values between requests

`setContext(name, value)` keeps a value for the requests that come after it: a login request's script after stores the token, the next requests read it with `getContext`. Context values can be any JSON value. They aren't variables: use them from scripts, not as `${name}`.

To set a variable that `${name}` fills in, use the Postman API below: `pm.environment.set('token', value)`. A value a script sets wins over the collection, environment and globals for the rest of the run (in the web app, until the page reloads). See [Variables and environments](/guides/variables-and-environments/).

## Postman scripts

Scripts that use `pm.*` run as they do in Postman, on the same values: `pm.test`, `pm.expect`, `pm.response` (`code`, `json()`, `text()`, `headers`, `responseTime`), `pm.request`, `pm.environment`, `pm.collectionVariables`, `pm.variables` and `pm.globals`. Collection, environment and local values are one set of variables in HITOP, so `pm.environment` and `pm.collectionVariables` read and write the same values; `pm.globals` is its own scope.

```js
pm.test('created', () => {
  pm.response.to.have.status(201);
  pm.environment.set('orderId', pm.response.json().id);
});
```

When you import a Postman collection, the import report lists every script line that calls something HITOP doesn't provide. [Migrate from Postman](/guides/migrate-from-postman/) walks through it.
