//! Old guide URLs: the guides moved from `/terms/<name>.html` to
//! `/guides/<name>/` (2026-11-06, "terms" read as legal terms). Every old
//! address answers with a 301 to its new one, so links and search results
//! keep working.

use rust_web_server::header::Header;
use rust_web_server::prelude::*;

/// The new address of an old guide URL, or `None` when `path` isn't one.
pub fn guide_location(path: &str) -> Option<String> {
    let rest = match path {
        "/terms" | "/terms/" => return Some("/guides/".to_string()),
        _ => path.strip_prefix("/terms/")?,
    };
    let (name, markdown) = match rest.strip_suffix(".html") {
        Some(name) => (name, false),
        None => (rest.strip_suffix(".md")?, true),
    };
    let valid = !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if !valid {
        return None;
    }
    Some(if markdown { format!("/guides/{name}.md") } else { format!("/guides/{name}/") })
}

/// `GET /terms…`: a 301 to the guide's new address, else 404.
pub fn redirect(request: &Request) -> Response {
    let path = request.request_uri.split(['?', '#']).next().unwrap_or("");
    match guide_location(path) {
        Some(location) => {
            let mut response = Response::get_response(STATUS_CODE_REASON_PHRASE.n301_moved_permanently, None, None);
            response.headers.push(Header::new("Location", location));
            response
        }
        None => Response::get_response(STATUS_CODE_REASON_PHRASE.n404_not_found, None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_old_guide_urls() {
        assert_eq!(guide_location("/terms/").as_deref(), Some("/guides/"));
        assert_eq!(guide_location("/terms").as_deref(), Some("/guides/"));
        assert_eq!(guide_location("/terms/hitop-vs-postman.html").as_deref(), Some("/guides/hitop-vs-postman/"));
        assert_eq!(guide_location("/terms/hitop-vs-postman.md").as_deref(), Some("/guides/hitop-vs-postman.md"));
    }

    #[test]
    fn refuses_anything_else() {
        assert_eq!(guide_location("/terms/x"), None);
        assert_eq!(guide_location("/terms/.html"), None);
        assert_eq!(guide_location("/terms/a/b.html"), None);
        assert_eq!(guide_location("/terms/%2e%2e.html"), None);
        assert_eq!(guide_location("/terms/https://evil.example.html"), None);
        assert_eq!(guide_location("/termsx/a.html"), None);
        assert_eq!(guide_location("/guides/a/"), None);
    }

    #[test]
    fn answers_301_with_location() {
        let request = Request { method: "GET".into(), request_uri: "/terms/collections-in-git.html?x=1".into(), http_version: "HTTP/1.1".into(), headers: vec![], body: Default::default() };
        let response = redirect(&request);
        assert_eq!(response.status_code, 301);
        let location = response.headers.iter().find(|h| h.name.eq_ignore_ascii_case("location")).map(|h| h.value.to_string());
        assert_eq!(location.as_deref(), Some("/guides/collections-in-git/"));
        let request = Request { request_uri: "/terms/../etc.html".into(), ..request };
        assert_eq!(redirect(&request).status_code, 404);
    }
}
