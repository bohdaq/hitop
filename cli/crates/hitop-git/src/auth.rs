//! Credentials for remotes. libgit2 asks again after every failed attempt,
//! so each way is tried once and then the fetch or push fails (otherwise a
//! wrong token would loop forever).

use git2::{Cred, CredentialType, RemoteCallbacks};
use serde::{Deserialize, Serialize};

/// What the caller has for the remote: an HTTPS access token (the desktop
/// reads it from the OS keychain; never kept in files) and an optional user
/// name for it (GitHub and most hosts take any name with a token).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Auth {
    pub token: Option<String>,
    pub username: Option<String>,
}

/// One way of answering libgit2's credential question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    /// The SSH user name, for an SSH URL without one.
    Username(String),
    /// A key from the SSH agent ($SSH_AUTH_SOCK, or Pageant on Windows).
    SshAgent(String),
    /// The token as the password (user name: `username`, the URL's, or "x-access-token").
    Token { username: String },
    /// git's credential helpers (credential.helper in git config). They run
    /// as programs, which the Mac App Store sandbox doesn't allow.
    Helper,
    /// The system's default (NTLM/Negotiate on Windows).
    Default,
}

/// Picks the next way to try. `tried` holds the ways already used.
pub fn choose(allowed: CredentialType, username_from_url: Option<&str>, auth: &Auth, tried: &[Method]) -> Option<Method> {
    let ssh_user = username_from_url.unwrap_or("git").to_string();
    let mut candidates = Vec::new();
    if allowed.contains(CredentialType::USERNAME) {
        candidates.push(Method::Username(ssh_user.clone()));
    }
    if allowed.contains(CredentialType::SSH_KEY) {
        candidates.push(Method::SshAgent(ssh_user));
    }
    if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
        if auth.token.as_deref().is_some_and(|t| !t.is_empty()) {
            let username = auth.username.clone().filter(|u| !u.is_empty()).or(username_from_url.map(str::to_string)).unwrap_or_else(|| "x-access-token".into());
            candidates.push(Method::Token { username });
        }
        candidates.push(Method::Helper);
    }
    if allowed.contains(CredentialType::DEFAULT) {
        candidates.push(Method::Default);
    }
    candidates.into_iter().find(|m| !tried.contains(m))
}

/// The message for a remote that turned every way down.
pub(crate) fn failed_message(url: &str, tried: &[Method]) -> String {
    let https = url.starts_with("http://") || url.starts_with("https://");
    let used: Vec<&str> = tried
        .iter()
        .map(|m| match m {
            Method::Username(_) => "user name",
            Method::SshAgent(_) => "SSH agent",
            Method::Token { .. } => "access token",
            Method::Helper => "credential helper",
            Method::Default => "system default",
        })
        .filter(|s| *s != "user name")
        .collect();
    let hint = if https {
        "Add an access token for this host in the Git panel (it is kept in your system keychain)"
    } else {
        "Load your SSH key into ssh-agent (ssh-add), or use an HTTPS remote with an access token"
    };
    let tried = if used.is_empty() { String::new() } else { format!(" (tried: {})", used.join(", ")) };
    format!("Authentication failed for {url}{tried}. {hint}.")
}

/// Callbacks that answer credential questions from `auth`, one way at a
/// time; the ways tried end up in `tried`.
pub(crate) fn callbacks<'a>(auth: &'a Auth, tried: &'a std::cell::RefCell<Vec<Method>>) -> RemoteCallbacks<'a> {
    let mut cb = RemoteCallbacks::new();
    cb.credentials(move |url, username_from_url, allowed| {
        loop {
            let Some(method) = choose(allowed, username_from_url, auth, &tried.borrow()) else {
                return Err(git2::Error::new(git2::ErrorCode::Auth, git2::ErrorClass::Callback, failed_message(url, &tried.borrow())));
            };
            tried.borrow_mut().push(method.clone());
            let cred = match &method {
                Method::Username(u) => Cred::username(u),
                Method::SshAgent(u) => Cred::ssh_key_from_agent(u),
                Method::Token { username } => Cred::userpass_plaintext(username, auth.token.as_deref().unwrap_or_default()),
                Method::Helper => match git2::Config::open_default() {
                    Ok(config) => Cred::credential_helper(&config, url, username_from_url),
                    Err(e) => Err(e),
                },
                Method::Default => Cred::default(),
            };
            // A way that can't even produce credentials (no agent, no helper): try the next one now.
            if let Ok(cred) = cred {
                return Ok(cred);
            }
        }
    });
    cb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_tries_the_token_then_the_helper_then_gives_up() {
        let auth = Auth { token: Some("t0k".into()), username: None };
        let mut tried = Vec::new();
        let allowed = CredentialType::USER_PASS_PLAINTEXT;
        while let Some(m) = choose(allowed, None, &auth, &tried) {
            tried.push(m);
        }
        assert_eq!(tried, [Method::Token { username: "x-access-token".into() }, Method::Helper]);
        let msg = failed_message("https://example.test/a.git", &tried);
        assert!(msg.contains("access token, credential helper"), "{msg}");
        assert!(!msg.contains("t0k"));
    }

    #[test]
    fn the_token_user_name_comes_from_auth_then_the_url() {
        let allowed = CredentialType::USER_PASS_PLAINTEXT;
        let auth = Auth { token: Some("t".into()), username: Some("ada".into()) };
        assert_eq!(choose(allowed, Some("bob"), &auth, &[]), Some(Method::Token { username: "ada".into() }));
        let auth = Auth { token: Some("t".into()), username: None };
        assert_eq!(choose(allowed, Some("bob"), &auth, &[]), Some(Method::Token { username: "bob".into() }));
        // No token: the helper only.
        assert_eq!(choose(allowed, None, &Auth::default(), &[]), Some(Method::Helper));
        assert_eq!(choose(allowed, None, &Auth::default(), &[Method::Helper]), None);
    }

    #[test]
    fn ssh_asks_the_agent_once() {
        let allowed = CredentialType::SSH_KEY;
        let first = choose(allowed, Some("git"), &Auth::default(), &[]);
        assert_eq!(first, Some(Method::SshAgent("git".into())));
        assert_eq!(choose(allowed, Some("git"), &Auth::default(), &[first.unwrap()]), None);
        assert_eq!(choose(CredentialType::USERNAME, None, &Auth::default(), &[]), Some(Method::Username("git".into())));
        assert!(failed_message("git@example.test:a.git", &[Method::SshAgent("git".into())]).contains("ssh-add"));
    }
}
