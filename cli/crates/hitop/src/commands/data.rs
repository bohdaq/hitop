//! `hitop run --data <csv|json>`: one iteration per row, the row's values as
//! variables.
use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::Value;

pub type Row = BTreeMap<String, String>;

/// Reads a CSV file (header row, then one row per iteration) or a JSON
/// array of objects. `.csv` and `.json` pick the parser; other names are
/// JSON when the file starts with `[`.
pub fn read(path: &Path) -> Result<Vec<Row>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading --data {}", path.display()))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    let json = match ext.as_deref() {
        Some("json") => true,
        Some("csv") => false,
        _ => text.trim_start().starts_with('['),
    };
    let rows = if json { from_json(text) } else { from_csv(text) }.with_context(|| format!("--data {}", path.display()))?;
    if rows.is_empty() {
        bail!("--data {} has no rows", path.display());
    }
    Ok(rows)
}

fn from_json(text: &str) -> Result<Vec<Row>> {
    let Value::Array(items) = serde_json::from_str(text)? else { bail!("expected a JSON array of objects") };
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| {
            let Value::Object(map) = item else { bail!("item {} is not an object", i + 1) };
            Ok(map
                .into_iter()
                .map(|(k, v)| {
                    let v = match v {
                        Value::String(s) => s,
                        Value::Null => String::new(),
                        other => other.to_string(),
                    };
                    (k, v)
                })
                .collect())
        })
        .collect()
}

/// RFC 4180: comma separated, `"` quotes fields that hold commas, quotes or
/// line breaks, `""` is a quote. Blank lines are skipped.
fn from_csv(text: &str) -> Result<Vec<Row>> {
    let records = csv_records(text)?;
    let mut it = records.into_iter();
    let Some((_, header)) = it.next() else { return Ok(vec![]) };
    if header.iter().any(|h| h.is_empty()) {
        bail!("line 1: every column needs a name");
    }
    it.map(|(line, fields)| {
        if fields.len() != header.len() {
            bail!("line {line}: {} values for {} columns", fields.len(), header.len());
        }
        Ok(header.iter().cloned().zip(fields).collect())
    })
    .collect()
}

fn csv_records(text: &str) -> Result<Vec<(usize, Vec<String>)>> {
    let mut records = vec![];
    let mut fields: Vec<String> = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut was_quoted = false;
    let mut line = 1;
    let mut start = 1;
    let mut chars = text.chars().peekable();
    let end_record = |fields: &mut Vec<String>, field: &mut String, was_quoted: bool, records: &mut Vec<(usize, Vec<String>)>, start: usize| {
        fields.push(std::mem::take(field));
        let record = std::mem::take(fields);
        let blank = record.len() == 1 && record[0].is_empty() && !was_quoted;
        if !blank {
            records.push((start, record));
        }
    };
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => quoted = false,
                '\n' => {
                    line += 1;
                    field.push(c);
                }
                _ => field.push(c),
            }
            continue;
        }
        match c {
            '"' if field.is_empty() && !was_quoted => {
                quoted = true;
                was_quoted = true;
            }
            '"' => bail!("line {line}: a quote inside an unquoted value (quote the value and double the quote)"),
            ',' => {
                fields.push(std::mem::take(&mut field));
                was_quoted = false;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' => {
                end_record(&mut fields, &mut field, was_quoted, &mut records, start);
                was_quoted = false;
                line += 1;
                start = line;
            }
            _ if was_quoted => bail!("line {line}: text after a closing quote"),
            _ => field.push(c),
        }
    }
    if quoted {
        bail!("line {start}: a quoted value is not closed");
    }
    end_record(&mut fields, &mut field, was_quoted, &mut records, start);
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pairs: &[(&str, &str)]) -> Row {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn csv_with_quotes_line_breaks_and_crlf() {
        let rows = from_csv("user,note\r\nada,\"a, \"\"b\"\"\"\r\n\r\nbob,\"two\nlines\"\n").unwrap();
        assert_eq!(rows, vec![row(&[("user", "ada"), ("note", "a, \"b\"")]), row(&[("user", "bob"), ("note", "two\nlines")])]);
    }

    #[test]
    fn csv_errors_name_the_line() {
        assert!(from_csv("a,b\n1\n").unwrap_err().to_string().contains("line 2: 1 values for 2 columns"));
        assert!(from_csv("a\n\"x\n").unwrap_err().to_string().contains("not closed"));
        assert!(from_csv("a,\n1,2\n").unwrap_err().to_string().contains("needs a name"));
        assert!(from_csv("a\nx\"y\n").unwrap_err().to_string().contains("line 2"));
    }

    #[test]
    fn json_values_become_strings() {
        let rows = from_json(r#"[{"id": 1, "ok": true, "name": "ada", "none": null, "tags": ["x"]}]"#).unwrap();
        assert_eq!(rows, vec![row(&[("id", "1"), ("ok", "true"), ("name", "ada"), ("none", ""), ("tags", "[\"x\"]")])]);
        assert!(from_json(r#"{"a": 1}"#).is_err());
        assert!(from_json(r#"[1]"#).unwrap_err().to_string().contains("item 1"));
    }
}
