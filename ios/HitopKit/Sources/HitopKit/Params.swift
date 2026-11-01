import Foundation

/// Query parameters as rows, without decoding or validating anything, so
/// `${variables}` and half-typed URLs survive unchanged (hitop-core
/// `params.rs`, the web app's `parseQuery` / `withQuery`).
public enum Query {
    /// The URL's query as `(key, value)` pairs, in order.
    public static func parse(_ url: String) -> [(String, String)] {
        guard let q = url.firstIndex(of: "?") else { return [] }
        var query = url[url.index(after: q)...]
        if let hash = query.firstIndex(of: "#") { query = query[..<hash] }
        if query.isEmpty { return [] }
        return query.split(separator: "&", omittingEmptySubsequences: false).map { part in
            if let eq = part.firstIndex(of: "=") {
                return (String(part[..<eq]), String(part[part.index(after: eq)...]))
            }
            return (String(part), "")
        }
    }

    private static func encodeSpecial(_ s: String, _ chars: Set<Unicode.Scalar>) -> String {
        var out = ""
        for c in s.unicodeScalars {
            if c.properties.isWhitespace || chars.contains(c) {
                for b in String(c).utf8 { out += String(format: "%%%02X", b) }
            } else {
                out.unicodeScalars.append(c)
            }
        }
        return out
    }

    /// The URL with its query replaced by `rows`; the fragment is kept.
    /// Spaces and characters that would end a key or value are encoded, the
    /// rest is left as typed.
    public static func with(_ url: String, _ rows: [(String, String)]) -> String {
        let noHash: Substring
        let hash: Substring
        if let i = url.firstIndex(of: "#") {
            noHash = url[..<i]
            hash = url[i...]
        } else {
            noHash = url[...]
            hash = ""
        }
        let base = noHash.split(separator: "?", maxSplits: 1, omittingEmptySubsequences: false).first ?? ""
        let query = rows.filter { !$0.0.isEmpty || !$0.1.isEmpty }.map { k, v -> String in
            let key = encodeSpecial(k, ["&", "#", "="])
            return v.isEmpty ? key : "\(key)=\(encodeSpecial(v, ["&", "#"]))"
        }
        if query.isEmpty { return "\(base)\(hash)" }
        return "\(base)?\(query.joined(separator: "&"))\(hash)"
    }

    /// `rows` appended to the URL's query.
    public static func append(_ url: String, _ rows: [(String, String)]) -> String {
        with(url, parse(url) + rows)
    }
}
