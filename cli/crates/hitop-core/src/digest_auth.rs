//! HTTP Digest auth (RFC 7616; RFC 2617 servers too): the request goes out without credentials,
//! and on a `401` with `WWW-Authenticate: Digest ...` it is sent once more with the answer
//! ([`answer`]). MD5 (here, RFC 1321: no crate for one hash), MD5-sess and SHA-256 (ring), `qop`
//! `auth` or none. No native dependencies, so the web app computes the same (features.js).

use crate::model::Auth;

/// MD5 (RFC 1321) of `data`, lower-case hex.
pub fn md5_hex(data: &[u8]) -> String {
    const S: [u32; 64] = [7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21];
    let k: Vec<u32> = (0..64).map(|i| ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32).collect();
    let (mut a0, mut b0, mut c0, mut d0) = (0x67452301u32, 0xefcdab89u32, 0x98badcfeu32, 0x10325476u32);
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_le_bytes());
    for chunk in msg.chunks(64) {
        let m: Vec<u32> = chunk.chunks(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect();
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    [a0, b0, c0, d0].iter().flat_map(|v| v.to_le_bytes()).map(|b| format!("{b:02x}")).collect()
}

/// The parameters of a `Digest` challenge (`WWW-Authenticate`), names lower case; None when the
/// header isn't a Digest challenge.
pub fn parse_challenge(header: &str) -> Option<Vec<(String, String)>> {
    let rest = header.trim();
    let at = rest.to_ascii_lowercase().find("digest ")?;
    let mut s = &rest[at + 7..];
    let mut out = Vec::new();
    loop {
        s = s.trim_start_matches([' ', ',']);
        let Some(eq) = s.find('=') else { break };
        let name = s[..eq].trim().to_ascii_lowercase();
        s = &s[eq + 1..];
        let value;
        if let Some(q) = s.strip_prefix('"') {
            let end = q.find('"').unwrap_or(q.len());
            value = q[..end].to_string();
            s = q.get(end + 1..).unwrap_or("");
        } else {
            let end = s.find(',').unwrap_or(s.len());
            value = s[..end].trim().to_string();
            s = &s[end..];
        }
        // A second scheme starts: stop at it.
        if name.contains(' ') {
            break;
        }
        out.push((name, value));
    }
    Some(out)
}

/// SHA-256 as lower-case hex (ring natively, none in the web build).
pub type Sha256Hex<'a> = &'a dyn Fn(&[u8]) -> String;

/// The `Authorization` header answering `challenge` for `method` and `uri` (path and query).
/// `hash`: how to hash for SHA-256 (None: only MD5 works). `cnonce`: random per answer.
pub fn answer(challenge: &[(String, String)], username: &str, password: &str, method: &str, uri: &str, cnonce: &str, sha256: Option<Sha256Hex>) -> Result<String, String> {
    let get = |k: &str| challenge.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    let realm = get("realm").unwrap_or("");
    let nonce = get("nonce").ok_or("the Digest challenge has no nonce")?;
    let algorithm = get("algorithm").unwrap_or("MD5");
    let upper = algorithm.to_ascii_uppercase();
    let h = |data: &str| -> Result<String, String> {
        if upper.starts_with("SHA-256") {
            sha256.map(|f| f(data.as_bytes())).ok_or_else(|| "SHA-256 Digest isn't available here".to_string())
        } else if upper.starts_with("MD5") {
            Ok(md5_hex(data.as_bytes()))
        } else {
            Err(format!("Digest algorithm {algorithm} isn't supported (MD5, SHA-256)"))
        }
    };
    let qop = get("qop").map(|q| q.split(',').map(str::trim).any(|x| x.eq_ignore_ascii_case("auth"))).unwrap_or(false);
    if get("qop").is_some() && !qop {
        return Err("the server wants Digest qop auth-int, which isn't supported".into());
    }
    let mut ha1 = h(&format!("{username}:{realm}:{password}"))?;
    if upper.ends_with("-SESS") {
        ha1 = h(&format!("{ha1}:{nonce}:{cnonce}"))?;
    }
    let ha2 = h(&format!("{}:{uri}", method.to_uppercase()))?;
    let nc = "00000001";
    let response = if qop { h(&format!("{ha1}:{nonce}:{nc}:{cnonce}:auth:{ha2}"))? } else { h(&format!("{ha1}:{nonce}:{ha2}"))? };
    let mut out = format!(r#"Digest username="{username}", realm="{realm}", nonce="{nonce}", uri="{uri}", algorithm={algorithm}, response="{response}""#);
    if qop {
        out.push_str(&format!(r#", qop=auth, nc={nc}, cnonce="{cnonce}""#));
    }
    if let Some(opaque) = get("opaque") {
        out.push_str(&format!(r#", opaque="{opaque}""#));
    }
    Ok(out)
}

/// For a `401` of a request whose auth is `digest`: the headers to send it again with, or None.
#[cfg(feature = "native")]
pub fn retry_headers(auth: Option<&Auth>, interpolate: &dyn Fn(&str) -> anyhow::Result<String>, status: u16, response_headers: &std::collections::HashMap<String, String>, method: &str, url: &str, headers: &[(String, String)]) -> anyhow::Result<Option<Vec<(String, String)>>> {
    let Some(Auth::Digest { username, password }) = auth else { return Ok(None) };
    if status != 401 {
        return Ok(None);
    }
    let Some(challenge) = response_headers.get("www-authenticate").and_then(|h| parse_challenge(h)) else { return Ok(None) };
    let u = reqwest::Url::parse(url)?;
    let uri = match u.query() {
        Some(q) => format!("{}?{q}", u.path()),
        None => u.path().to_string(),
    };
    let cnonce = format!("{:016x}", rand_u64());
    let sha = |b: &[u8]| crate::aws_sigv4::sha256_hex(b);
    let value = answer(&challenge, &interpolate(username)?, &interpolate(password)?, method, &uri, &cnonce, Some(&sha)).map_err(|e| anyhow::anyhow!(e))?;
    let mut out: Vec<(String, String)> = headers.iter().filter(|(k, _)| !k.eq_ignore_ascii_case("authorization")).cloned().collect();
    out.push(("Authorization".into(), value));
    Ok(Some(out))
}

#[cfg(feature = "native")]
fn rand_u64() -> u64 {
    let mut b = [0u8; 8];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut b).ok();
    u64::from_le_bytes(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_vectors() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(md5_hex(b"The quick brown fox jumps over the lazy dog"), "9e107d9d372bb6826bd81d3542a419d6");
        assert_eq!(md5_hex(&[b'a'; 1000]), "cabe45dcc9ae5b66ba86600cca6b8ba8");
    }

    #[test]
    fn rfc_2617_example() {
        // RFC 2617 §3.5: Mufasa, testrealm@host.com.
        let c = parse_challenge(r#"Digest realm="testrealm@host.com", qop="auth,auth-int", nonce="dcd98b7102dd2f0e8b11d0f600bfb0c093", opaque="5ccc069c403ebaf9f0171e9517f40e41""#).unwrap();
        let a = answer(&c, "Mufasa", "Circle Of Life", "GET", "/dir/index.html", "0a4f113b", None).unwrap();
        assert!(a.contains(r#"response="6629fae49393a05397450978507c4ef1""#), "{a}");
        assert!(a.contains("qop=auth, nc=00000001") && a.contains(r#"opaque="5ccc069c403ebaf9f0171e9517f40e41""#));
        assert!(parse_challenge("Basic realm=\"x\"").is_none());
    }
}
