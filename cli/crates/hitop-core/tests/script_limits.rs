//! scripting::set_limits: a script past the loop or call-depth limit fails instead of running on
//! (the sync server's monitors run other people's scripts). Its own test binary: the limits are
//! process-wide.
use std::collections::HashMap;

use hitop_core::scripting::{run_post_request_script_with, set_limits, PostRequestCtx};

fn run(script: &str) -> anyhow::Result<()> {
    let (headers, vars, ctx) = (HashMap::new(), HashMap::new(), HashMap::new());
    run_post_request_script_with(script, &PostRequestCtx { response_body: "{}", response_headers: &headers, status_code: 200, duration_ms: 1, variables: &vars, context: &ctx, request: None, globals: None }).map(|_| ())
}

#[test]
fn loops_and_recursion_stop_at_the_limits() {
    assert!(run("let n = 0; for (let i = 0; i < 100000; i++) { n += i; }").is_ok(), "no limit by default");
    set_limits(10_000, 64);
    assert!(run("while (true) {}").is_err(), "an endless loop stops");
    assert!(run("function f() { return f(); } f();").is_err(), "endless recursion stops");
    assert!(run("let n = 0; for (let i = 0; i < 1000; i++) { n += i; } pm.test('ok', () => {});").is_ok(), "ordinary scripts still run");
}
