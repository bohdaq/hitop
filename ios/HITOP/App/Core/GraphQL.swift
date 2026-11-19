import Foundation

// I4 GraphQL schema: introspection through the normal send path (the web app's `store.fetchSchema`:
// the request's URL, headers and auth, POST, the introspection query as a GraphQL body with
// operation name IntrospectionQuery, no scripts), the answer read into types and fields for the
// schema browser and the query editor's field suggestions.

/// A type reference as GraphQL writes it (`[Product!]!`) and the named type under it.
struct GQLTypeRef: Hashable, Sendable {
    var display: String
    var named: String

    init(display: String, named: String) {
        self.display = display
        self.named = named
    }

    init?(_ v: JSONAny?) {
        guard let v, case .object = v else { return nil }
        switch v["kind"]?.string {
        case "NON_NULL":
            guard let inner = GQLTypeRef(v["ofType"]) else { return nil }
            self.init(display: inner.display + "!", named: inner.named)
        case "LIST":
            guard let inner = GQLTypeRef(v["ofType"]) else { return nil }
            self.init(display: "[\(inner.display)]", named: inner.named)
        default:
            guard let n = v["name"]?.string else { return nil }
            self.init(display: n, named: n)
        }
    }
}

struct GQLArgument: Hashable, Sendable {
    var name: String
    var type: GQLTypeRef
}

struct GQLField: Hashable, Sendable, Identifiable {
    var name: String
    var description: String?
    var type: GQLTypeRef
    var args: [GQLArgument]
    var deprecated: Bool
    var id: String { name }

    /// `products(first: Int): [Product!]!`
    var signature: String {
        let a = args.isEmpty ? "" : "(" + args.map { "\($0.name): \($0.type.display)" }.joined(separator: ", ") + ")"
        return "\(name)\(a): \(type.display)"
    }
}

struct GQLType: Hashable, Sendable, Identifiable {
    var name: String
    /// OBJECT, INTERFACE, UNION, ENUM, INPUT_OBJECT, SCALAR.
    var kind: String
    var description: String?
    var fields: [GQLField]
    var enumValues: [String]
    var possibleTypes: [String]
    var id: String { name }
}

struct GraphQLSchema: Hashable, Sendable {
    var queryType: String?
    var mutationType: String?
    var subscriptionType: String?
    /// Every type but the introspection ones (`__Type`…), in the server's order.
    var types: [GQLType]

    func type(_ name: String?) -> GQLType? {
        guard let name else { return nil }
        return types.first { $0.name == name }
    }

    /// The root types first, then the other object types, then the rest by name.
    var browsable: [GQLType] {
        let roots = [queryType, mutationType, subscriptionType].compactMap { type($0) }
        let rootNames = Set(roots.map(\.name))
        let rest = types.filter { !rootNames.contains($0.name) && !["String", "Int", "Float", "Boolean", "ID"].contains($0.name) }
        let order = ["OBJECT": 0, "INTERFACE": 1, "UNION": 2, "INPUT_OBJECT": 3, "ENUM": 4, "SCALAR": 5]
        return roots + rest.sorted { (order[$0.kind] ?? 9, $0.name) < (order[$1.kind] ?? 9, $1.name) }
    }

    struct Failure: Error, LocalizedError, Equatable {
        var message: String
        var errorDescription: String? { message }
    }

    /// The schema in an introspection answer (`{data: {__schema}}`); the answer's `errors` or a
    /// missing `__schema` (introspection turned off) are an error, worded as the web app does.
    static func parse(_ text: String) throws -> GraphQLSchema {
        guard let json = try? JSONDecoder().decode(JSONAny.self, from: Data(text.utf8)) else {
            throw Failure(message: "the answer isn't JSON")
        }
        guard let s = json["data"]?["__schema"], case .object = s else {
            let errors = json["errors"]?.array?.compactMap { $0["message"]?.string } ?? []
            throw Failure(message: errors.isEmpty ? "the answer has no __schema (is introspection turned off?)" : errors.joined(separator: "; "))
        }
        var types: [GQLType] = []
        for t in s["types"]?.array ?? [] {
            guard let name = t["name"]?.string, !name.hasPrefix("__") else { continue }
            let fields = (t["fields"]?.array ?? t["inputFields"]?.array ?? []).compactMap { f -> GQLField? in
                guard let n = f["name"]?.string, let type = GQLTypeRef(f["type"]) else { return nil }
                let args = (f["args"]?.array ?? []).compactMap { a -> GQLArgument? in
                    guard let an = a["name"]?.string, let at = GQLTypeRef(a["type"]) else { return nil }
                    return GQLArgument(name: an, type: at)
                }
                return GQLField(name: n, description: f["description"]?.string, type: type, args: args, deprecated: f["isDeprecated"]?.bool ?? false)
            }
            types.append(GQLType(name: name, kind: t["kind"]?.string ?? "OBJECT", description: t["description"]?.string, fields: fields,
                                 enumValues: (t["enumValues"]?.array ?? []).compactMap { $0["name"]?.string },
                                 possibleTypes: (t["possibleTypes"]?.array ?? []).compactMap { $0["name"]?.string }))
        }
        return GraphQLSchema(queryType: s["queryType"]?["name"]?.string, mutationType: s["mutationType"]?["name"]?.string,
                             subscriptionType: s["subscriptionType"]?["name"]?.string, types: types)
    }

    // MARK: Suggestions

    struct Suggestion: Hashable, Sendable, Identifiable {
        var name: String
        var detail: String
        var id: String { name }
    }

    /// Where the cursor is: the type whose fields fit there and the word being typed.
    struct Context: Equatable {
        var type: String?
        /// The letters before the cursor (the part a suggestion replaces).
        var prefix: String
    }

    /// The selection set the cursor (`offset`, in characters) is in: `{` opens the type of the
    /// field before it (or the operation's root type, or a fragment's `on Type`), `}` closes it;
    /// strings, comments and argument lists are skipped.
    func context(in query: String, at offset: Int) -> Context {
        let chars = Array(query.prefix(max(0, offset)))
        var stack: [String?] = []
        var words: [String] = []
        var parens = 0
        var i = 0
        var pendingOn = false
        var onType: String?
        func top() -> String? { stack.last ?? nil }
        while i < chars.count {
            let c = chars[i]
            if c == "#" {
                while i < chars.count && chars[i] != "\n" { i += 1 }
                continue
            }
            if c == "\"" {
                i += 1
                while i < chars.count && chars[i] != "\"" { if chars[i] == "\\" { i += 1 }; i += 1 }
                i += 1
                continue
            }
            if c.isLetter || c == "_" {
                var w = ""
                while i < chars.count && (chars[i].isLetter || chars[i].isNumber || chars[i] == "_") { w.append(chars[i]); i += 1 }
                if parens == 0 {
                    if pendingOn { onType = w; pendingOn = false } else if w == "on" { pendingOn = true } else { words.append(w) }
                }
                continue
            }
            switch c {
            case "(": parens += 1
            case ")": parens = max(0, parens - 1)
            case "{" where parens == 0:
                var next: String?
                if let t = onType {
                    next = t
                } else if stack.isEmpty {
                    let op = words.first { ["query", "mutation", "subscription"].contains($0) }
                    next = op == "mutation" ? mutationType : op == "subscription" ? subscriptionType : queryType
                } else if let field = words.last {
                    next = type(top())?.fields.first { $0.name == field }?.type.named
                }
                stack.append(next)
                words = []
                onType = nil
            case "}" where parens == 0:
                if !stack.isEmpty { stack.removeLast() }
                words = []
                onType = nil
            default: break
            }
            i += 1
        }
        // The word being typed (letters right before the cursor).
        var prefix = ""
        var j = chars.count - 1
        while j >= 0 && (chars[j].isLetter || chars[j].isNumber || chars[j] == "_") { prefix = String(chars[j]) + prefix; j -= 1 }
        if parens > 0 || stack.isEmpty { return Context(type: nil, prefix: prefix) }
        return Context(type: top(), prefix: prefix)
    }

    /// Fields of the type at the cursor that start with the word being typed (all of them right
    /// after `{` or a space), `__typename` last.
    func suggestions(in query: String, at offset: Int, limit: Int = 12) -> [Suggestion] {
        let ctx = context(in: query, at: offset)
        guard let t = type(ctx.type) else { return [] }
        let p = ctx.prefix.lowercased()
        var out = t.fields.filter { !$0.deprecated && (p.isEmpty || $0.name.lowercased().hasPrefix(p)) && $0.name != ctx.prefix }
            .map { Suggestion(name: $0.name, detail: $0.type.display) }
        if t.kind == "UNION" { out = [] }
        if p.isEmpty || "__typename".hasPrefix(p) { out.append(Suggestion(name: "__typename", detail: "String!")) }
        return Array(out.prefix(limit))
    }

    /// `query` with the word being typed at `offset` replaced by `name` (and a space after it);
    /// the new text and the cursor after the inserted name.
    static func apply(_ name: String, to query: String, at offset: Int) -> (text: String, cursor: Int) {
        let chars = Array(query)
        let end = min(max(0, offset), chars.count)
        var start = end
        while start > 0 && (chars[start - 1].isLetter || chars[start - 1].isNumber || chars[start - 1] == "_") { start -= 1 }
        let text = String(chars[..<start]) + name + String(chars[end...])
        return (text, start + name.count)
    }

    /// The introspection query (graphql-js `getIntrospectionQuery()` without descriptions of
    /// directives: what the browser and the suggestions need).
    static let introspectionQuery = """
    query IntrospectionQuery {
      __schema {
        queryType { name }
        mutationType { name }
        subscriptionType { name }
        types { ...FullType }
      }
    }

    fragment FullType on __Type {
      kind
      name
      description
      fields(includeDeprecated: true) {
        name
        description
        args { name type { ...TypeRef } }
        type { ...TypeRef }
        isDeprecated
      }
      inputFields { name description type { ...TypeRef } }
      enumValues(includeDeprecated: true) { name }
      possibleTypes { name }
    }

    fragment TypeRef on __Type {
      kind
      name
      ofType { kind name ofType { kind name ofType { kind name ofType { kind name ofType { kind name ofType { kind name ofType { kind name } } } } } } }
    }
    """

    /// The request that asks `r`'s server for its schema: same URL, headers and auth, POST, the
    /// introspection query, no scripts (the web app's `fetchSchema` source).
    static func introspectionRequest(for r: SavedRequest) -> SavedRequest {
        var q = r
        q.method = "POST"
        q.kind = nil
        q.bodyMode = "raw"
        q.bodyType = "graphql"
        q.body = introspectionQuery
        q.graphqlVariables = ""
        q.graphqlOperationName = "IntrospectionQuery"
        q.preRequestScript = ""
        q.postRequestScript = ""
        return q
    }
}
