import Foundation

/// A pasted curl command as a request, deterministically (no model): the web app's `src/curl.js`
/// (UX2 (2)) rule for rule. Shell words as bash reads them ('…', "…", $'…', \ outside quotes,
/// line continuations of bash, cmd and PowerShell); Postman-style `{{name}}` (hitop's own snippets
/// write variables that way) becomes `${name}`. Tests: `UnitTests/CurlTests.swift` (the web app's
/// unit-ux2 cases and every curl.sh of hitop-core's snippet goldens).
enum CurlImport {
    struct Header: Equatable, Sendable {
        var name: String
        var value: String
    }

    struct FormField: Equatable, Sendable {
        var key: String
        var value: String
        /// "text" or "file".
        var type: String
        var fileName: String
    }

    enum Auth: Equatable, Sendable {
        case basic(username: String, password: String)
        case bearer(token: String)
    }

    struct Request: Equatable, Sendable {
        var method: String
        var url: String
        var headers: [Header]
        var body: String
        /// "raw" or "multipart".
        var bodyMode: String
        var formFields: [FormField]
        var auth: Auth?
        var warnings: [String]
    }

    struct Failure: Error, Equatable, LocalizedError, Sendable {
        var message: String
        init(_ message: String) { self.message = message }
        var errorDescription: String? { message }
    }

    /// Whether `text` is a curl command ("curl …", "$ curl …", "curl.exe …").
    static func isCurl(_ text: String) -> Bool {
        var s = Substring(text).drop(while: { $0.isWhitespace })
        if s.hasPrefix("$") {
            let rest = s.dropFirst()
            guard rest.first?.isWhitespace == true else { return false }
            s = rest.drop(while: { $0.isWhitespace })
        }
        for word in ["curl.exe", "curl"] where s.hasPrefix(word) {
            let after = s.dropFirst(word.count)
            if after.isEmpty || after.first!.isWhitespace { return true }
        }
        return false
    }

    // Options that take a value and only change how curl runs: skipped with their value.
    static let skipWithValue: Set<String> = [
        "-o", "--output", "-m", "--max-time", "--connect-timeout", "-w", "--write-out", "--retry", "--retry-delay", "--retry-max-time",
        "-x", "--proxy", "--cacert", "--capath", "--cert", "-E", "--key", "--cert-type", "--key-type", "-c", "--cookie-jar", "-K", "--config",
        "--resolve", "--connect-to", "--limit-rate", "-y", "--speed-time", "-Y", "--speed-limit", "--max-redirs", "--interface", "-r", "--range",
        "-z", "--time-cond", "--dns-servers", "--proxy-user", "-U", "--trace", "--trace-ascii", "--stderr", "--max-filesize", "--expect100-timeout",
        "-D", "--dump-header", "--unix-socket", "--abstract-unix-socket", "--happy-eyeballs-timeout-ms", "--keepalive-time", "--tls-max", "--ciphers",
    ]
    // Flags with no value that change nothing a request keeps (or that HITOP does anyway).
    static let skipFlags: Set<String> = [
        "-s", "--silent", "-S", "--show-error", "-L", "--location", "-k", "--insecure", "-i", "--include", "-v", "--verbose", "--compressed",
        "-N", "--no-buffer", "-f", "--fail", "--fail-with-body", "-#", "--progress-bar", "-q", "--disable", "-g", "--globoff", "--http1.0",
        "--http1.1", "--http2", "--http2-prior-knowledge", "--http3", "-4", "--ipv4", "-6", "--ipv6", "--no-keepalive", "--tr-encoding",
        "--path-as-is", "--location-trusted", "--raw", "-n", "--netrc", "--ssl", "--ssl-reqd", "--tlsv1.2", "--tlsv1.3", "-l", "--list-only",
        "--no-progress-meter", "-O", "--remote-name", "-J", "--remote-header-name", "--create-dirs",
    ]
    // Short flags with no value, for clusters like -sSL.
    static let shortFlags: Set<Character> = ["s", "S", "L", "k", "i", "v", "N", "f", "q", "g", "4", "6", "n", "l", "O", "J", "G", "I", "#"]
    // Short options whose value may be glued on (-XPOST, -H'Accept: x').
    static let shortWithValue: Set<Character> = ["X", "H", "d", "u", "A", "e", "b", "F", "o", "m", "w", "x", "E", "c", "K", "r", "z", "y", "Y", "U", "D"]

    /// Splits a command line into words as a POSIX shell would (no expansion).
    static func shellWords(_ text: String) throws -> [String] {
        let src = Array(text
            .replacingOccurrences(of: "\\\r\n", with: " ").replacingOccurrences(of: "\\\n", with: " ")
            .replacingOccurrences(of: "^\r\n", with: " ").replacingOccurrences(of: "^\n", with: " ")
            .replacingOccurrences(of: "`\r\n", with: " ").replacingOccurrences(of: "`\n", with: " "))
        var words: [String] = []
        var word: String?
        func push(_ s: String) { word = (word ?? "") + s }
        var i = 0
        while i < src.count {
            let c = src[i]
            if c == "'" {
                guard let end = src[(i + 1)...].firstIndex(of: "'") else { throw Failure("A ' quote isn't closed.") }
                push(String(src[(i + 1)..<end]))
                i = end
            } else if c == "$", i + 1 < src.count, src[i + 1] == "'" {
                var out = ""
                var j = i + 2
                while j < src.count, src[j] != "'" {
                    if src[j] != "\\" {
                        out.append(src[j])
                        j += 1
                        continue
                    }
                    j += 1
                    let n: Character? = j < src.count ? src[j] : nil
                    let map: [Character: String] = ["n": "\n", "t": "\t", "r": "\r", "\\": "\\", "'": "'", "\"": "\"", "a": "\u{07}",
                                                    "b": "\u{08}", "e": "\u{1B}", "f": "\u{0C}", "v": "\u{0B}", "0": "\0"]
                    if n == "x" || n == "u" {
                        let maxLen = n == "x" ? 2 : 4
                        var hex = ""
                        var k = j + 1
                        while k < src.count, hex.count < maxLen, src[k].isHexDigit {
                            hex.append(src[k])
                            k += 1
                        }
                        if !hex.isEmpty, let code = UInt32(hex, radix: 16), let scalar = Unicode.Scalar(code) {
                            out.append(Character(scalar))
                        } else {
                            out += "\\\(n!)"
                        }
                        j += hex.count
                    } else if let n {
                        out += map[n] ?? "\\\(n)"
                    } else {
                        out += "\\"
                    }
                    j += 1
                }
                if j >= src.count { throw Failure("A $' quote isn't closed.") }
                push(out)
                i = j
            } else if c == "\"" {
                var out = ""
                var j = i + 1
                while j < src.count, src[j] != "\"" {
                    if src[j] == "\\", j + 1 < src.count, "\\\"$`\n".contains(src[j + 1]) {
                        j += 1
                        out.append(src[j])
                    } else {
                        out.append(src[j])
                    }
                    j += 1
                }
                if j >= src.count { throw Failure("A \" quote isn't closed.") }
                push(out)
                i = j
            } else if c == "\\" {
                if i + 1 < src.count {
                    i += 1
                    push(String(src[i]))
                }
            } else if c.isWhitespace {
                if let w = word { words.append(w) }
                word = nil
            } else {
                push(String(c))
            }
            i += 1
        }
        if let w = word { words.append(w) }
        return words
    }

    /// `{{name}}` → `${name}`.
    static func vars(_ s: String) -> String {
        guard let re = try? NSRegularExpression(pattern: "\\{\\{\\s*([A-Za-z_$][\\w.$-]*)\\s*\\}\\}") else { return s }
        return re.stringByReplacingMatches(in: s, range: NSRange(s.startIndex..., in: s), withTemplate: "\\${$1}")
    }

    /// encodeURIComponent.
    static func encodeComponent(_ s: String) -> String {
        var allowed = CharacterSet.alphanumerics.intersection(CharacterSet(charactersIn: Unicode.Scalar(0)...Unicode.Scalar(127)))
        allowed.insert(charactersIn: "-_.!~*'()")
        return s.addingPercentEncoding(withAllowedCharacters: allowed) ?? s
    }

    // --data-urlencode's forms: content, =content, name=content, @file, name@file.
    static func urlencodeArg(_ v: String) -> String? {
        guard let eq = v.firstIndex(of: "=") else { return v.hasPrefix("@") ? nil : encodeComponent(v) }
        let name = String(v[..<eq])
        let rest = String(v[v.index(after: eq)...])
        return name.isEmpty ? encodeComponent(rest) : "\(name)=\(encodeComponent(rest))"
    }

    /// Typographic quotes and dashes back to the ASCII a shell reads: the iOS keyboard's smart
    /// punctuation (and notes apps a command was copied from) turn ' into ’ and -- into —.
    static func plain(_ text: String) -> String {
        var s = text
        for (from, to) in [("\u{2018}", "'"), ("\u{2019}", "'"), ("\u{201C}", "\""), ("\u{201D}", "\""), ("\u{2014}", "--"), ("\u{2013}", "--")] {
            s = s.replacingOccurrences(of: from, with: to)
        }
        return s
    }

    /// The request a curl command makes. Throws when it isn't one or has no URL.
    static func parse(_ raw: String) throws -> Request {
        let text = plain(raw)
        guard isCurl(text) else { throw Failure("That isn't a curl command: it should start with curl.") }
        var stripped = Substring(text).drop(while: { $0.isWhitespace })
        if stripped.hasPrefix("$") { stripped = stripped.dropFirst().drop(while: { $0.isWhitespace }) }
        let words = try shellWords(String(stripped))
        // Several commands joined with && or ; or |: the first one.
        let stop = words.indices.first { $0 > 0 && ["&&", "||", ";", "|"].contains(words[$0]) }
        let args = Array((stop.map { Array(words[..<$0]) } ?? words).dropFirst())
        var method: String?
        var url: String?
        var headers: [Header] = []
        var data: [String] = []
        var form: [FormField] = []
        var auth: Auth?
        var get = false
        var head = false
        var json = false
        var warnings: [String] = []
        var unknown: [String] = []
        var i = 0
        while i < args.count {
            var a = args[i]
            var value: String?
            // --opt=value
            if a.hasPrefix("--"), let eq = a.firstIndex(of: "=") {
                value = String(a[a.index(after: eq)...])
                a = String(a[..<eq])
            }
            // -XPOST, -H'Accept: x' (a value glued to a short option)
            let chars = Array(a)
            if chars.count > 2, chars[0] == "-", chars[1] != "-", chars[1].isLetter || chars[1] == "#" {
                if shortWithValue.contains(chars[1]) {
                    value = String(chars[2...])
                    a = "-\(chars[1])"
                } else if chars.dropFirst().allSatisfy({ shortFlags.contains($0) || shortWithValue.contains($0) }) {
                    // A cluster like -sSL or -sSX POST: flags, the last may take the next word.
                    let flags = Array(chars.dropFirst())
                    for f in flags.dropLast() {
                        if f == "G" { get = true } else if f == "I" { head = true } else if shortWithValue.contains(f) { throw Failure("-\(f) needs a value.") }
                    }
                    a = "-\(flags.last!)"
                }
            }
            let name = a
            func val() throws -> String {
                if let value { return value }
                guard i + 1 < args.count else { throw Failure("\(name) needs a value.") }
                i += 1
                return args[i]
            }
            switch a {
            case "-X", "--request":
                method = try val().uppercased()
            case "-H", "--header":
                let h = try val()
                if let colon = h.firstIndex(of: ":") {
                    headers.append(Header(name: h[..<colon].trimmingCharacters(in: .whitespaces),
                                          value: vars(h[h.index(after: colon)...].trimmingCharacters(in: .whitespaces))))
                } else {
                    // "-H 'X-Thing;'" sends an empty header; "-H 'X-Thing'" removes one: both kept as empty.
                    var n = h
                    if n.hasSuffix(";") { n.removeLast() }
                    n = n.trimmingCharacters(in: .whitespaces)
                    if !n.isEmpty { headers.append(Header(name: n, value: "")) }
                }
            case "-d", "--data", "--data-ascii", "--data-binary", "--data-raw":
                let v = try val()
                if v.hasPrefix("@") && a != "--data-raw" {
                    warnings.append("The body comes from the file \(v.dropFirst()): paste its contents into Body.")
                } else {
                    data.append(a == "--data-binary" || a == "--data-raw" ? v : v.replacingOccurrences(of: "\r", with: "").replacingOccurrences(of: "\n", with: ""))
                }
            case "--data-urlencode":
                if let v = urlencodeArg(try val()) { data.append(v) } else { warnings.append("A --data-urlencode value comes from a file: add it to Body.") }
            case "--json":
                data.append(try val())
                json = true
            case "-F", "--form", "--form-string":
                let v = try val()
                guard let eq = v.firstIndex(of: "=") else { throw Failure("\(a) '\(v)' should be name=value.") }
                let key = String(v[..<eq])
                let rest = String(v[v.index(after: eq)...])
                if a != "--form-string" && (rest.hasPrefix("@") || rest.hasPrefix("<")) {
                    let after = String(rest.dropFirst())
                    var fileName: String
                    if after.hasPrefix("\""), let close = after.dropFirst().firstIndex(of: "\"") {
                        fileName = String(after[after.index(after: after.startIndex)..<close])
                    } else {
                        fileName = String(after.split(separator: ";", omittingEmptySubsequences: false).first ?? "")
                    }
                    fileName = fileName.trimmingCharacters(in: .whitespaces)
                    form.append(FormField(key: key, value: "", type: "file", fileName: fileName))
                } else if a == "--form-string" {
                    form.append(FormField(key: key, value: vars(rest), type: "text", fileName: ""))
                } else {
                    var t = rest
                    if t.count >= 2, t.hasPrefix("\""), t.hasSuffix("\"") { t = String(t.dropFirst().dropLast()) }
                    if let r = t.range(of: ";type=") { t = String(t[..<r.lowerBound]) }
                    form.append(FormField(key: key, value: vars(t), type: "text", fileName: ""))
                }
            case "-u", "--user":
                let v = try val()
                if let colon = v.firstIndex(of: ":") {
                    auth = .basic(username: vars(String(v[..<colon])), password: vars(String(v[v.index(after: colon)...])))
                } else {
                    auth = .basic(username: vars(v), password: "")
                }
            case "-A", "--user-agent":
                headers.append(Header(name: "User-Agent", value: vars(try val())))
            case "-e", "--referer":
                headers.append(Header(name: "Referer", value: vars(try val())))
            case "-b", "--cookie":
                let v = try val()
                if v.contains("=") { headers.append(Header(name: "Cookie", value: vars(v))) } else { warnings.append("Cookies from the file \(v) were left out.") }
            case "--oauth2-bearer":
                auth = .bearer(token: vars(try val()))
            case "--url":
                url = try val()
            case "-G", "--get":
                get = true
            case "-I", "--head":
                head = true
            default:
                if skipWithValue.contains(a) {
                    if value == nil { i += 1 }
                } else if skipFlags.contains(a) {
                    // nothing a request keeps
                } else if a.hasPrefix("-") && a.count > 1 {
                    unknown.append(a)
                } else if url == nil {
                    url = a
                } else {
                    warnings.append("Only the first URL is used (\(a) left out).")
                }
            }
            i += 1
        }
        guard var url, !url.isEmpty else { throw Failure("The curl command has no URL.") }
        if !unknown.isEmpty {
            var seen = Set<String>()
            warnings.append("Left out: \(unknown.filter { seen.insert($0).inserted }.joined(separator: ", ")).")
        }
        url = vars(url)
        // -G: the data goes into the query.
        if get && !data.isEmpty {
            url += (url.contains("?") ? "&" : "?") + data.joined(separator: "&")
            data = []
        }
        func has(_ n: String) -> Bool { headers.contains { $0.name.lowercased() == n } }
        if json {
            if !has("content-type") { headers.append(Header(name: "Content-Type", value: "application/json")) }
            if !has("accept") { headers.append(Header(name: "Accept", value: "application/json")) }
        }
        var body = ""
        var bodyMode = "raw"
        if !form.isEmpty {
            bodyMode = "multipart"
        } else if !data.isEmpty {
            body = vars(data.joined(separator: "&"))
            // curl sends -d as a form unless told otherwise: said so when it isn't JSON the app would see.
            let start = body.drop(while: { $0.isWhitespace }).first
            if !has("content-type") && start != "{" && start != "[" {
                headers.append(Header(name: "Content-Type", value: "application/x-www-form-urlencoded"))
            }
        }
        let m = method ?? (head ? "HEAD" : (!form.isEmpty || !data.isEmpty) ? "POST" : "GET")
        return Request(method: m, url: url, headers: headers, body: body, bodyMode: bodyMode, formFields: form, auth: auth, warnings: warnings)
    }
}

extension CurlImport.Request {
    /// The request as HITOP keeps it (web app `store.applyCurl`): headers, auth, body; a JSON body is
    /// JSON, multipart file fields stay as rows to fill in (this version sends text fields only).
    var savedRequest: SavedRequest {
        var r = SavedRequest(name: AutoName.untitled, url: url, method: method)
        r.headers = headers.map { KeyValue($0.name, $0.value) }
        switch auth {
        case let .basic(u, p)?: r.auth = .basic(username: u, password: p)
        case let .bearer(t)?: r.auth = .bearer(token: t)
        case nil: break
        }
        if bodyMode == "multipart" {
            r.bodyMode = "multipart"
            r.formFields = formFields.map { FormRow($0.key, $0.type == "file" ? $0.fileName : $0.value, type: $0.type) }
        } else if !body.isEmpty {
            r.body = body
            let start = body.drop(while: { $0.isWhitespace }).first
            r.bodyType = start == "{" || start == "[" ? "json" : "text"
        }
        return r
    }
}
