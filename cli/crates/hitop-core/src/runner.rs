use std::collections::HashMap;
use anyhow::Result;
use serde_json::Value;

use crate::{
    http,
    interpolation::interpolate,
    model::{Collection, RequestResult},
    scripting::{run_post_request_script, run_pre_request_script, PreRequestCtx},
};

#[derive(Debug)]
pub struct RunStepResult {
    pub request_name: String,
    pub method: String,
    pub url: String,
    pub result: Result<RequestResult>,
}

pub fn run_collection(
    collection: &Collection,
    stop_on_fail: bool,
    on_step: &mut dyn FnMut(&RunStepResult),
) -> Vec<RunStepResult> {
    let mut context: HashMap<String, Value> = HashMap::new();
    let variables = &collection.variables;
    let mut results = Vec::new();

    for request in &collection.requests {
        // Build flat header pairs
        let raw_headers: Vec<(String, String)> = request
            .headers
            .iter()
            .filter(|h| !h.name.is_empty())
            .map(|h| (h.name.clone(), h.value.clone()))
            .collect();

        // Interpolate
        let url = match interpolate(&request.url, variables) {
            Ok(u) => u,
            Err(e) => {
                let step = RunStepResult {
                    request_name: request.name.clone(),
                    method: request.method.clone(),
                    url: request.url.clone(),
                    result: Err(e),
                };
                on_step(&step);
                results.push(step);
                if stop_on_fail {
                    break;
                }
                continue;
            }
        };

        let headers: Vec<(String, String)> = raw_headers
            .iter()
            .filter_map(|(k, v)| {
                Some((
                    interpolate(k, variables).ok()?,
                    interpolate(v, variables).ok()?,
                ))
            })
            .collect();

        let body = interpolate(&request.body, variables).unwrap_or_else(|_| request.body.clone());

        // Pre-request script
        let pre_ctx = PreRequestCtx {
            url,
            method: request.method.clone(),
            headers,
            body,
            variables: variables.clone(),
            context: context.clone(),
        };

        let pre_result = match run_pre_request_script(&request.pre_request_script, pre_ctx) {
            Ok(r) => r,
            Err(e) => {
                let step = RunStepResult {
                    request_name: request.name.clone(),
                    method: request.method.clone(),
                    url: request.url.clone(),
                    result: Err(e),
                };
                on_step(&step);
                results.push(step);
                if stop_on_fail {
                    break;
                }
                continue;
            }
        };

        // Apply pre-script context updates
        for (k, v) in &pre_result.context_updates {
            context.insert(k.clone(), v.clone());
        }

        // Execute HTTP request
        let http_result = http::execute(
            &request.method,
            &pre_result.url,
            &pre_result.headers,
            &pre_result.body,
        );

        let step_result = match http_result {
            Ok(ref resp) => {
                // Post-request script
                if let Err(e) = run_post_request_script(
                    &request.post_request_script,
                    &resp.body,
                    &resp.headers,
                    resp.status,
                    variables,
                    &context,
                )
                .map(|r| {
                    for (k, v) in r.context_updates {
                        context.insert(k, v);
                    }
                }) {
                    eprintln!("post-request script error (non-fatal): {}", e);
                }

                RunStepResult {
                    request_name: request.name.clone(),
                    method: request.method.clone(),
                    url: pre_result.url.clone(),
                    result: http_result,
                }
            }
            Err(_) => RunStepResult {
                request_name: request.name.clone(),
                method: request.method.clone(),
                url: pre_result.url.clone(),
                result: http_result,
            },
        };

        let failed = step_result.result.is_err()
            || step_result
                .result
                .as_ref()
                .map(|r| r.status >= 400)
                .unwrap_or(false);

        on_step(&step_result);
        results.push(step_result);

        if failed && stop_on_fail {
            break;
        }
    }

    results
}
