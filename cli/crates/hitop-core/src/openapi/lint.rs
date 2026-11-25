//! Checks an OpenAPI 3.x document: what breaks the specification (errors) and what makes it hard to
//! use (warnings). Pure, like the rest of `openapi`: every surface shows the same problems.
use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::refs::{RefError, Resolver};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// One problem. `rule` is a stable name (`off` takes it), `location` a JSON pointer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    pub rule: String,
    pub severity: Severity,
    pub location: String,
    /// 1-based line of the location, where it could be found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lint {
    pub problems: Vec<Problem>,
    pub errors: usize,
    pub warnings: usize,
}

/// Every rule with its severity.
pub const RULES: &[(&str, Severity)] = &[
    ("syntax", Severity::Error),
    ("version", Severity::Error),
    ("info", Severity::Error),
    ("structure", Severity::Error),
    ("path-format", Severity::Error),
    ("path-duplicate", Severity::Error),
    ("path-parameter", Severity::Error),
    ("parameter", Severity::Error),
    ("parameter-duplicate", Severity::Error),
    ("operation-id-duplicate", Severity::Error),
    ("responses", Severity::Error),
    ("response-description", Severity::Error),
    ("ref", Severity::Error),
    ("ref-external", Severity::Warning),
    ("security-scheme", Severity::Error),
    ("server-variable", Severity::Error),
    ("array-items", Severity::Error),
    ("required-property", Severity::Warning),
    ("get-body", Severity::Warning),
    ("operation-id", Severity::Warning),
    ("summary", Severity::Warning),
    ("tag", Severity::Warning),
    ("unused-component", Severity::Warning),
];

const METHODS: [&str; 8] = ["get", "put", "post", "delete", "options", "head", "patch", "trace"];
const MAX_DEPTH: usize = 200;

fn esc(k: &str) -> String {
    k.replace('~', "~0").replace('/', "~1")
}

struct Linter<'a> {
    root: &'a Value,
    refs: Resolver<'a>,
    v30: bool,
    off: &'a [String],
    out: Vec<Problem>,
    used: BTreeSet<String>,
}

/// Checks `text` (YAML or JSON), leaving out the rules named in `off`.
pub fn lint(text: &str, off: &[String]) -> Lint {
    let root = match super::parse(text) {
        Ok(v) => v,
        Err(e) => return finish(text, vec![problem("syntax", "", format!("{e:#}"))], off),
    };
    if let Err(e) = super::refs::check_version(&root) {
        return finish(text, vec![problem("version", "", format!("{e:#}"))], off);
    }
    let v30 = root["openapi"].as_str().is_some_and(|v| v.starts_with("3.0")) || root["openapi"].as_f64() == Some(3.0);
    let mut l = Linter { root: &root, refs: Resolver::new(&root), v30, off, out: vec![], used: BTreeSet::new() };
    l.run();
    finish(text, l.out, off)
}

fn problem(rule: &str, location: &str, message: String) -> Problem {
    let severity = RULES.iter().find(|(r, _)| *r == rule).map_or(Severity::Error, |(_, s)| *s);
    Problem { rule: rule.into(), severity, location: location.into(), line: None, message }
}

fn finish(text: &str, mut problems: Vec<Problem>, off: &[String]) -> Lint {
    problems.retain(|p| !off.contains(&p.rule));
    if problems.iter().any(|p| !p.location.is_empty()) {
        let lines = super::lines::index(text);
        for p in &mut problems {
            p.line = super::lines::find(&lines, &p.location);
        }
    }
    // Errors first, then in document order (no line: last), then by pointer.
    problems.sort_by(|a, b| (a.severity, a.line.unwrap_or(usize::MAX), &a.location, &a.rule).cmp(&(b.severity, b.line.unwrap_or(usize::MAX), &b.location, &b.rule)));
    problems.dedup();
    let errors = problems.iter().filter(|p| p.severity == Severity::Error).count();
    Lint { errors, warnings: problems.len() - errors, problems }
}

impl<'a> Linter<'a> {
    fn add(&mut self, rule: &str, at: &str, message: impl Into<String>) {
        if !self.off.iter().any(|o| o == rule) {
            self.out.push(problem(rule, at, message.into()));
        }
    }

    /// What `v` refers to; a broken ref is reported by the walk over every ref.
    fn get(&self, v: &'a Value) -> Option<&'a Value> {
        self.refs.get(v).ok()
    }

    fn run(&mut self) {
        let root = self.root;
        match root["info"].as_object() {
            None => self.add("info", "", "`info` is missing"),
            Some(i) => {
                for k in ["title", "version"] {
                    match i.get(k) {
                        None | Some(Value::Null) => self.add("info", "/info", format!("`info.{k}` is missing")),
                        Some(Value::String(s)) if !s.trim().is_empty() => {}
                        Some(Value::String(_)) => self.add("info", &format!("/info/{k}"), format!("`info.{k}` is empty")),
                        Some(_) => self.add("info", &format!("/info/{k}"), format!("`info.{k}` must be text (quote it)")),
                    }
                }
            }
        }
        if root.get("paths").is_none() && (self.v30 || (root.get("webhooks").is_none() && root.get("components").is_none())) {
            self.add("structure", "", "`paths` is missing");
        }
        self.servers(root, "");
        self.security(root.get("security"), "/security");
        self.paths();
        self.walk(root, "", 0);
        self.unused();
    }

    fn object(&mut self, v: &'a Value, at: &str, what: &str) -> Option<&'a Map<String, Value>> {
        let o = v.as_object();
        if o.is_none() {
            self.add("structure", at, format!("{what} must be an object"));
        }
        o
    }

    fn servers(&mut self, holder: &'a Value, at: &str) {
        let Some(list) = holder.get("servers").and_then(Value::as_array) else { return };
        for (i, s) in list.iter().enumerate() {
            let at = format!("{at}/servers/{i}");
            let Some(url) = s["url"].as_str() else {
                self.add("structure", &at, "the server has no `url`");
                continue;
            };
            for name in braces(url) {
                match s["variables"].get(&name) {
                    None => self.add("server-variable", &at, format!("the server URL uses {{{name}}} but `variables` doesn't define it")),
                    Some(v) if v.get("default").is_none() => self.add("server-variable", &format!("{at}/variables/{}", esc(&name)), format!("server variable {name} has no `default`")),
                    Some(_) => {}
                }
            }
        }
    }

    fn security(&mut self, v: Option<&'a Value>, at: &str) {
        for (i, req) in v.and_then(Value::as_array).into_iter().flatten().enumerate() {
            for name in req.as_object().into_iter().flat_map(|o| o.keys()) {
                if self.root["components"]["securitySchemes"].get(name).is_none() {
                    self.add("security-scheme", &format!("{at}/{i}"), format!("security scheme {name} is not in components.securitySchemes"));
                } else {
                    self.used.insert(format!("/components/securitySchemes/{}", esc(name)));
                }
            }
        }
    }

    fn paths(&mut self) {
        let Some(v) = self.root.get("paths") else { return };
        let Some(paths) = self.object(v, "/paths", "`paths`") else { return };
        let tags: Option<BTreeSet<&str>> = self.root["tags"].as_array().map(|t| t.iter().filter_map(|t| t["name"].as_str()).collect());
        let mut ids: HashMap<&'a str, String> = HashMap::new();
        let mut shapes: BTreeMap<String, &str> = BTreeMap::new();
        for (path, item) in paths {
            if path.starts_with("x-") {
                continue;
            }
            let at = format!("/paths/{}", esc(path));
            if !path.starts_with('/') {
                self.add("path-format", &at, "a path must start with /");
            }
            let names = braces(path);
            let mut shape = path.clone();
            for n in &names {
                shape = shape.replace(&format!("{{{n}}}"), "{}");
            }
            if let Some(other) = shapes.insert(shape, path) {
                self.add("path-duplicate", &at, format!("the same path as {other} (only the parameter names differ)"));
            }
            let Some(item) = self.get(item) else { continue };
            let Some(item) = self.object(item, &at, "the path") else { continue };
            let shared = self.parameters(item.get("parameters"), &format!("{at}/parameters"));
            for method in METHODS {
                let Some(op) = item.get(method) else { continue };
                let at = format!("{at}/{method}");
                let Some(o) = self.object(op, &at, "the operation") else { continue };
                let label = format!("{} {path}", method.to_uppercase());
                match o.get("operationId").and_then(Value::as_str) {
                    None => self.add("operation-id", &at, format!("{label} has no `operationId`")),
                    Some(id) => {
                        if let Some(first) = ids.insert(id, label.clone()) {
                            self.add("operation-id-duplicate", &format!("{at}/operationId"), format!("operationId {id} is also used by {first}"));
                        }
                    }
                }
                let has = |k: &str| o.get(k).and_then(Value::as_str).is_some_and(|s| !s.trim().is_empty());
                if !has("summary") && !has("description") {
                    self.add("summary", &at, format!("{label} has no `summary` or `description`"));
                }
                if let Some(tags) = &tags {
                    for (i, t) in o.get("tags").and_then(Value::as_array).into_iter().flatten().enumerate() {
                        if let Some(t) = t.as_str().filter(|t| !tags.contains(t)) {
                            self.add("tag", &format!("{at}/tags/{i}"), format!("tag {t} is not in the top-level `tags`"));
                        }
                    }
                }
                let own = self.parameters(o.get("parameters"), &format!("{at}/parameters"));
                let declared: BTreeSet<&str> = shared.iter().chain(&own).copied().collect();
                for n in &names {
                    if !declared.contains(n.as_str()) {
                        self.add("path-parameter", &at, format!("{label}: path parameter {n} is not declared"));
                    }
                }
                for n in declared.iter().filter(|n| !names.iter().any(|x| x == *n)) {
                    self.add("path-parameter", &at, format!("{label}: parameter {n} is `in: path` but the path has no {{{n}}}"));
                }
                if o.contains_key("requestBody") && matches!(method, "get" | "head" | "delete") {
                    self.add("get-body", &format!("{at}/requestBody"), format!("{label} has a request body, which many servers and proxies ignore"));
                }
                self.security(o.get("security"), &format!("{at}/security"));
                self.servers(op, &at);
                self.responses(o.get("responses"), &at, &label);
            }
        }
    }

    /// Checks a parameter list; answers the names that are `in: path`.
    fn parameters(&mut self, v: Option<&'a Value>, at: &str) -> Vec<&'a str> {
        let mut path = vec![];
        let mut seen = BTreeSet::new();
        for (i, p) in v.and_then(Value::as_array).into_iter().flatten().enumerate() {
            let at = format!("{at}/{i}");
            let Some(p) = self.get(p) else { continue };
            let (Some(name), Some(place)) = (p["name"].as_str(), p["in"].as_str()) else {
                self.add("parameter", &at, "a parameter needs `name` and `in`");
                continue;
            };
            if !["path", "query", "header", "cookie"].contains(&place) {
                self.add("parameter", &at, format!("parameter {name}: `in` is path, query, header or cookie, not {place}"));
            }
            // Header names don't depend on case.
            let key = if place == "header" { name.to_lowercase() } else { name.to_string() };
            if !seen.insert((place, key)) {
                self.add("parameter-duplicate", &at, format!("parameter {name} (in {place}) is listed twice"));
            }
            if p.get("schema").is_none() && p.get("content").is_none() {
                self.add("parameter", &at, format!("parameter {name} has no `schema` or `content`"));
            }
            if place == "path" {
                path.push(name);
                if p["required"] != Value::Bool(true) {
                    self.add("path-parameter", &at, format!("path parameter {name} must have `required: true`"));
                }
            }
        }
        path
    }

    fn responses(&mut self, v: Option<&'a Value>, op: &str, label: &str) {
        let Some(v) = v else {
            // Optional since 3.1.
            if self.v30 {
                self.add("responses", op, format!("{label} has no `responses`"));
            }
            return;
        };
        let at = format!("{op}/responses");
        let Some(list) = self.object(v, &at, "`responses`") else { return };
        if list.keys().all(|k| k.starts_with("x-")) {
            self.add("responses", &at, format!("{label} has no response"));
        }
        for (code, r) in list {
            if code.starts_with("x-") {
                continue;
            }
            let at = format!("{at}/{}", esc(code));
            let b = code.as_bytes();
            let ok = code == "default" || (b.len() == 3 && (b'1'..=b'5').contains(&b[0]) && (b[1..].iter().all(u8::is_ascii_digit) || &b[1..] == b"XX"));
            if !ok {
                self.add("responses", &at, format!("{code} is not a status code (200, 4XX, default)"));
            }
            if let Some(r) = self.get(r) {
                if r["description"].as_str().is_none() {
                    self.add("response-description", &at, format!("{label}: response {code} has no `description`"));
                }
            }
        }
    }

    /// Every `$ref` and schema in the document.
    fn walk(&mut self, v: &'a Value, at: &str, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        match v {
            Value::Array(a) => a.iter().enumerate().for_each(|(i, x)| self.walk(x, &format!("{at}/{i}"), depth + 1)),
            Value::Object(o) => {
                let last = at.rsplit('/').next().unwrap_or("");
                let parent = at.rsplit('/').nth(1).unwrap_or("");
                // A map of names: its keys are names, not keywords.
                let names = matches!(last, "properties" | "patternProperties" | "$defs" | "schemas" | "parameters" | "responses" | "requestBodies" | "headers" | "securitySchemes" | "links" | "callbacks" | "pathItems" | "examples" | "dependentSchemas")
                    && !(matches!(parent, "properties" | "patternProperties" | "$defs" | "schemas"));
                if !names {
                    if o.get("$ref").is_some_and(Value::is_string) {
                        match self.refs.get(v) {
                            Err(e @ RefError::External(_)) => self.add("ref-external", at, e.to_string()),
                            Err(e) => self.add("ref", at, e.to_string()),
                            Ok(_) => {}
                        }
                    }
                    self.schema(o, at);
                }
                for (k, x) in o {
                    // Data and extensions, not parts of the document.
                    if !names && (matches!(k.as_str(), "example" | "default" | "enum" | "const") || k.starts_with("x-")) {
                        continue;
                    }
                    if !names && k == "examples" && x.is_array() {
                        continue;
                    }
                    if k == "value" && parent == "examples" {
                        continue;
                    }
                    // A discriminator names its schemas as refs or bare names.
                    if !names && k == "discriminator" {
                        for t in x["mapping"].as_object().into_iter().flat_map(|m| m.values()).filter_map(Value::as_str) {
                            self.used.insert(t.strip_prefix('#').map_or_else(|| format!("/components/schemas/{t}"), str::to_string));
                        }
                    }
                    self.walk(x, &format!("{at}/{}", esc(k)), depth + 1);
                }
            }
            _ => {}
        }
    }

    fn schema(&mut self, o: &'a Map<String, Value>, at: &str) {
        if self.v30 && o.get("type").and_then(Value::as_str) == Some("array") && !o.contains_key("items") {
            self.add("array-items", at, "an array schema needs `items`");
        }
        // Only where the properties are all here: an `allOf` may bring others.
        if let (Some(req), Some(props)) = (o.get("required").and_then(Value::as_array), o.get("properties").and_then(Value::as_object)) {
            if !["allOf", "anyOf", "oneOf", "$ref"].iter().any(|k| o.contains_key(*k)) && !at.contains("/allOf/") {
                for r in req.iter().filter_map(Value::as_str).filter(|r| !props.contains_key(*r)) {
                    self.add("required-property", at, format!("`required` names {r}, which is not in `properties`"));
                }
            }
        }
    }

    fn unused(&mut self) {
        let Some(c) = self.root["components"].as_object() else { return };
        // Anywhere, extensions (`x-anyOf`) too: a ref there is checked by nobody but is a use.
        uses(self.root, &mut self.used, 0);
        for (kind, list) in c.iter().filter(|(k, _)| !k.starts_with("x-") && *k != "pathItems") {
            for name in list.as_object().into_iter().flat_map(|o| o.keys()) {
                let at = format!("/components/{}/{}", esc(kind), esc(name));
                let inside = format!("{at}/");
                if !self.used.iter().any(|u| *u == at || u.starts_with(&inside)) {
                    self.add("unused-component", &at, format!("components.{kind}.{name} is not used"));
                }
            }
        }
    }
}

fn uses(v: &Value, out: &mut BTreeSet<String>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    match v {
        Value::Array(a) => a.iter().for_each(|x| uses(x, out, depth + 1)),
        Value::Object(o) => {
            if let Some(p) = o.get("$ref").and_then(Value::as_str).and_then(|r| r.strip_prefix('#')) {
                out.insert(p.to_string());
            }
            o.values().for_each(|x| uses(x, out, depth + 1));
        }
        _ => {}
    }
}

/// The names in `{}` of a path or server URL.
fn braces(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut rest = s;
    while let Some((_, after)) = rest.split_once('{') {
        let Some((name, tail)) = after.split_once('}') else { break };
        if !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
        rest = tail;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(text: &str) -> Vec<(String, Option<usize>)> {
        lint(text, &[]).problems.into_iter().map(|p| (p.rule, p.line)).collect()
    }

    const GOOD: &str = "openapi: 3.0.3\ninfo:\n  title: Shop\n  version: '1'\npaths:\n  /a/{id}:\n    get:\n      operationId: a\n      summary: A\n      parameters:\n        - {name: id, in: path, required: true, schema: {type: string}}\n      responses:\n        200:\n          description: ok\n";

    #[test]
    fn a_good_spec_has_no_problems() {
        assert_eq!(rules(GOOD), vec![]);
    }

    #[test]
    fn unreadable_and_old_specs_are_one_problem() {
        assert_eq!(rules("a: [")[0].0, "syntax");
        assert_eq!(rules("swagger: '2.0'")[0].0, "version");
        assert_eq!(rules("[]")[0].0, "version");
    }

    /// Each rule, from a change to the good spec, with the line it names.
    #[test]
    fn each_rule_fires_where_the_problem_is() {
        let check = |from: &str, to: &str, rule: &str, line: usize| {
            assert!(GOOD.contains(from), "{from}");
            assert_eq!(rules(&GOOD.replacen(from, to, 1)), vec![(rule.to_string(), Some(line))], "{rule}: {to}");
        };
        check("  version: '1'", "  version: 1", "info", 4);
        check("required: true, ", "", "path-parameter", 11);
        check(", schema: {type: string}", "", "parameter", 11);
        check("      operationId: a\n", "", "operation-id", 7);
        check("      summary: A\n", "", "summary", 7);
        check("          description: ok", "          x-a: 1", "response-description", 13);
        check("        200:", "        '20':", "responses", 13);
        check("{type: string}", "{type: array}", "array-items", 11);
        check("{type: string}", "{$ref: '#/components/schemas/No'}", "ref", 11);
        check("{type: string}", "{$ref: 'other.yaml#/A'}", "ref-external", 11);
        check("      summary: A\n", "      summary: A\n      security: [{k: []}]\n", "security-scheme", 10);
        check("      summary: A\n", "      summary: A\n      requestBody: {content: {}}\n", "get-body", 10);
        check("paths:", "servers: [{url: 'https://{h}.x.test'}]\npaths:", "server-variable", 5);
    }

    #[test]
    fn paths_ids_tags_and_components_are_compared() {
        let more = format!("{GOOD}  /a/{{other}}:\n    get:\n      operationId: a\n      summary: B\n      tags: [x]\n      parameters:\n        - {{name: other, in: path, required: true, schema: {{type: string}}}}\n        - {{name: q, in: query, schema: {{}}}}\n        - {{name: q, in: query, schema: {{}}}}\n      responses: {{default: {{description: d}}}}\ntags: [{{name: y}}]\ncomponents:\n  schemas:\n    Unused: {{type: object, required: [a], properties: {{b: {{}}}}}}\n");
        let mut got: Vec<String> = rules(&more).into_iter().map(|r| r.0).collect();
        got.sort();
        assert_eq!(got, ["operation-id-duplicate", "parameter-duplicate", "path-duplicate", "required-property", "tag", "unused-component"]);
        let off = lint(&more, &["tag".into(), "path-duplicate".into()]);
        assert_eq!((off.errors, off.warnings), (2, 2));
    }

    #[test]
    fn names_and_data_are_not_keywords() {
        // Properties called $ref, example and required; a ref inside an example.
        let t = GOOD.replace("{type: string}", "{type: object, properties: {$ref: {type: string}, required: {type: string}}, example: {$ref: '#/nope'}}");
        assert_eq!(rules(&t), vec![]);
    }

    #[test]
    fn json_lines_are_found() {
        let t = "{\n \"openapi\": \"3.1.0\",\n \"info\": {\"title\": \"x\",\n  \"version\": 2},\n \"paths\": {\n  \"a/~b\": {}\n }\n}";
        assert_eq!(rules(t), vec![("info".into(), Some(4)), ("path-format".into(), Some(6))]);
    }

    #[test]
    fn published_specs_have_no_errors() {
        let count = |file: &str| {
            let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/openapi").join(file)).unwrap();
            let l = lint(&text, &[]);
            assert!(l.problems.iter().all(|p| p.line.is_some()));
            (l.errors, l.warnings)
        };
        assert_eq!(count("petstore/openapi.yaml"), (0, 2));
        // Uses in extensions (`x-anyOf`) count.
        assert_eq!(count("adyen-checkout/CheckoutService-v71.yaml"), (0, 0));
        assert_eq!(count("kubernetes-apps/apis__apps__v1_openapi.json"), (0, 11));
    }

    #[test]
    fn refs_in_a_circle_end() {
        let t = format!("{GOOD}components:\n  schemas:\n    Tree: {{type: object, properties: {{next: {{$ref: '#/components/schemas/Tree'}}}}}}\n    A: {{$ref: '#/components/schemas/B'}}\n    B: {{$ref: '#/components/schemas/A'}}\n");
        assert_eq!(rules(&t).iter().filter(|r| r.0 == "ref").count(), 2);
    }
}
