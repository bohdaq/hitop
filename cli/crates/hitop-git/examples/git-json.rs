//! `git-json <cmd> <args JSON>`: runs one hitop-git command and prints
//! { "ok": … } or { "error": … }. The web app's e2e mock of the desktop
//! (webapp/tests/tauri-mock.cjs) answers the `git` Tauri command with it.
//! The token for fetch, pull and push comes from HITOP_GIT_TOKEN.

fn main() {
    let mut argv = std::env::args().skip(1);
    let cmd = argv.next().unwrap_or_default();
    let args: serde_json::Value = serde_json::from_str(&argv.next().unwrap_or_else(|| "{}".into())).unwrap_or_default();
    let auth = hitop_git::Auth { token: std::env::var("HITOP_GIT_TOKEN").ok(), username: None };
    println!("{}", hitop_git::json::answer(&cmd, &args, &auth));
}
