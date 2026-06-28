use colored::Colorize;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};

pub fn print_status(code: u16, duration_ms: u64) {
    let status_str = format!("{}", code);
    let colored_status = if code < 300 {
        status_str.green().bold()
    } else if code < 400 {
        status_str.yellow().bold()
    } else {
        status_str.red().bold()
    };
    println!("{} • {}ms", colored_status, duration_ms);
}

pub fn print_headers(headers: &std::collections::HashMap<String, String>) {
    let mut keys: Vec<&String> = headers.keys().collect();
    keys.sort();
    for k in keys {
        println!("{}: {}", k.dimmed(), headers[k]);
    }
}

pub fn print_body(body: &str) {
    if !atty::is(atty::Stream::Stdout) {
        println!("{}", body);
        return;
    }

    // Try JSON highlight
    let trimmed = body.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let pretty = serde_json::to_string_pretty(&v).unwrap_or_else(|_| body.to_string());
            print_highlighted(&pretty, "json");
            return;
        }
    }

    println!("{}", body);
}

fn print_highlighted(code: &str, lang: &str) {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ps
        .find_syntax_by_extension(lang)
        .unwrap_or_else(|| ps.find_syntax_plain_text());
    let theme = &ts.themes["base16-ocean.dark"];
    let mut h = HighlightLines::new(syntax, theme);

    for line in LinesWithEndings::from(code) {
        let ranges = h.highlight_line(line, &ps).unwrap_or_default();
        print!("{}", as_24_bit_terminal_escaped(&ranges, false));
    }
    println!("\x1b[0m");
}

pub fn print_separator() {
    println!("{}", "─".repeat(60).dimmed());
}
