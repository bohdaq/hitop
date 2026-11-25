import Foundation

/// Files in and out: hitop-core's importers and exporters through SyncFFI (the same code the
/// Android app and the CLI run), and zips of collection folders.
enum Importing {
    static let maxBytes = 25 * 1024 * 1024

    struct Failure: Error, LocalizedError, Equatable {
        var message: String
        var errorDescription: String? { message }
    }

    /// An environment to import; its secret values go to the vault only.
    struct ImportedEnvironment: Equatable {
        var environment: SavedEnvironment
        var secretValues: [String: String] = [:]
    }

    /// What a file holds, read but not yet added.
    struct Plan: Equatable {
        var kind: String
        var collections: [SavedCollection] = []
        var environments: [ImportedEnvironment] = []
        var globals: ImportedEnvironment?
        var issues: [String] = []

        var title: String {
            switch kind {
            case "postman": "Postman collection"
            case "openapi": "OpenAPI"
            case "environment": "Postman environment"
            case "globals": "Postman globals"
            case "folder": "HITOP collection folder"
            case "insomnia": "Insomnia"
            case "bruno": "Bruno collection"
            case "har": "HAR recording"
            default: "HITOP collections"
            }
        }

        /// "2 collections, 14 requests, 1 environment".
        var summary: String {
            func count(_ n: Int, _ word: String) -> String? { n == 0 ? nil : "\(n) \(word)\(n == 1 ? "" : "s")" }
            return [count(collections.count, "collection"), count(collections.reduce(0) { $0 + $1.requests.count }, "request"),
                    count(environments.count, "environment"), globals == nil ? nil : "globals"].compactMap { $0 }.joined(separator: ", ")
        }
    }

    static func isZip(_ data: Data) -> Bool { data.starts(with: [0x50, 0x4B, 0x03, 0x04]) }

    /// `source`: the address the text came from (OpenAPI's relative servers resolve against it).
    static func read(_ data: Data, name: String = "Imported", source: String? = nil) throws -> Plan {
        guard data.count <= maxBytes else { throw Failure(message: "The file is too large.") }
        if isZip(data) {
            let files = strings(try SyncCore.call("unzip", ["data": .string(CloudCrypto.base64url([UInt8](data)))]))
            if files.keys.contains(where: { $0 == "hitop.yaml" || $0.hasSuffix("/hitop.yaml") }) { return try folder(files, name: name) }
            if files.keys.contains(where: { $0 == "bruno.json" || $0.hasSuffix("/bruno.json") }) {
                let bru = files.filter { $0.key.hasSuffix(".bru") || $0.key.hasSuffix("bruno.json") }
                return try plan(SyncCore.call("importFiles", ["files": .object(bru.mapValues(JSONAny.string))]))
            }
            throw Failure(message: "The zip has no hitop.yaml or bruno.json: it isn't a collection folder.")
        }
        guard let text = String(data: data, encoding: .utf8) else { throw Failure(message: "The file isn't text.") }
        return try read(text: text, source: source)
    }

    static func read(text: String, source: String? = nil) throws -> Plan {
        var args: [String: JSONAny] = ["text": .string(text)]
        if let source { args["source"] = .string(source) }
        return try plan(SyncCore.call("import", args))
    }

    private static func strings(_ j: JSONAny?) -> [String: String] {
        guard case let .object(o)? = j else { return [:] }
        return o.compactMapValues { $0.string }
    }

    private static func list(_ j: JSONAny?) -> [JSONAny] {
        guard case let .array(a)? = j else { return [] }
        return a
    }

    private static func environment(_ j: JSONAny?) -> SavedEnvironment? {
        guard let j, j != .null else { return nil }
        return try? JSONDecoder().decode(SavedEnvironment.self, from: j.canonical)
    }

    private static func imported(_ j: JSONAny) -> ImportedEnvironment? {
        environment(j["environment"]).map { ImportedEnvironment(environment: $0, secretValues: strings(j["secretValues"])) }
    }

    private static func plan(_ j: JSONAny) throws -> Plan {
        var p = Plan(kind: j["kind"]?.string ?? "hitop")
        // Collections cross as text: exact 64-bit ids.
        p.collections = try list(j["collections"]).compactMap(\.string).map { try JSONDecoder().decode(SavedCollection.self, from: Data($0.utf8)) }
        p.environments = list(j["environments"]).compactMap(imported)
        p.issues = list(j["issues"]).compactMap(\.string)
        if let one = imported(j) {
            if p.kind == "globals" { p.globals = one } else { p.environments.append(one) }
        }
        return p
    }

    private static func folder(_ files: [String: String], name: String) throws -> Plan {
        // The hitop.yaml closest to the top; the folder around it.
        let marker = files.keys.filter { $0 == "hitop.yaml" || $0.hasSuffix("/hitop.yaml") }.min { $0.count < $1.count } ?? "hitop.yaml"
        let root = String(marker.dropLast("hitop.yaml".count))
        var kept: [String: JSONAny] = [:]
        for (path, text) in files where path.hasPrefix(root) {
            let p = String(path.dropFirst(root.count))
            guard p.hasSuffix(".yaml") || p.hasSuffix(".yml") else { continue }
            if !p.contains("/") || p.hasPrefix("requests/") || p.hasPrefix("environments/") { kept[p] = .string(text) }
        }
        let fallback = root.isEmpty ? name : String(root.split(separator: "/").last ?? "Imported")
        let text = try SyncCore.string("readFiles", ["files": .object(kept), "name": .string(fallback)])
        struct Folder: Decodable {
            var collection: SavedCollection
            var environments: [SavedEnvironment]?
            var globals: SavedEnvironment?
        }
        let f = try JSONDecoder().decode(Folder.self, from: Data(text.utf8))
        var p = Plan(kind: "folder", collections: [f.collection], environments: (f.environments ?? []).map { ImportedEnvironment(environment: $0) })
        if let g = f.globals, !g.variables.isEmpty || !g.secrets.isEmpty { p.globals = ImportedEnvironment(environment: g) }
        return p
    }

    // MARK: Export

    enum Format: String, CaseIterable, Identifiable {
        case hitop, postman, zip
        var id: String { rawValue }
        var label: String {
            switch self {
            case .hitop: "HITOP JSON"
            case .postman: "Postman v2.1"
            case .zip: "Collection folder (.zip)"
            }
        }
    }

    private static func json(_ c: SavedCollection) throws -> String {
        String(decoding: try JSONEncoder().encode(c), as: UTF8.self)
    }

    static func export(_ c: SavedCollection, as format: Format) throws -> (name: String, data: Data) {
        let slug = c.name.lowercased().split { !$0.isLetter && !$0.isNumber }.joined(separator: "-")
        let base = slug.isEmpty ? "collection" : slug
        switch format {
        case .hitop, .postman:
            let text = try SyncCore.string("export", ["collection": .string(json(c)), "format": .string(format.rawValue)])
            return (format == .hitop ? "\(base).hitop.json" : "\(base).postman_collection.json", Data(text.utf8))
        case .zip:
            let folder = "{\"collection\":\(try json(c)),\"environments\":[]}"
            let files = try SyncCore.call("toFiles", ["folder": .string(folder)])
            var named: [String: JSONAny] = [:]
            for (p, t) in strings(files) { named["\(base)/\(p)"] = .string(t) }
            let packed = try SyncCore.string("zip", ["files": .object(named)])
            guard let data = CloudCrypto.unbase64url(packed).map({ Data($0) }) else { throw Failure(message: "The zip couldn't be made.") }
            return ("\(base).zip", data)
        }
    }
}

extension AppModel {
    /// Adds what `plan` holds. Ids something here already has (an export of something that is still
    /// here, imported again) are replaced, as are environment ids; secret values go to the vault.
    @discardableResult
    func apply(_ plan: Importing.Plan) -> [UInt64] {
        var added: [UInt64] = []
        for var c in plan.collections {
            var taken = Set(collections.flatMap { $0.requests.map(\.id) })
            for i in c.requests.indices where c.requests[i].id == 0 || !taken.insert(c.requests[i].id).inserted {
                c.requests[i].id = IDs.next()
                taken.insert(c.requests[i].id)
            }
            if c.id == 0 || collections.contains(where: { $0.id == c.id }) { c.id = IDs.next() }
            c.org = orgScope?.newCollectionOrg
            addCollectionCopy(c)
            added.append(c.id)
        }
        for i in plan.environments {
            var e = addEnvironment(name: i.environment.name)
            for n in i.environment.secrets { e = addSecret(n, value: i.secretValues[n] ?? "", to: e) }
            for (k, v) in i.environment.variables where !i.environment.secrets.contains(k) { e.variables[k] = v }
            updateEnvironment(e)
            if state.activeEnvironmentID == nil { setActiveEnvironment(e.id) }
        }
        if let g = plan.globals {
            for (k, v) in g.environment.variables where !g.environment.secrets.contains(k) { setVariable(globals, name: k, value: v, secret: false) }
            for n in g.environment.secrets { setVariable(globals, name: n, value: g.secretValues[n] ?? secretValue(globals, n), secret: true) }
        }
        return added
    }

    private func addSecret(_ name: String, value: String, to e: SavedEnvironment) -> SavedEnvironment {
        var out = e
        out.secrets.append(name)
        vault.setValue(value, environment: vaultID(e), name: name)
        return out
    }
}
