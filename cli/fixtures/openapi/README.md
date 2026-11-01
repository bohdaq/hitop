# OpenAPI fixtures

Public specs used by the OpenAPI import and sync tests
(`crates/hitop-core/tests/openapi.rs`, `crates/hitop/tests/openapi.rs`). Each is
vendored unchanged, with the license of the repository it comes from.

| Folder | Source | License | What it exercises |
| --- | --- | --- | --- |
| `petstore/` | [swagger-api/swagger-petstore](https://github.com/swagger-api/swagger-petstore) `src/main/resources/openapi.yaml` at `5cb909fc7780bc708151bce0014939acba8b212a` ([raw](https://raw.githubusercontent.com/swagger-api/swagger-petstore/5cb909fc7780bc708151bce0014939acba8b212a/src/main/resources/openapi.yaml)) | Apache-2.0 (`petstore/LICENSE`) | OpenAPI 3.0, YAML, tags, path and query parameters, JSON, XML and form bodies, API key and OAuth 2 schemes |
| `adyen-checkout/` | [Adyen/adyen-openapi](https://github.com/Adyen/adyen-openapi) `yaml/CheckoutService-v71.yaml` at `1d165a07700491fe3eff1e368ffe090a33251e19` ([raw](https://raw.githubusercontent.com/Adyen/adyen-openapi/1d165a07700491fe3eff1e368ffe090a33251e19/yaml/CheckoutService-v71.yaml)) | MIT (`adyen-checkout/LICENSE`) | OpenAPI 3.1, 805 KB, 28 operations, named request and response examples through `$ref`, basic and API key auth, header parameters |
| `kubernetes-apps/` | [kubernetes/kubernetes](https://github.com/kubernetes/kubernetes) `api/openapi-spec/v3/apis__apps__v1_openapi.json` at `3c690c189e5d367715dbd056a5c2009a476c8d77` ([raw](https://raw.githubusercontent.com/kubernetes/kubernetes/3c690c189e5d367715dbd056a5c2009a476c8d77/api/openapi-spec/v3/apis__apps__v1_openapi.json)) | Apache-2.0 (`kubernetes-apps/LICENSE`) | OpenAPI 3.0, JSON, 919 KB, 77 operations, deep `allOf` schemas, `*/*` bodies, no servers |

The example values in these specs are the publishers' placeholders
(`YOUR_MERCHANT_ACCOUNT`); none is a credential.
