<!--
hitop collection format compatibility suite, version 1
Copyright 2026 Bohdan Tsap. Licensed under CC BY 4.0, like the specification:
https://creativecommons.org/licenses/by/4.0/
-->

# Format v1 compatibility suite

Collection folders in format 1 (`../../collection-format.md`) that every
reader of format 1, in hitop or any other tool, must read. The format is
stable: these files never change, and new cases are only added.

Each case directory holds:

- `folder/`: the collection folder, as a v1 tool wrote it (or, where
  `case.json` says `"canonical": false`, input a writer wouldn't produce but a
  reader must accept: flow style, `.yml`, numbers and booleans as strings,
  hidden and non-YAML files, a blank header row).
- `expected.json`: the JSON model the folder reads as (`{collection,
  environments, globals}`, section 6 of the spec): ids, derived ids, default
  names, `secrets` lists, `file`, and every unknown key carried over.
- `case.json` (optional): `{"canonical": false}` when the writer wouldn't
  write `folder/` byte for byte.

| Case | Covers |
| --- | --- |
| `minimal` | `format: 1` and a request with only a URL: every default |
| `http` | order, ids, workspace, collection auth; JSON, XML, text, form and multipart bodies; bearer, basic, API key (header and query) and `none` auth; params with disabled rows; scripts; a URL kept as typed; an environment |
| `graphql` | GraphQL bodies with variables text and operation name, and with a Content-Type of its own |
| `websocket` | `kind: websocket`, subprotocols, saved messages with an unknown row key |
| `grpc` | `kind: grpc`, metadata, protos and import paths, plaintext; reflection over TLS |
| `openapi` | an OpenAPI import (`spec.yaml`, by `hitop import openapi`): the `openapi` records and `source: openapi` examples |
| `openapi-rows` | an OpenAPI import by hitop 1.1: the request record's per-row hashes (`openapi.rows`) for headers and query parameters |
| `secrets` | secret variables in environments and globals, `globals.yaml`, secret manager references (`${secret:vault|aws|azure|op:...}`) |
| `examples` | saved responses with extra keys |
| `unknown-keys` | unknown keys on the collection, a request, an environment and rows (kept and written back) |
| `quirks` | non-canonical input every reader must take |
| `run-report/report.json` | a `reportVersion: 1` run report (`../../run-report.md`); later writers keep every key with the same type |

hitop runs the suite in `cli/crates/hitop-core/tests/compat.rs`: read each
folder (from disk and through the in-memory reader the web app uses) and
compare with `expected.json`, write it back and read it again, and compare
the written files with `folder/` for canonical cases. A new case without
`expected.json` makes the test write one and fail, so it is reviewed before
it is committed.
