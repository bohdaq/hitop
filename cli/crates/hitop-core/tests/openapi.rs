//! The OpenAPI fixtures (`cli/fixtures/openapi/`, sources in its README):
//! counts and key mappings, a folder round trip, and a no-op sync.
use hitop_core::format::{files, Folder};
use hitop_core::model::{ApiKeyLocation, Auth, Collection, Request};
use hitop_core::openapi::{import, sync, OpenApiReport, Options};
use serde_json::Value;

fn fixture(p: &str) -> String {
    std::fs::read_to_string(format!("{}/../../fixtures/openapi/{p}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn req(c: &Collection, pred: impl Fn(&Request) -> bool) -> &Request {
    c.requests.iter().find(|r| pred(r)).expect("request")
}

fn op(r: &Request) -> String {
    let id = r.extra["openapi"]["operationId"].as_str().unwrap_or("");
    format!("{} {id}", r.extra["openapi"]["operation"].as_str().unwrap())
}

fn text(list: &[hitop_core::postman::Issue]) -> String {
    list.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("\n")
}

/// Written as a folder and read back, the collection is the same, and a
/// sync with the same spec changes nothing.
fn round_trip(c: &Collection, spec: &str) {
    let files = files::to_files(&Folder { collection: c.clone(), ..Default::default() }).unwrap();
    let back = files::read_files(&files, "x").unwrap().collection;
    assert_eq!(back.requests.len(), c.requests.len());
    let again = files::to_files(&Folder { collection: back.clone(), ..Default::default() }).unwrap();
    assert_eq!(files, again, "a folder round trip changes nothing");
    let s = sync(&back, spec, &Options::default(), false).unwrap();
    assert!(s.diff.is_empty() && s.diff.changed.is_empty(), "{:?}", s.diff);
    assert_eq!(s.diff.unchanged, c.requests.len());
}

#[test]
fn petstore_3_0() {
    let spec = fixture("petstore/openapi.yaml");
    let (c, report): (Collection, OpenApiReport) = import(&spec, &Options::default()).unwrap();
    assert_eq!(report.mapped.openapi, "3.0.4");
    assert_eq!(c.name, "Swagger Petstore - OpenAPI 3.0");
    assert_eq!((report.mapped.operations, report.mapped.requests, report.mapped.folders), (19, 19, 3));
    assert_eq!(c.variables["baseUrl"], "https://petstore3.swagger.io/api/v3");
    assert_eq!(report.mapped.variables, ["baseUrl", "orderId", "petId", "username"]);
    let folders: std::collections::BTreeSet<&str> = c.requests.iter().filter_map(|r| r.file.as_deref()?.split_once('/').map(|(d, _)| d)).collect();
    assert_eq!(folders.into_iter().collect::<Vec<_>>(), ["pet", "store", "user"]);

    let by_status = req(&c, |r| op(r) == "GET /pet/findByStatus findPetsByStatus");
    assert_eq!(by_status.url, "${baseUrl}/pet/findByStatus?status=available");
    let get = req(&c, |r| op(r) == "GET /pet/{petId} getPetById");
    assert_eq!(get.url, "${baseUrl}/pet/${petId}");
    assert_eq!(get.auth, Some(Auth::ApiKey { key: "api_key".into(), value: "${api_key}".into(), location: ApiKeyLocation::Header }));
    let add = req(&c, |r| op(r) == "POST /pet addPet");
    let body: Value = serde_json::from_str(&add.body).unwrap();
    assert_eq!(body["name"], "doggie");
    assert_eq!(body["category"]["name"], "Dogs");
    assert_eq!(add.headers[0].value, "application/json");
    let form = req(&c, |r| op(r) == "POST /pet/{petId} updatePetWithForm");
    assert_eq!(form.url, "${baseUrl}/pet/${petId}", "its query parameters are optional: {form:?}");
    assert_eq!(form.disabled_params.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(), ["name", "status"]);
    let login = req(&c, |r| op(r) == "GET /user/login loginUser");
    assert!(login.disabled_params.iter().any(|p| p.key == "password"));

    let skipped = text(&report.skipped);
    assert!(skipped.contains("POST /pet/{petId}/uploadImage: request body application/octet-stream"), "{skipped}");
    assert!(skipped.contains("security scheme petstore_auth: OAuth 2"), "{skipped}");
    assert!(text(&report.warnings).contains("set api_key in an environment"));
    round_trip(&c, &spec);
}

#[test]
fn adyen_checkout_3_1() {
    let spec = fixture("adyen-checkout/CheckoutService-v71.yaml");
    let (c, report) = import(&spec, &Options::default()).unwrap();
    assert_eq!(report.mapped.openapi, "3.1.0");
    assert_eq!(c.name, "Adyen Checkout API");
    assert_eq!((report.mapped.operations, report.mapped.requests, report.mapped.folders), (28, 28, 7));
    assert_eq!(report.mapped.examples, 143);
    assert_eq!(c.variables["baseUrl"], "https://checkout-test.adyen.com/v71");
    assert!(report.skipped.is_empty(), "{}", text(&report.skipped));

    let payments = req(&c, |r| op(r) == "POST /payments post-payments");
    assert_eq!(payments.url, "${baseUrl}/payments");
    assert_eq!(payments.auth, Some(Auth::Basic { username: "${BasicAuthUsername}".into(), password: "${BasicAuthPassword}".into() }));
    let body: Value = serde_json::from_str(&payments.body).unwrap();
    assert_eq!(body["merchantAccount"], "YOUR_MERCHANT_ACCOUNT", "the first request example");
    let idem = payments.headers.iter().find(|h| h.name == "Idempotency-Key").unwrap();
    assert!(!idem.enabled, "an optional header is kept, off");
    assert!(payments.examples.iter().all(|e| e["source"] == "openapi" && e["status"].is_u64()));
    let link = req(&c, |r| op(r) == "GET /paymentLinks/{linkId} get-paymentLinks-linkId");
    assert_eq!(link.url, "${baseUrl}/paymentLinks/${linkId}");
    round_trip(&c, &spec);
}

#[test]
fn kubernetes_apps_v1_3_0() {
    let spec = fixture("kubernetes-apps/apis__apps__v1_openapi.json");
    let started = std::time::Instant::now();
    let (c, report) = import(&spec, &Options::default()).unwrap();
    assert!(started.elapsed().as_secs() < 20, "{:?}", started.elapsed());
    assert_eq!(report.mapped.openapi, "3.0.0");
    assert_eq!((report.mapped.operations, report.mapped.requests, report.mapped.folders), (77, 77, 1));
    assert_eq!(report.mapped.variables, ["baseUrl", "name", "namespace"]);
    assert_eq!(c.variables["baseUrl"], "", "no servers");
    assert!(text(&report.warnings).contains("no servers"));

    let create = req(&c, |r| op(r) == "POST /apis/apps/v1/namespaces/{namespace}/deployments createAppsV1NamespacedDeployment");
    assert_eq!(create.url, "${baseUrl}/apis/apps/v1/namespaces/${namespace}/deployments");
    // `*/*` with a schema: a JSON body built from io.k8s.api.apps.v1.Deployment.
    let body: Value = serde_json::from_str(&create.body).unwrap();
    assert!(body["spec"]["template"]["spec"]["containers"].is_array(), "{}", &create.body[..300.min(create.body.len())]);
    assert!(body.get("status").is_none() || body["status"].is_object());
    let delete = req(&c, |r| op(r) == "DELETE /apis/apps/v1/namespaces/{namespace}/deployments/{name} deleteAppsV1NamespacedDeployment");
    assert!(delete.body.is_empty(), "optional DELETE body left out");
    let list = req(&c, |r| op(r) == "GET /apis/apps/v1/deployments listAppsV1DeploymentForAllNamespaces");
    assert!(list.disabled_params.iter().any(|p| p.key == "labelSelector" && p.value.is_empty()), "nothing made up for parameters");
    round_trip(&c, &spec);
}
