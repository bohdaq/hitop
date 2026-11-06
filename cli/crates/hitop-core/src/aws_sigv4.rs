#![cfg(feature = "native")]
//! AWS Signature Version 4: the secret managers' AWS calls and the `awsv4` auth preset
//! ([`sign_request`]), which signs a request as it is sent (method, URL, headers and body final).
use std::time::SystemTime;

use crate::model::AwsV4;

/// `YYYYMMDDTHHMMSSZ` in UTC.
pub fn amz_date(t: SystemTime) -> String {
    let secs = t.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// What [`sigv4`] signs. `headers` are lower-case names, all of them signed.
pub struct SigV4<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    pub headers: &'a [(String, String)],
    pub body: &'a [u8],
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub region: &'a str,
    pub service: &'a str,
    pub amz_date: &'a str,
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(ring::digest::digest(&ring::digest::SHA256, data).as_ref())
}

pub(crate) fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    ring::hmac::sign(&ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key), data.as_bytes()).as_ref().to_vec()
}

/// The `Authorization` header of AWS Signature Version 4.
pub fn sigv4(s: &SigV4) -> String {
    let mut headers: Vec<(String, String)> = s.headers.iter().map(|(k, v)| (k.to_ascii_lowercase(), v.split_whitespace().collect::<Vec<_>>().join(" "))).collect();
    headers.sort();
    let canonical_headers: String = headers.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let signed: Vec<&str> = headers.iter().map(|(k, _)| k.as_str()).collect();
    let signed = signed.join(";");
    let canonical = format!("{}\n{}\n{}\n{canonical_headers}\n{signed}\n{}", s.method, s.path, s.query, sha256_hex(s.body));
    let date = &s.amz_date[..8];
    let scope = format!("{date}/{}/{}/aws4_request", s.region, s.service);
    let to_sign = format!("AWS4-HMAC-SHA256\n{}\n{scope}\n{}", s.amz_date, sha256_hex(canonical.as_bytes()));
    let k = hmac(format!("AWS4{}", s.secret_key).as_bytes(), date);
    let k = hmac(&k, s.region);
    let k = hmac(&k, s.service);
    let k = hmac(&k, "aws4_request");
    let signature = hex(&hmac(&k, &to_sign));
    format!("AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed}, Signature={signature}", s.access_key)
}

/// AWS's URI encoding: unreserved characters as they are, the rest `%XX` (upper case).
fn aws_encode(s: &str, keep_slash: bool) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b'/' if keep_slash => "/".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Percent-decoding; `+` is a space only in the query.
fn decode(s: &str, plus_space: bool) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(if plus_space && bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Adds `Authorization`, `X-Amz-Date`, `X-Amz-Content-Sha256` (and `X-Amz-Security-Token` with a
/// session token) for `aws`: every header present is signed, plus `host`.
pub fn sign_request(aws: &AwsV4, method: &str, url: &str, headers: &mut Vec<(String, String)>, body: &[u8], now: SystemTime) -> anyhow::Result<()> {
    let u = reqwest::Url::parse(url).map_err(|e| anyhow::anyhow!("AWS Signature: invalid URL: {e}"))?;
    let host = match u.port() {
        Some(p) => format!("{}:{p}", u.host_str().unwrap_or_default()),
        None => u.host_str().unwrap_or_default().to_string(),
    };
    let path = {
        let p = u.path();
        let segments: Vec<String> = p.split('/').map(|seg| aws_encode(&decode(seg, false), false)).collect();
        let joined = segments.join("/");
        if joined.is_empty() { "/".to_string() } else { joined }
    };
    let mut pairs: Vec<(String, String)> = u.query().unwrap_or("").split('&').filter(|kv| !kv.is_empty()).map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (aws_encode(&decode(k, true), false), aws_encode(&decode(v, true), false))
    }).collect();
    pairs.sort();
    let query = pairs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&");
    let amz_date = amz_date(now);
    let payload = sha256_hex(body);
    headers.retain(|(k, _)| !["authorization", "x-amz-date", "x-amz-content-sha256", "x-amz-security-token", "host"].contains(&k.to_ascii_lowercase().as_str()));
    headers.push(("X-Amz-Date".into(), amz_date.clone()));
    headers.push(("X-Amz-Content-Sha256".into(), payload));
    if !aws.session_token.is_empty() {
        headers.push(("X-Amz-Security-Token".into(), aws.session_token.clone()));
    }
    let mut signed: Vec<(String, String)> = headers.iter().filter(|(k, _)| !k.is_empty()).map(|(k, v)| (k.to_ascii_lowercase(), v.clone())).collect();
    signed.push(("host".into(), host));
    let auth = sigv4(&SigV4 { method: &method.to_uppercase(), path: &path, query: &query, headers: &signed, body, access_key: &aws.access_key, secret_key: &aws.secret_key, region: &aws.region, service: &aws.service, amz_date: &amz_date });
    headers.push(("Authorization".into(), auth));
    Ok(())
}

/// Signs the request when `auth` is `awsv4` (its values filled in by `interpolate`); else nothing.
pub fn sign_if_aws(auth: Option<&crate::model::Auth>, interpolate: &dyn Fn(&str) -> anyhow::Result<String>, method: &str, url: &str, headers: &mut Vec<(String, String)>, body: &[u8]) -> anyhow::Result<()> {
    let Some(crate::model::Auth::AwsV4(a)) = auth else { return Ok(()) };
    let filled = AwsV4 { access_key: interpolate(&a.access_key)?, secret_key: interpolate(&a.secret_key)?, session_token: interpolate(&a.session_token)?, region: interpolate(&a.region)?, service: interpolate(&a.service)? };
    if filled.access_key.is_empty() || filled.secret_key.is_empty() || filled.region.is_empty() || filled.service.is_empty() {
        anyhow::bail!("AWS Signature: give the access key, secret key, region and service");
    }
    sign_request(&filled, method, url, headers, body, SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn signs_like_the_aws_test_suite() {
        // aws-sig-v4-test-suite get-vanilla-query-order-key-case: GET /?Param2=value2&Param1=value1.
        let aws = AwsV4 { access_key: "AKIDEXAMPLE".into(), secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(), region: "us-east-1".into(), service: "service".into(), session_token: String::new() };
        let mut headers = vec![];
        sign_request(&aws, "GET", "https://example.amazonaws.com/?Param2=value2&Param1=value1", &mut headers, b"", SystemTime::UNIX_EPOCH + Duration::from_secs(1_440_938_160)).unwrap();
        let auth = headers.iter().find(|(k, _)| k == "Authorization").unwrap().1.clone();
        assert!(auth.starts_with("AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature="), "{auth}");
        assert!(headers.iter().any(|(k, v)| k == "X-Amz-Content-Sha256" && v == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"));
    }

    #[test]
    fn the_web_app_signs_the_same() {
        // webapp/tests/unit.mjs signs this request too and expects the same signature.
        let aws = AwsV4 { access_key: "AKIDEXAMPLE".into(), secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(), region: "us-east-1".into(), service: "service".into(), session_token: String::new() };
        let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
        sign_request(&aws, "POST", "https://example.amazonaws.com/a b/?Param2=value2&Param1=value%201", &mut headers, br#"{"x":1}"#, SystemTime::UNIX_EPOCH + Duration::from_secs(1_440_938_160)).unwrap();
        let auth = headers.iter().find(|(k, _)| k == "Authorization").unwrap().1.clone();
        assert!(auth.ends_with("SignedHeaders=content-type;host;x-amz-content-sha256;x-amz-date, Signature=c7bcd27431aa0a1bc6ad750fffadf2bd3e7e5192ad433b09b8acf535e743c825"), "{auth}");
    }
}
