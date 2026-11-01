import Foundation

/// A collection folder (format v1, `spec/collection-format.md`), read into
/// the JSON model:
///
/// ```text
/// my-api/
///   hitop.yaml            format, name, variables, default auth, request order
///   requests/**/*.yaml    one request per file; subfolders are folders
///   environments/*.yaml   variables; secret ones have no value in the file
///   globals.yaml          optional: the globals scope
/// ```
///
/// The reader follows hitop-core's `format` module: the same defaults, the
/// same errors for keys that would change meaning, the same ids.
public struct CollectionFolder: Sendable {
    public var collection: Collection
    /// In file name order.
    public var environments: [Environment]
    /// `globals.yaml`, when the folder has one.
    public var globals: Environment?
    /// The folder itself.
    public var url: URL

    public static let manifest = "hitop.yaml"
    public static let formatVersion = 1

    /// True when `url` holds a `hitop.yaml`.
    public static func isFolder(_ url: URL) -> Bool {
        var dir: ObjCBool = false
        return FileManager.default.fileExists(atPath: url.appendingPathComponent(manifest).path, isDirectory: &dir) && !dir.boolValue
    }

    /// The environment called `name` (its `name`, as `hitop run --env` takes it).
    public func environment(named name: String) -> Environment? {
        environments.first { $0.name == name }
    }

    /// Reads the folder at `url`.
    public static func read(_ url: URL) throws -> CollectionFolder {
        let manifestURL = url.appendingPathComponent(manifest)
        guard isFolder(url) else {
            throw FormatError("\(url.path) is not a collection folder (no \(manifest); create one with `hitop init`)")
        }
        let doc = try parseFile(manifestURL)
        let m = try mapping(doc, manifest)
        switch m["format"] {
        case nil, .null?:
            throw FormatError("\(manifest): `format: \(formatVersion)` is missing")
        case let v?:
            guard let n = Int(try v.scalarString("\(manifest): format")) else { throw FormatError("\(manifest): format must be a number") }
            if n > formatVersion { throw FormatError("\(manifest): format \(n) needs a newer hitop") }
        }
        try checkExtra(m, known: ["format", "name", "id", "workspace", "variables", "auth", "order", "openapi"],
                       reserved: ["id", "name", "workspace", "requests", "variables", "auth", "format", "order"], manifest)

        let files = try yamlFiles(url.appendingPathComponent("requests"), recursive: true)
        // Paths that differ only in case are one file on macOS and Windows.
        var lower: [String: String] = [:]
        for p in sortedKeys(files) {
            if let prev = lower.updateValue(p, forKey: p.lowercased()) {
                throw FormatError("requests/\(prev) and requests/\(p) differ only in case; rename one")
            }
        }
        var ordered: [String] = []
        var seen = Set<String>()
        if let order = m["order"] {
            guard case let .sequence(entries) = order else { throw FormatError("\(manifest): order must be a list") }
            for e in entries {
                let entry = try e.scalarString("\(manifest): order")
                do { try Names.checkRelPath(entry) } catch { throw FormatError("\(manifest): order: \(error)") }
                if files[entry] != nil, seen.insert(entry).inserted { ordered.append(entry) }
            }
        }
        ordered += sortedKeys(files).filter { !seen.contains($0) }

        var requests: [Request] = []
        for path in ordered {
            let file = files[path]!
            try Names.checkRelPath(path)
            requests.append(try readRequest(try parseFile(file), path: path, fileExtension: file.pathExtension))
        }

        let name: String
        if let n = m["name"], case .null = n {
            name = url.standardizedFileURL.lastPathComponent
        } else if let n = m["name"] {
            name = try n.scalarString("\(manifest): name")
        } else {
            name = url.standardizedFileURL.lastPathComponent
        }
        var collection = Collection(
            id: try m["id"].map { try uint($0, "\(manifest): id") } ?? Names.collectionID(name),
            name: name,
            requests: requests,
            variables: try stringMap(m["variables"], "\(manifest): variables"),
            workspace: try m["workspace"].flatMap { v -> String? in
                if case .null = v { return nil }
                return try v.scalarString("\(manifest): workspace")
            },
            auth: try m["auth"].flatMap { try readAuth($0, manifest) }
        )
        if collection.name.isEmpty { collection.name = url.standardizedFileURL.lastPathComponent }

        var environments: [Environment] = []
        let envFiles = try yamlFiles(url.appendingPathComponent("environments"), recursive: false)
        for stem in sortedKeys(envFiles) {
            let what = "environments/\(stem)"
            environments.append(try readEnvironment(try parseFile(envFiles[stem]!), stem: stem, what: what))
        }

        var globals: Environment?
        for file in ["globals.yaml", "globals.yml"] {
            let g = url.appendingPathComponent(file)
            if FileManager.default.fileExists(atPath: g.path) {
                var env = try readEnvironment(try parseFile(g), stem: globalsID, what: file)
                env.id = globalsID
                env.name = globalsID
                globals = env
                break
            }
        }
        return CollectionFolder(collection: collection, environments: environments, globals: globals, url: url)
    }

    /// Reads one environment file from anywhere (`hitop run --env-file`).
    public static func readEnvironmentFile(_ url: URL) throws -> Environment {
        guard ["yaml", "yml"].contains(url.pathExtension) else {
            throw FormatError("\(url.lastPathComponent): an environment file is .yaml or .yml")
        }
        let stem = url.deletingPathExtension().lastPathComponent
        return try readEnvironment(try parseFile(url), stem: stem, what: url.lastPathComponent)
    }

    // MARK: - Documents

    static func readRequest(_ doc: YAMLValue, path: String, fileExtension: String) throws -> Request {
        let what = "requests/\(path)"
        let d = try mapping(doc, what)
        // Keys the JSON model gives a meaning to can't be unknown keys here.
        try checkExtra(d, known: ["name", "kind", "method", "url", "params", "headers", "auth", "body", "websocket", "grpc", "scripts", "examples", "id", "openapi"],
                       reserved: ["id", "name", "url", "method", "headers", "body", "preRequestScript", "postRequestScript", "auth", "bodyMode",
                                  "formFields", "disabledParams", "bodyType", "examples", "file", "params", "scripts", "graphql", "kind", "websocket", "grpc"], what)

        var on: [(String, String)] = []
        var disabled: [Param] = []
        for row in try rows(d["params"], what) {
            if row.enabled {
                if let k = row.extra.first {
                    throw FormatError("\(what): query parameter '\(row.name)': extra keys like '\(k)' are only kept on disabled parameters (enabled ones live in the URL)")
                }
                on.append((row.name, row.value))
            } else {
                disabled.append(Param(key: row.name, value: row.value))
            }
        }
        let rawURL = try str(d["url"], "\(what): url")
        let url = on.isEmpty ? rawURL : Query.append(rawURL, on)
        let headers = try rows(d["headers"], what).map { Header($0.name, $0.value, enabled: $0.enabled) }

        var body = ""
        var bodyMode: String?
        var bodyType: String?
        var formFields: [FormField] = []
        var graphql: GraphQL?
        if let b = d["body"], !isNull(b) {
            let bm = try mapping(b, "\(what): body")
            if let k = bm.keys.first(where: { !["type", "content", "variables", "operationName", "fields"].contains($0) }) {
                throw FormatError("\(what): body: unknown key '\(k)'")
            }
            let type = try bm["type"].flatMap { isNull($0) ? nil : try $0.scalarString("\(what): body.type") } ?? "none"
            body = try str(bm["content"], "\(what): body.content")
            let g = GraphQL(variables: try str(bm["variables"], "\(what): body.variables"),
                            operationName: try str(bm["operationName"], "\(what): body.operationName"))
            graphql = g.isEmpty ? nil : g
            formFields = try rows(bm["fields"], "\(what): body").map {
                if $0.extra.contains("key") { throw FormatError("\(what): the key 'key' can't be carried over (it means something else on the other side)") }
                return FormField(key: $0.name, value: $0.value, type: $0.type ?? "text", enabled: $0.enabled, fileName: $0.fileName)
            }
            if ["none", "json", "text", "xml", "graphql"].contains(type) {
                if type != BodyType.infer(headers: headers, body: body) { bodyType = type }
            } else if type == "form" {
                bodyMode = "urlencoded"
            } else {
                bodyMode = type
            }
        }

        if let w = d["websocket"], !isNull(w) {
            let wm = try mapping(w, "\(what): websocket")
            if let k = wm.keys.first(where: { !["subprotocols", "messages"].contains($0) }) {
                throw FormatError("\(what): websocket: unknown key '\(k)'")
            }
        }
        if let g = d["grpc"], !isNull(g) {
            let gm = try mapping(g, "\(what): grpc")
            if let k = gm.keys.first(where: { !["service", "method", "message", "protos", "importPaths", "plaintext"].contains($0) }) {
                throw FormatError("\(what): grpc: unknown key '\(k)'")
            }
        }

        var pre = "", post = ""
        if let s = d["scripts"], !isNull(s) {
            let sm = try mapping(s, "\(what): scripts")
            if let k = sm.keys.first(where: { !["pre", "post"].contains($0) }) {
                throw FormatError("\(what): scripts: unknown key '\(k)'")
            }
            pre = try str(sm["pre"], "\(what): scripts.pre")
            post = try str(sm["post"], "\(what): scripts.post")
        }

        let kind = try d["kind"].map { try $0.scalarString("\(what): kind") }.flatMap { $0 == "http" || $0.isEmpty ? nil : $0 }
        let defaultName = String(path.split(separator: "/").last ?? Substring(path))
        let name = try d["name"].flatMap { isNull($0) ? nil : try $0.scalarString("\(what): name") } ?? defaultName
        let method = try d["method"].flatMap { isNull($0) ? nil : try $0.scalarString("\(what): method") } ?? "GET"
        return Request(
            id: try d["id"].map { try uint($0, "\(what): id") } ?? Names.requestID(path),
            name: name,
            url: url,
            method: method,
            headers: headers,
            body: body,
            preRequestScript: pre,
            postRequestScript: post,
            auth: try d["auth"].flatMap { try readAuth($0, what) },
            bodyMode: bodyMode,
            formFields: formFields,
            disabledParams: disabled,
            bodyType: bodyType,
            graphql: graphql,
            kind: kind,
            path: path,
            fileExtension: fileExtension
        )
    }

    static func readEnvironment(_ doc: YAMLValue, stem: String, what: String) throws -> Environment {
        let d = try mapping(doc, what)
        try checkExtra(d, known: ["name", "id", "variables"], reserved: ["id", "name", "variables", "secrets"], what)
        var variables: [String: String] = [:]
        var secrets: [String] = []
        if let v = d["variables"], !isNull(v) {
            guard case let .mapping(pairs) = v else { throw FormatError("\(what): variables must be a map") }
            for (k, value) in pairs {
                if case let .mapping(o) = value {
                    if o.count == 1, o[0].0 == "secret", case .bool(true) = o[0].1 {
                        secrets.append(k)
                    } else {
                        throw FormatError("\(what): variable '\(k)' must be a value or {secret: true}")
                    }
                } else {
                    variables[k] = try value.scalarString("\(what): variable '\(k)'")
                }
            }
        }
        let name = try d["name"].flatMap { isNull($0) ? nil : try $0.scalarString("\(what): name") } ?? stem
        let id = try d["id"].flatMap { isNull($0) ? nil : try $0.scalarString("\(what): id") } ?? stem
        return Environment(id: id, name: name, variables: variables, secrets: secrets.sorted(by: utf8Less))
    }

    static func readAuth(_ v: YAMLValue, _ what: String) throws -> Auth? {
        if isNull(v) { return nil }
        let m = try mapping(v, "\(what): auth")
        let s = { (k: String) in try str(m[k], "\(what): auth.\(k)") }
        switch try str(m["type"], "\(what): auth.type") {
        case "none": return Auth.none
        case "bearer": return .bearer(token: try s("token"))
        case "basic": return .basic(username: try s("username"), password: try s("password"))
        case "apikey":
            let loc = try s("in")
            guard let location = loc.isEmpty ? .header : ApiKeyLocation(rawValue: loc) else {
                throw FormatError("\(what): auth.in must be header or query, got '\(loc)'")
            }
            return .apiKey(key: try s("key"), value: try s("value"), location: location)
        case let other:
            throw FormatError("\(what): unknown auth type '\(other)' (none, bearer, basic, apikey)")
        }
    }

    // MARK: - Helpers

    struct Row {
        var name: String
        var value: String
        var type: String?
        var enabled: Bool
        var fileName: String?
        /// Keys other than name, value, type and enabled.
        var extra: [String]
    }

    static func rows(_ v: YAMLValue?, _ what: String) throws -> [Row] {
        guard let v, !isNull(v) else { return [] }
        guard case let .sequence(items) = v else { throw FormatError("\(what): expected a list of rows") }
        return try items.map { item in
            let m = try mapping(item, what)
            let extra = m.keys.filter { !["name", "value", "type", "enabled"].contains($0) }
            if extra.contains("key") {
                throw FormatError("\(what): the key 'key' can't be carried over (it means something else on the other side)")
            }
            var enabled = true
            if let e = m["enabled"] {
                switch e {
                case let .bool(b): enabled = b
                case .null: enabled = true
                default: throw FormatError("\(what): enabled must be true or false")
                }
            }
            let type = try m["type"].flatMap { isNull($0) ? nil : try $0.scalarString("\(what): type") }
            let fileName = try m["fileName"].map { try $0.scalarString("\(what): fileName") }
            return Row(name: try str(m["name"], "\(what): name"), value: try str(m["value"], "\(what): value"),
                       type: type, enabled: enabled, fileName: fileName, extra: extra)
        }
    }

    static func isNull(_ v: YAMLValue) -> Bool {
        if case .null = v { return true }
        return false
    }

    static func mapping(_ v: YAMLValue, _ what: String) throws -> YAMLValue {
        switch v {
        case .mapping: return v
        case .null: return .mapping([])
        default: throw FormatError("\(what): expected a map, found a \(v.kindName)")
        }
    }

    static func str(_ v: YAMLValue?, _ what: String) throws -> String {
        try v?.scalarString(what) ?? ""
    }

    static func uint(_ v: YAMLValue, _ what: String) throws -> UInt64 {
        guard let n = UInt64(try v.scalarString(what)) else { throw FormatError("\(what): expected a whole number") }
        return n
    }

    static func stringMap(_ v: YAMLValue?, _ what: String) throws -> [String: String] {
        guard let v, !isNull(v) else { return [:] }
        guard case let .mapping(pairs) = v else { throw FormatError("\(what): expected a map") }
        var out: [String: String] = [:]
        for (k, value) in pairs { out[k] = try value.scalarString("\(what): \(k)") }
        return out
    }

    /// An unknown key that means something in the JSON model is an error:
    /// it would be read back as something else.
    static func checkExtra(_ m: YAMLValue, known: Set<String>, reserved: Set<String>, _ what: String) throws {
        if let k = m.keys.first(where: { !known.contains($0) && reserved.contains($0) }) {
            throw FormatError("\(what): the key '\(k)' can't be carried over (it means something else on the other side)")
        }
    }

    static func parseFile(_ url: URL) throws -> YAMLValue {
        let data: Data
        do { data = try Data(contentsOf: url) } catch { throw FormatError("reading \(url.path): \(error.localizedDescription)") }
        guard let text = String(data: data, encoding: .utf8) else { throw FormatError("\(url.path): not UTF-8") }
        do { return try YAMLValue.parse(text) } catch let e as FormatError { throw FormatError("\(url.path): \(e)") } catch {
            throw FormatError("\(url.path): \(error)")
        }
    }

    static func utf8Less(_ a: String, _ b: String) -> Bool {
        a.utf8.lexicographicallyPrecedes(b.utf8)
    }

    static func sortedKeys(_ m: [String: URL]) -> [String] {
        m.keys.sorted(by: utf8Less)
    }

    /// Every `.yaml`/`.yml` file under `dir` by its `/`-separated path without
    /// the extension. Hidden entries and symlinks are skipped; `a.yaml` next
    /// to `a.yml` is an error.
    static func yamlFiles(_ dir: URL, recursive: Bool) throws -> [String: URL] {
        var out: [String: URL] = [:]
        let fm = FileManager.default
        var isDir: ObjCBool = false
        guard fm.fileExists(atPath: dir.path, isDirectory: &isDir), isDir.boolValue else { return out }
        func walk(_ d: URL, _ prefix: String) throws {
            let names: [String]
            do { names = try fm.contentsOfDirectory(atPath: d.path) } catch { throw FormatError("reading \(d.path): \(error.localizedDescription)") }
            for name in names.sorted(by: utf8Less) where !name.hasPrefix(".") {
                let u = d.appendingPathComponent(name)
                // attributesOfItem doesn't follow symlinks.
                let type = (try? fm.attributesOfItem(atPath: u.path)[.type] as? FileAttributeType) ?? nil
                if type == .typeDirectory {
                    if recursive { try walk(u, prefix + name + "/") }
                } else if type == .typeRegular, ["yaml", "yml"].contains(u.pathExtension) {
                    let key = prefix + String(name.dropLast(u.pathExtension.count + 1))
                    if let prev = out.updateValue(u, forKey: key) {
                        throw FormatError("\(prev.path) and \(u.path) are the same request; keep one")
                    }
                }
            }
        }
        try walk(dir, "")
        return out
    }
}

/// The body type hitop infers from a text body and its `Content-Type`.
public enum BodyType {
    public static func infer(headers: [Header], body: String) -> String {
        if body.isEmpty { return "none" }
        let ct = headers.first { $0.enabled && $0.name.lowercased() == "content-type" }?.value.lowercased() ?? ""
        if ct.contains("json") { return "json" }
        if ct.contains("xml") { return "xml" }
        if !ct.isEmpty { return "text" }
        let t = body.drop(while: { $0.isWhitespace })
        if t.hasPrefix("{") || t.hasPrefix("[") {
            if case .some(.object) = try? JSONValue.parse(body) { return "json" }
            if case .some(.array) = try? JSONValue.parse(body) { return "json" }
        }
        if t.hasPrefix("<") { return "xml" }
        return "text"
    }
}
