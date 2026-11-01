use std::collections::HashMap;
use anyhow::Result;
use serde_json::Value;

use crate::{
    http,
    model::{Collection, Environment, RequestResult, TestResult},
    prepare::{prepare, variables, Draft},
    scripting::run_post_request_script,
};

#[derive(Debug)]
pub struct RunStepResult {
    pub request_name: String,
    pub method: String,
    pub url: String,
    pub result: Result<RequestResult>,
    /// The post-request script's `assert.*` results.
    pub tests: Vec<TestResult>,
}

impl RunStepResult {
    /// Sent, answered below 400, and every assertion passed.
    pub fn passed(&self) -> bool {
        self.result.as_ref().map(|r| r.status < 400).unwrap_or(false) && self.tests.iter().all(|t| t.pass)
    }
}

/// Runs the requests in order, passing script context from one to the next.
/// `environment` overrides collection variables.
pub fn run_collection(
    collection: &Collection,
    environment: Option<&Environment>,
    stop_on_fail: bool,
    on_step: &mut dyn FnMut(&RunStepResult),
) -> Vec<RunStepResult> {
    let mut context: HashMap<String, Value> = HashMap::new();
    let variables = variables(&collection.variables, environment);
    let mut results = Vec::new();

    for request in &collection.requests {
        let draft = Draft {
            method: &request.method,
            url: &request.url,
            headers: &request.headers,
            body: &request.body,
            pre_request_script: &request.pre_request_script,
            // A request's own auth (even `none`) wins over the collection's.
            auth: request.auth.as_ref().or(collection.auth.as_ref()),
        };
        let step = match prepare(&draft, &variables, &context) {
            Err(e) => RunStepResult {
                request_name: request.name.clone(),
                method: request.method.clone(),
                url: request.url.clone(),
                result: Err(e),
                tests: vec![],
            },
            Ok(pre) => {
                context.extend(pre.context_updates.clone());
                let result = http::execute(&request.method, &pre.url, &pre.headers, &pre.body);
                let mut tests = vec![];
                if let Ok(resp) = &result {
                    match run_post_request_script(
                        &request.post_request_script,
                        &resp.body,
                        &resp.headers,
                        resp.status,
                        resp.duration_ms,
                        &variables,
                        &context,
                    ) {
                        Ok(post) => {
                            context.extend(post.context_updates);
                            tests = post.tests;
                        }
                        Err(e) => tests.push(TestResult { name: "post-request script".into(), pass: false, detail: e.to_string() }),
                    }
                }
                RunStepResult { request_name: request.name.clone(), method: request.method.clone(), url: pre.url, result, tests }
            }
        };
        let failed = !step.passed();
        on_step(&step);
        results.push(step);
        if failed && stop_on_fail {
            break;
        }
    }

    results
}
