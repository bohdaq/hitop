#![cfg(feature = "native")]
//! `hitop run --reporter html`: a golden page from a fixed report (the
//! page is a pure function of the report, so no clock to fix), and
//! escaping of every string field.
//!
//! Regenerate after an intended change with
//! `UPDATE_GOLDEN=1 cargo test -p hitop-core --test report_html`, then
//! review the diff (open the page in a browser, light and dark).
use std::fs;
use std::path::{Path, PathBuf};

use hitop_core::report::Report;

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/report")
}

fn fixture() -> Report {
    serde_json::from_str(&fs::read_to_string(golden().join("report.json")).unwrap()).unwrap()
}

#[test]
fn html_report_matches_the_golden_page() {
    let page = fixture().to_html();
    let path = golden().join("report.html");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::write(&path, &page).unwrap();
    }
    let expected = fs::read_to_string(&path).unwrap();
    assert!(page == expected, "report.html differs from the golden page; run with UPDATE_GOLDEN=1 and review the diff");
    // Same report, same bytes.
    assert_eq!(page, fixture().to_html());
}

#[test]
fn html_report_is_self_contained() {
    let page = fixture().to_html();
    assert!(page.starts_with("<!DOCTYPE html>\n"));
    assert!(page.contains("default-src 'none'"), "a CSP that loads nothing");
    assert!(page.contains("prefers-color-scheme:dark"));
    for needle in ["<script", "<link", "<img", "src=", "href=", "url(", "@import", "http-equiv=\"refresh\""] {
        assert!(!page.contains(needle), "page has {needle}");
    }
    // What the report holds is on the page.
    for needle in [
        "Shop API",
        "staging",
        "shop-api/requests/orders/create-order.yaml",
        "status is 201: <span class=\"failure\">got 200</span>",
        "HTTP 500",
        "error sending request: operation timed out",
        "<th>user</th>",
        "<td>bob</td>",
        "--bail",
        "3/6",
        "1.234 s",
        "key=••••",
    ] {
        assert!(page.contains(needle), "page lacks {needle}");
    }
}

const HOSTILE: &str = "\"'><img src=x onerror=alert(1)><script>alert(document.cookie)</script><!--&amp;\u{0}";

/// Tags the page is built from: anything else came from report data.
const TAGS: &[&str] = &[
    "!DOCTYPE", "html", "head", "meta", "title", "style", "body", "header", "h1", "h2", "h3", "span", "p", "time", "div", "b",
    "label", "input", "main", "section", "code", "table", "tr", "th", "td", "details", "summary", "i", "dl", "dt", "dd", "ul", "li",
    "footer",
];

#[test]
fn every_string_in_the_report_is_escaped() {
    let mut v: serde_json::Value = serde_json::from_str(&fs::read_to_string(golden().join("report.json")).unwrap()).unwrap();
    // The hostile text in every string field...
    fn poison(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::String(s) => *s = format!("{s}{HOSTILE}"),
            serde_json::Value::Array(a) => a.iter_mut().for_each(poison),
            serde_json::Value::Object(o) => {
                let keys: Vec<String> = o.keys().cloned().collect();
                for k in keys {
                    poison(o.get_mut(&k).unwrap());
                }
            }
            _ => {}
        }
    }
    poison(&mut v);
    // ...and in every data key (the only free-form keys).
    for c in v["collections"].as_array_mut().unwrap() {
        for i in c["iterations"].as_array_mut().unwrap() {
            if let Some(row) = i["data"].as_object_mut() {
                let row2: serde_json::Map<String, serde_json::Value> = row.iter().map(|(k, v)| (format!("{k}{HOSTILE}"), v.clone())).collect();
                *row = row2;
            }
        }
    }
    let report: Report = serde_json::from_value(v).unwrap();
    assert!(report.collections[1].iterations[0].data.as_ref().unwrap().keys().all(|k| k.contains("<script>")));
    let page = report.to_html();

    assert!(!page.contains(HOSTILE));
    for raw in ["<img", "<script", "<!--", "onerror=alert(1)>", "\"'>", "\u{0}"] {
        assert!(!page.contains(raw), "raw {raw:?} on the page");
    }
    assert!(page.contains("&quot;&#39;&gt;&lt;img src=x onerror=alert(1)&gt;&lt;script&gt;alert(document.cookie)&lt;/script&gt;&lt;!--&amp;amp;\u{FFFD}"));
    // Every tag is one the page writes.
    let mut rest = page.as_str();
    while let Some(at) = rest.find('<') {
        rest = &rest[at + 1..];
        let name: String = rest.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '!').collect();
        assert!(TAGS.contains(&name.as_str()), "unexpected tag <{name}: {}", &rest[..rest.len().min(80)]);
    }
    // Attribute values: every double quote on the page opens or closes one
    // of the page's own attributes (an even count per tag).
    for tag in page.split('<').skip(1) {
        let tag = tag.split('>').next().unwrap();
        assert_eq!(tag.matches('"').count() % 2, 0, "odd quotes in <{tag}>");
    }
}
