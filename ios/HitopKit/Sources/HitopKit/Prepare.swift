import Foundation

/// Variables in two layers: the globals, and everything above them
/// (collection, environment, data row, run values, script-set values,
/// secrets). A name in `upper` hides the global of that name.
public struct Scopes: Sendable, Equatable {
    public var globals: [String: String]
    public var upper: [String: String]

    public init(globals: [String: String] = [:], upper: [String: String] = [:]) {
        self.globals = globals
        self.upper = upper
    }

    /// Every variable, the upper layer winning.
    public var merged: [String: String] { globals.merging(upper) { _, u in u } }

    mutating func apply(_ variables: VariableUpdates, _ globals: VariableUpdates) {
        Scopes.apply(&upper, variables)
        Scopes.apply(&self.globals, globals)
    }

    static func apply(_ map: inout [String: String], _ updates: VariableUpdates) {
        for (k, v) in updates {
            if let v { map[k] = v } else { map.removeValue(forKey: k) }
        }
    }
}

/// A request as it is sent: variables filled in, auth applied, then the
/// pre-request script run, so a script can still override the auth
/// (hitop-core `prepare.rs`); the URL then gets a scheme if it has none.
enum Prepare {
    static func variables(collection: [String: String], environment: Environment?) -> [String: String] {
        var vars = collection
        for (k, v) in environment?.variables ?? [:] { vars[k] = v }
        return vars
    }

    typealias Built = (url: String, headers: [(String, String)], body: String)

    /// URL, enabled headers and body filled in, then the auth. A GraphQL body
    /// whose variables aren't JSON is an error unless `deferred()` (a
    /// variable is still missing): its variables are then left out for now.
    static func build(_ r: Request, auth: Auth?, _ fill: (String) throws -> String, deferred: () -> Bool) throws -> Built {
        let url = try fill(r.url)
        var headers = try r.headers.filter { $0.enabled && !$0.name.isEmpty }.map { (try fill($0.name), try fill($0.value)) }
        let body: String
        if r.isGraphQL {
            let g = r.graphql ?? GraphQL()
            let (query, variables, name) = (try fill(r.body), try fill(g.variables), try fill(g.operationName))
            do {
                body = try GraphQLBody.requestBody(query: query, variables: variables, operationName: name)
            } catch where deferred() {
                body = try GraphQLBody.requestBody(query: query, variables: "", operationName: name)
            }
            if !headers.contains(where: { $0.0.lowercased() == "content-type" }) {
                headers.append(("Content-Type", GraphQLBody.contentType))
            }
        } else {
            body = try fill(r.body)
        }
        let (u, h) = try AuthApply.apply(auth, url: url, headers: headers, fill)
        return (u, h, body)
    }

    /// The prepared request. Without an engine a pre-request script is not
    /// run (`scriptSkipped`).
    static func prepare(_ r: Request, auth: Auth?, scopes: Scopes, context: [String: JSONValue], engine: ScriptEngine?) throws -> (PreRequestResult, scriptSkipped: Bool) {
        let variables = scopes.merged
        // A `${name}` with no value yet is kept for the script: a Postman
        // pre-request script often sets the variable the same request uses.
        var missing: [String] = []
        let first = try build(r, auth: auth, { Interpolation.lenient($0, variables, missing: &missing) }, deferred: { !missing.isEmpty })
        let hasScript = !r.preRequestScript.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        guard let engine, hasScript else {
            if let name = missing.first { throw Interpolation.MissingVariable(name: name) }
            return (PreRequestResult(url: try Sending.withScheme(first.url), headers: first.headers, body: first.body, contextUpdates: [:], variables: [:], globals: [:]), hasScript)
        }
        var pre = try Scripts.runPreRequest(r.preRequestScript, PreRequestInput(
            url: first.url, method: r.method, headers: first.headers, body: first.body,
            variables: variables, context: context, globals: scopes.globals), engine: engine)
        if pre.variables.isEmpty && pre.globals.isEmpty {
            if let name = missing.first { throw Interpolation.MissingVariable(name: name) }
            pre.url = try Sending.withScheme(pre.url)
            return (pre, false)
        }
        // The script set variables: what it left as built is built again with
        // them; what it changed only gets the missing ones filled in.
        var after = scopes
        after.apply(pre.variables, pre.globals)
        let vars = after.merged
        let strict = try build(r, auth: auth, { try Interpolation.interpolate($0, vars) }, deferred: { false })
        pre.url = pre.url == first.url ? strict.url : try Interpolation.fill(pre.url, names: missing, vars)
        pre.body = pre.body == first.body ? strict.body : try Interpolation.fill(pre.body, names: missing, vars)
        if pre.headers.elementsEqual(first.headers, by: { $0 == $1 }) {
            pre.headers = strict.headers
        } else {
            pre.headers = try pre.headers.map { (try Interpolation.fill($0.0, names: missing, vars), try Interpolation.fill($0.1, names: missing, vars)) }
        }
        pre.url = try Sending.withScheme(pre.url)
        return (pre, false)
    }
}
