use serde_json::{json, Value};

use super::*;
use crate::format::{files, Folder};
use crate::model::{ApiKeyLocation, Auth};

const SHOP: &str = r##"
openapi: 3.0.3
info: {title: Shop, version: "1.0"}
servers:
- url: https://{region}.shop.test/v1/
  variables: {region: {default: eu}}
- url: https://backup.shop.test/v1
tags: [{name: orders}, {name: pets}]
security: [{bearer: []}]
paths:
  /pets/{petId}:
    parameters:
    - {name: petId, in: path, required: true, schema: {type: integer, example: 7}}
    get:
      tags: [pets]
      operationId: getPet
      summary: Get a pet
      parameters:
      - {name: verbose, in: query, schema: {type: boolean}}
      - {name: fields, in: query, required: true, schema: {type: array, items: {type: string, enum: [name, age]}}}
      - {name: X-Trace, in: header, required: true, example: abc}
      - {name: session, in: cookie, schema: {type: string}}
      - {name: Accept, in: header, schema: {type: string}}
      responses:
        200:
          description: The pet
          content:
            application/json:
              examples:
                rex: {summary: Rex, value: {id: 7, name: Rex}}
                ext: {externalValue: "https://x.test/pet.json"}
        "404": {$ref: '#/components/responses/NotFound'}
    delete:
      tags: [pets]
      operationId: deletePet
      security: []
      requestBody: {content: {application/json: {schema: {type: object}}}}
      responses: {"204": {description: gone}}
  /pets:
    post:
      tags: [pets]
      summary: Add a pet
      requestBody:
        required: true
        content:
          application/json:
            schema: {$ref: '#/components/schemas/NewPet'}
      callbacks: {onAdd: {}}
      responses: {"201": {description: added}}
  /orders:
    post:
      tags: [orders]
      operationId: createOrder
      security: [{key: []}]
      requestBody:
        content:
          application/x-www-form-urlencoded:
            schema: {type: object, properties: {item: {type: string}, qty: {type: integer}}}
            example: {item: bone, qty: 2}
      responses: {"200": {description: ok}}
  /orders/{id}/receipt:
    put:
      tags: [orders]
      operationId: uploadReceipt
      security: [{oauth: [write]}]
      parameters: [{name: id, in: path, required: true, schema: {type: string}}]
      requestBody:
        content:
          multipart/form-data:
            schema:
              type: object
              properties:
                file: {type: string, format: binary}
                note: {type: string}
      responses: {"200": {description: ok}}
  /health:
    get:
      operationId: health
      security: [{key: [], bearer: []}]
      responses: {"200": {description: ok, content: {text/plain: {example: OK}}}}
  /remote:
    get:
      operationId: remote
      parameters: [{$ref: 'common.yaml#/components/parameters/Page'}]
      responses: {"200": {description: ok}}
components:
  responses:
    NotFound:
      description: Not found
      content: {application/json: {example: {error: missing}}}
  schemas:
    NewPet:
      allOf:
      - $ref: '#/components/schemas/Base'
      - type: object
        required: [name]
        properties:
          name: {type: string, example: Rex}
          born: {type: string, format: date}
          tags: {type: array, items: {$ref: '#/components/schemas/Tag'}}
    Base:
      type: object
      properties:
        id: {type: integer, readOnly: true}
        kind: {type: string, enum: [dog, cat]}
    Tag: {type: object, properties: {label: {type: string}}}
  securitySchemes:
    bearer: {type: http, scheme: bearer}
    key: {type: apiKey, in: query, name: api_key}
    oauth: {type: oauth2, flows: {}}
webhooks:
  newPet: {post: {responses: {"200": {description: ok}}}}
"##;

fn shop() -> (Collection, OpenApiReport) {
    import(SHOP, &Options::default()).unwrap()
}

fn req<'a>(c: &'a Collection, name: &str) -> &'a Request {
    c.requests.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("no request {name}: {:?}", c.requests.iter().map(|r| &r.name).collect::<Vec<_>>()))
}

fn issues(list: &[crate::postman::Issue]) -> String {
    list.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("\n")
}

#[test]
fn maps_operations_parameters_and_bodies() {
    let (c, report) = shop();
    assert_eq!(c.name, "Shop");
    assert_eq!(c.variables["baseUrl"], "https://eu.shop.test/v1");
    assert_eq!(c.variables["petId"], "7");
    assert_eq!(c.variables["id"], "");
    assert_eq!(c.auth, Some(Auth::Bearer { token: "${bearer}".into() }));
    assert_eq!(report.mapped.operations, 7);
    assert_eq!(report.mapped.requests, 7);
    assert_eq!(report.mapped.folders, 2);

    // Folders follow the tag list; the rest at the top.
    let files: Vec<&str> = c.requests.iter().map(|r| r.file.as_deref().unwrap()).collect();
    assert_eq!(files, ["health", "remote", "orders/createorder", "orders/uploadreceipt", "pets/add-a-pet", "pets/get-a-pet", "pets/deletepet"]);

    let get = req(&c, "Get a pet");
    assert_eq!(get.method, "GET");
    assert_eq!(get.url, "${baseUrl}/pets/${petId}?fields=name");
    assert_eq!(get.disabled_params.len(), 1);
    assert_eq!(get.disabled_params[0].key, "verbose");
    assert_eq!(get.headers.len(), 1, "Accept is the spec's to set: {:?}", get.headers);
    assert_eq!((get.headers[0].name.as_str(), get.headers[0].value.as_str(), get.headers[0].enabled), ("X-Trace", "abc", true));
    assert_eq!(get.auth, None, "inherits the collection's");
    let names: Vec<&str> = get.examples.iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["200 Rex", "404 Not found"]);
    assert_eq!(get.examples[0]["status"], 200);
    assert_eq!(serde_json::from_str::<Value>(get.examples[0]["body"].as_str().unwrap()).unwrap(), json!({"id": 7, "name": "Rex"}));
    assert_eq!(get.examples[0]["source"], "openapi");
    let rec: OpRecord = serde_json::from_value(get.extra["openapi"].clone()).unwrap();
    assert_eq!((rec.operation.as_str(), rec.operation_id.as_deref()), ("GET /pets/{petId}", Some("getPet")));
    assert_eq!(rec.synced.len(), 7);

    let del = req(&c, "deletePet");
    assert_eq!(del.auth, Some(Auth::None), "security: [] turns the collection's off");
    assert!(del.body.is_empty(), "an optional DELETE body is left out");

    let add = req(&c, "Add a pet");
    assert_eq!(add.headers[0].name, "Content-Type");
    let body: Value = serde_json::from_str(&add.body).unwrap();
    assert_eq!(body, json!({"born": "2026-01-01", "kind": "dog", "name": "Rex", "tags": [{"label": "string"}]}), "allOf merged, readOnly id left out");

    let order = req(&c, "createOrder");
    assert_eq!(order.body_mode.as_deref(), Some("urlencoded"));
    assert_eq!(order.form_fields.iter().map(|f| (f.key.as_str(), f.value.as_str())).collect::<Vec<_>>(), [("item", "bone"), ("qty", "2")]);
    assert_eq!(order.auth, Some(Auth::ApiKey { key: "api_key".into(), value: "${key}".into(), location: ApiKeyLocation::Query }));

    let upload = req(&c, "uploadReceipt");
    assert_eq!(upload.body_mode.as_deref(), Some("multipart"));
    assert_eq!(upload.form_fields.iter().map(|f| (f.key.as_str(), f.kind.as_str())).collect::<Vec<_>>(), [("file", "file"), ("note", "text")]);
    assert_eq!(upload.url, "${baseUrl}/orders/${id}/receipt");

    let health = req(&c, "health");
    assert_eq!(health.examples[0]["body"], "OK");

    let skipped = issues(&report.skipped);
    for want in [
        "GET /pets/{petId}: cookie parameter session",
        "GET /pets/{petId}: example ext with externalValue",
        "POST /pets: callbacks",
        "GET /remote: parameter: external $ref common.yaml#/components/parameters/Page",
        "security scheme oauth: OAuth 2",
        "webhooks: 1 webhook",
        "GET /health: security schemes bearer and key together",
    ] {
        assert!(skipped.contains(want), "{want} missing in\n{skipped}");
    }
    let warnings = issues(&report.warnings);
    assert!(warnings.contains("servers: 2 servers: baseUrl uses the first"), "{warnings}");
    assert!(warnings.contains("security scheme bearer: set bearer in an environment (as a secret)"), "{warnings}");
    assert!(report.mapped.auth.iter().any(|a| a.starts_with("collection: bearer")), "{:?}", report.mapped.auth);
}

#[test]
fn the_record_survives_a_folder_round_trip() {
    let (c, _) = shop();
    let files = files::to_files(&Folder { collection: c.clone(), ..Default::default() }).unwrap();
    let text = &files["requests/pets/get-a-pet.yaml"];
    assert!(text.contains("openapi:\n  operation: GET /pets/{petId}\n  operationId: getPet\n"), "{text}");
    assert!(files["hitop.yaml"].contains("openapi:"), "{}", files["hitop.yaml"]);
    let back = files::read_files(&files, "x").unwrap().collection;
    assert_eq!(back.requests.len(), c.requests.len());
    assert_eq!(back.extra["openapi"], c.extra["openapi"]);
    assert_eq!(req(&back, "Get a pet").extra["openapi"], req(&c, "Get a pet").extra["openapi"]);
    // Nothing to do when the spec is the same.
    let s = sync(&back, SHOP, &Options::default(), false).unwrap();
    assert!(s.diff.is_empty(), "{:?}", s.diff);
    assert_eq!(s.diff.unchanged, 7);
}

#[test]
fn recursive_schemas_stop() {
    let spec = r##"
openapi: 3.1.0
info: {title: Tree}
paths:
  /nodes:
    post:
      requestBody:
        content:
          application/json:
            schema: {$ref: '#/components/schemas/Node'}
      responses: {}
components:
  schemas:
    Node:
      type: object
      properties:
        name: {type: [string, "null"]}
        children: {type: array, items: {$ref: '#/components/schemas/Node'}}
        other: {$ref: '#/components/schemas/A'}
    A: {type: object, properties: {b: {$ref: '#/components/schemas/B'}, n: {const: 3}}}
    B: {type: object, properties: {a: {$ref: '#/components/schemas/A'}}}
    Loop: {$ref: '#/components/schemas/Loop'}
"##;
    let (c, report) = import(spec, &Options::default()).unwrap();
    let body: Value = serde_json::from_str(&c.requests[0].body).unwrap();
    assert_eq!(body, json!({"children": [], "name": "string", "other": {"b": {}, "n": 3}}));
    assert_eq!(c.requests[0].name, "POST /nodes");
    assert!(issues(&report.warnings).contains("no servers"), "{:?}", report.warnings);
    let r = Resolver::new(&parse(spec).unwrap()).get(&json!({"$ref": "#/components/schemas/Loop"})).map(|_| ()).unwrap_err();
    assert_eq!(r, RefError::Loop("#/components/schemas/Loop".into()));
}

#[test]
fn json_pointers_unescape() {
    let spec = json!({"paths": {"/a/{b}": {"x~y": 1}}, "%": 2});
    let r = Resolver::new(&spec);
    assert_eq!(r.pointer("#/paths/~1a~1%7Bb%7D/x~0y").unwrap(), &json!(1));
    assert_eq!(r.pointer("#/%25").unwrap(), &json!(2));
    assert!(matches!(r.pointer("#/nope"), Err(RefError::Missing(_))));
    assert!(matches!(r.pointer("other.json#/a"), Err(RefError::External(_))));
}

#[test]
fn refuses_what_isnt_openapi_3() {
    let e = import("swagger: '2.0'\ninfo: {title: x}\npaths: {}\n", &Options::default()).unwrap_err().to_string();
    assert!(e.contains("Swagger 2.0") && e.contains("3.0 and 3.1"), "{e}");
    assert!(import("{\"info\": {}}", &Options::default()).unwrap_err().to_string().contains("not an OpenAPI spec"));
    assert!(import("openapi: 4.0.0\n", &Options::default()).is_err());
    assert!(detect("{\"openapi\": \"3.1.0\"}") && detect("openapi: 3.0.0\n") && !detect("{\"info\": {}, \"item\": []}"));
}

#[test]
fn relative_servers_resolve_against_the_spec_url() {
    let spec = "openapi: 3.0.0\ninfo: {title: R}\nservers: [{url: /api/v3}]\npaths: {}\n";
    let opts = Options { source: Some("https://petstore.test/docs/openapi.json".into()), ..Default::default() };
    let (c, report) = import(spec, &opts).unwrap();
    assert_eq!(c.variables["baseUrl"], "https://petstore.test/api/v3");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let (c, report) = import(spec, &Options::default()).unwrap();
    assert_eq!(c.variables["baseUrl"], "/api/v3");
    assert!(issues(&report.warnings).contains("relative server URL /api/v3"));
    assert_eq!(join_url("https://h.test/a/b.yaml", "v2"), "https://h.test/a/v2");
    let rec: SpecRecord = serde_json::from_value(import(spec, &opts).unwrap().0.extra["openapi"].clone()).unwrap();
    assert_eq!(rec.source.as_deref(), Some("https://petstore.test/docs/openapi.json"));
}

#[test]
fn flat_and_inline_auth() {
    let opts = Options { flat: true, inline_auth: true, ..Default::default() };
    let (c, _) = import(SHOP, &opts).unwrap();
    assert!(c.requests.iter().all(|r| !r.file.as_deref().unwrap().contains('/')));
    assert_eq!(c.auth, None);
    assert_eq!(req(&c, "Get a pet").auth, Some(Auth::Bearer { token: "${bearer}".into() }));
    assert_eq!(req(&c, "deletePet").auth, None);
    // Sync compares auth the same way.
    let s = sync(&c, SHOP, &opts, false).unwrap();
    assert!(s.diff.is_empty(), "{:?}", s.diff);
}

#[test]
fn json_specs_and_every_operation_s_auth() {
    let spec = json!({
        "openapi": "3.1.0", "info": {"title": "J"}, "servers": [{"url": "http://localhost:8080"}],
        "paths": {"/a": {"get": {"security": [{"basic": []}], "responses": {}}}, "/b": {"get": {"security": [{"basic": []}], "responses": {}}}},
        "components": {"securitySchemes": {"basic": {"type": "http", "scheme": "Basic"}}}
    });
    let (c, report) = import(&spec.to_string(), &Options::default()).unwrap();
    assert_eq!(c.auth, Some(Auth::Basic { username: "${basicUsername}".into(), password: "${basicPassword}".into() }));
    assert!(c.requests.iter().all(|r| r.auth.is_none()));
    assert_eq!(report.mapped.requests, 2);
}

/// The spec after a release: getPet gains a header and a new URL, deletePet
/// is gone, listPets is new, createOrder's body changed, health's example.
fn shop_v2() -> String {
    SHOP.replace("info: {title: Shop, version: \"1.0\"}", "info: {title: Shop, version: \"2.0\"}")
        .replace("      - {name: X-Trace, in: header, required: true, example: abc}", "      - {name: X-Trace, in: header, required: true, example: abc}\n      - {name: X-Tenant, in: header, required: true, example: t1}")
        .replace("  /pets/{petId}:\n", "  /animals/{petId}:\n")
        .replace("    delete:\n      tags: [pets]\n      operationId: deletePet\n      security: []\n      requestBody: {content: {application/json: {schema: {type: object}}}}\n      responses: {\"204\": {description: gone}}\n", "")
        .replace("  /pets:\n    post:\n", "  /pets:\n    get:\n      tags: [pets]\n      operationId: listPets\n      responses: {\"200\": {description: ok}}\n    post:\n")
        .replace("example: {item: bone, qty: 2}", "example: {item: ball, qty: 1}")
        .replace("text/plain: {example: OK}", "text/plain: {example: ALIVE}")
        .replace("https://{region}.shop.test/v1/", "https://{region}.shop.test/v2/")
}

#[test]
fn sync_keeps_your_edits_and_applies_the_spec_s() {
    let (c, _) = shop();
    // Through the folder files, as the CLI reads it.
    let folder = files::to_files(&Folder { collection: c, ..Default::default() }).unwrap();
    let mut c = files::read_files(&folder, "x").unwrap().collection;
    {
        let get = c.requests.iter_mut().find(|r| r.name == "Get a pet").unwrap();
        get.name = "Fetch pet".into();
        get.file = Some("mine/fetch".into());
        get.pre_request_script = "setHeader('X-Run', '1');".into();
        get.post_request_script = "assert.status(200);".into();
        get.auth = Some(Auth::ApiKey { key: "X-Key".into(), value: "${k}".into(), location: ApiKeyLocation::Header });
        get.examples.push(json!({"name": "mine", "status": 200, "body": "{}"}));
        let order = c.requests.iter_mut().find(|r| r.name == "createOrder").unwrap();
        order.form_fields[1].value = "5".into();
        let add = c.requests.iter_mut().find(|r| r.name == "Add a pet").unwrap();
        add.post_request_script = "assert.status(201);".into();
    }
    // And back through the files: your edits as a folder holds them.
    let folder = files::to_files(&Folder { collection: c, ..Default::default() }).unwrap();
    let c = files::read_files(&folder, "x").unwrap().collection;

    let dry = sync(&c, &shop_v2(), &Options::default(), false).unwrap();
    let d = &dry.diff;
    assert_eq!(d.added.iter().map(|a| (a.operation.as_str(), a.file.as_str())).collect::<Vec<_>>(), [("GET /pets", "pets/listpets")]);
    assert_eq!(d.removed.iter().map(|a| a.operation.as_str()).collect::<Vec<_>>(), ["DELETE /pets/{petId}"]);
    let changed = |id: &str| d.changed.iter().find(|c| c.operation_id.as_deref() == Some(id)).unwrap_or_else(|| panic!("{id} not changed: {d:?}"));
    let get = changed("getPet");
    assert_eq!(get.operation, "GET /animals/{petId}", "matched by operationId though the path moved");
    assert_eq!(get.name, "Fetch pet");
    assert_eq!(get.updated, ["url", "headers"]);
    assert!(get.kept.is_empty(), "{get:?}");
    let order = changed("createOrder");
    assert_eq!(order.kept, ["body"], "{order:?}");
    assert_eq!(changed("health").updated, ["examples"]);
    assert_eq!(d.collection.updated, ["baseUrl"]);
    assert!(!d.pruned);

    let s = dry.collection;
    let get = req(&s, "Fetch pet");
    assert_eq!(get.url, "${baseUrl}/animals/${petId}?fields=name");
    assert_eq!(get.headers.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(), ["X-Trace", "X-Tenant"]);
    assert_eq!(get.file.as_deref(), Some("mine/fetch"));
    assert_eq!(get.pre_request_script, "setHeader('X-Run', '1');");
    assert_eq!(get.post_request_script, "assert.status(200);");
    assert_eq!(get.auth, Some(Auth::ApiKey { key: "X-Key".into(), value: "${k}".into(), location: ApiKeyLocation::Header }));
    assert!(get.examples.iter().any(|e| e["name"] == "mine"), "your example stays");
    assert_eq!(req(&s, "createOrder").form_fields[1].value, "5", "your edit wins over the spec's change");
    assert_eq!(req(&s, "Add a pet").post_request_script, "assert.status(201);");
    assert_eq!(req(&s, "health").examples[0]["body"], "ALIVE");
    assert!(s.requests.iter().any(|r| r.name == "deletePet"), "removed operations stay without prune");
    assert_eq!(s.variables["baseUrl"], "https://eu.shop.test/v2");
    let rec: SpecRecord = serde_json::from_value(s.extra["openapi"].clone()).unwrap();
    assert_eq!(rec.version, "2.0");

    // Applied and written, a second sync has nothing left but the kept edit.
    let folder = files::to_files(&Folder { collection: s.clone(), ..Default::default() }).unwrap();
    let again = sync(&files::read_files(&folder, "x").unwrap().collection, &shop_v2(), &Options::default(), false).unwrap();
    assert!(again.diff.added.is_empty() && again.diff.changed.is_empty(), "{:?}", again.diff);
    assert_eq!(again.diff.removed.len(), 1);

    // --prune takes deletePet out.
    let pruned = sync(&s, &shop_v2(), &Options::default(), true).unwrap();
    assert!(pruned.diff.pruned && !pruned.collection.requests.iter().any(|r| r.name == "deletePet"));
    assert_eq!(pruned.collection.requests.len(), 7);
}

#[test]
fn sync_needs_an_imported_collection() {
    let e = sync(&Collection { name: "Plain".into(), ..Default::default() }, SHOP, &Options::default(), false).unwrap_err().to_string();
    assert!(e.contains("wasn't imported from an OpenAPI spec"), "{e}");
}
