use std::collections::HashMap;
use anyhow::{bail, Result};
use regex::Regex;

pub fn interpolate(template: &str, variables: &HashMap<String, String>) -> Result<String> {
    let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
    let mut error: Option<String> = None;

    let result = re.replace_all(template, |caps: &regex::Captures| {
        let key = &caps[1];
        match variables.get(key) {
            Some(val) => val.clone(),
            None => {
                error = Some(format!("variable '{}' is not defined", key));
                caps[0].to_string()
            }
        }
    });

    if let Some(e) = error {
        bail!(e);
    }
    Ok(result.into_owned())
}

pub fn interpolate_headers(
    headers: &[(String, String)],
    variables: &HashMap<String, String>,
) -> Result<Vec<(String, String)>> {
    headers
        .iter()
        .map(|(k, v)| Ok((interpolate(k, variables)?, interpolate(v, variables)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn no_placeholders_returns_input_unchanged() {
        let result = interpolate("https://example.com/api", &vars(&[])).unwrap();
        assert_eq!(result, "https://example.com/api");
    }

    #[test]
    fn single_variable_replaced() {
        let result = interpolate("${host}/todos", &vars(&[("host", "https://api.example.com")])).unwrap();
        assert_eq!(result, "https://api.example.com/todos");
    }

    #[test]
    fn multiple_variables_replaced() {
        let result = interpolate(
            "${scheme}://${host}/${path}",
            &vars(&[("scheme", "https"), ("host", "api.example.com"), ("path", "users")]),
        )
        .unwrap();
        assert_eq!(result, "https://api.example.com/users");
    }

    #[test]
    fn same_variable_used_twice() {
        let result = interpolate("${a}+${a}", &vars(&[("a", "x")])).unwrap();
        assert_eq!(result, "x+x");
    }

    #[test]
    fn missing_variable_returns_error() {
        let err = interpolate("${missing}", &vars(&[])).unwrap_err();
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn empty_template_returns_empty() {
        let result = interpolate("", &vars(&[("a", "b")])).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn interpolate_headers_replaces_values() {
        let headers = vec![
            ("Authorization".to_string(), "Bearer ${token}".to_string()),
            ("X-App".to_string(), "hitop".to_string()),
        ];
        let result = interpolate_headers(&headers, &vars(&[("token", "abc123")])).unwrap();
        assert_eq!(result[0], ("Authorization".to_string(), "Bearer abc123".to_string()));
        assert_eq!(result[1], ("X-App".to_string(), "hitop".to_string()));
    }

    #[test]
    fn interpolate_headers_missing_variable_errors() {
        let headers = vec![("X-Key".to_string(), "${missing}".to_string())];
        assert!(interpolate_headers(&headers, &vars(&[])).is_err());
    }
}
