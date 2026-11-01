# Postman fixtures

Three public Postman collections, vendored unchanged as `collection.json` with their
license, so the importer and the `pm.*` shim are tested on collections people wrote,
not only on ones written for hitop. `cli/crates/hitop/tests/postman.rs` imports each
one with `hitop import --to` and runs it against a local mock server (httpmock); no
test reaches the public internet.

| Folder | Source | License | What it exercises |
| --- | --- | --- | --- |
| `conduit/` | [gothinkster/realworld](https://github.com/gothinkster/realworld) `api/Conduit.postman_collection.json` at `dedb6969fe2e025ccac06464f01bc25584d9290c` | MIT (`conduit/LICENSE`) | folders, Postman's legacy sandbox (`tests[...]`, `responseBody`, `responseCode`, `environment`), `pm.globals` carrying a token and ids from one request to the next |
| `newman-sample/` | [postmanlabs/newman](https://github.com/postmanlabs/newman) `examples/sample-collection.json` at `8fbb2eb43860193344dcccec3f67ec62c77d80f9` | Apache-2.0 (`newman-sample/LICENSE`) | collection format v2.0 (string URLs), `pm.test`, `pm.response.to.be.ok`, chai `pm.expect` chains |
| `adyen-binlookup/` | [Adyen/adyen-postman](https://github.com/Adyen/adyen-postman) `postman/BinLookupService-v54.json` at `50d57dd28d3720ecf08a6de63786903e069f7472` | MIT (`adyen-binlookup/LICENSE`) | nested folders, collection-level API key auth, collection variables, saved responses |

The environment and globals files next to them (`*.postman_environment.json`,
`*.postman_globals.json`) are written for hitop's tests (MIT, like the rest of hitop).
Their secret values are dummies.
