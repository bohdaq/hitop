import Foundation

/// `${name}` substitution, one pass (a value isn't scanned again), as
/// hitop-core's `interpolation.rs` (`\$\{([^}]+)\}`).
public enum Interpolation {
    public struct MissingVariable: Error, CustomStringConvertible, LocalizedError, Sendable {
        public let name: String
        /// A plugin call (`${plugin.name(args)}`, spec section 5c) can't run
        /// here: the request is refused with the plugin's name, as hitop-core
        /// refuses it where no plugin host is installed.
        public var description: String {
            if let plugin = Interpolation.plugin(name) {
                return "this request uses plugin '\(plugin)', which can't run here: plugins run only where they are installed (hitop CLI: hitop plugin install <file>)"
            }
            return "variable '\(name)' is not defined"
        }
        public var errorDescription: String? { description }
    }

    /// The plugin a `${...}`'s inner text calls (`plugin.name(args)`), or nil.
    public static func plugin(_ inner: String) -> String? {
        let t = inner.trimmingCharacters(in: .whitespaces)
        guard t.hasPrefix("plugin."), t.hasSuffix(")"), let open = t.firstIndex(of: "(") else { return nil }
        let name = t[t.index(t.startIndex, offsetBy: 7)..<open]
        guard let first = name.first, first.isASCII, first.isLowercase, name.count <= 64,
              name.allSatisfy({ $0.isASCII && ($0.isLowercase || $0.isNumber || $0 == "-" || $0 == "_") }) else { return nil }
        return String(name)
    }

    /// The first plugin call in a text (a variable's value may hold one), as
    /// its inner text, or nil.
    public static func pluginCall(in text: String) -> String? {
        guard text.contains("${plugin.") else { return nil }
        var found: String?
        _ = scan(text) { name, whole in
            if found == nil, plugin(name) != nil { found = name }
            return whole
        }
        return found
    }

    /// Throws the refusal when a text still holds a plugin call: one is never sent as text.
    public static func refusePlugins(_ texts: [String]) throws {
        for t in texts {
            if let call = pluginCall(in: t) { throw MissingVariable(name: call) }
        }
    }

    /// Calls `replace` for every `${name}` and joins the pieces.
    static func scan(_ template: String, _ replace: (String, String) throws -> String) rethrows -> String {
        guard template.contains("${") else { return template }
        var out = ""
        var rest = template[...]
        while let start = rest.range(of: "${") {
            out += rest[..<start.lowerBound]
            let after = rest[start.upperBound...]
            if let close = after.firstIndex(of: "}"), close > after.startIndex {
                let name = String(after[..<close])
                out += try replace(name, "${\(name)}")
                rest = after[after.index(after: close)...]
            } else {
                // No name: the `$` is kept and the scan goes on after it.
                out += "$"
                rest = rest[rest.index(after: start.lowerBound)...]
            }
        }
        out += rest
        return out
    }

    /// Every `${name}` replaced; one without a value is an error naming it
    /// (the last one, as hitop-core does).
    public static func interpolate(_ template: String, _ variables: [String: String]) throws -> String {
        var missing: String?
        let out = scan(template) { name, whole in
            if let v = variables[name] {
                if let call = pluginCall(in: v) { missing = call }
                return v
            }
            missing = name
            return whole
        }
        if let missing { throw MissingVariable(name: missing) }
        return out
    }

    /// Like `interpolate`, but a `${name}` without a value stays as written
    /// and its name is added to `missing` (once, in order).
    public static func lenient(_ template: String, _ variables: [String: String], missing: inout [String]) -> String {
        var found = missing
        let out = scan(template) { name, whole in
            if let v = variables[name] { return v }
            if !found.contains(name) { found.append(name) }
            return whole
        }
        missing = found
        return out
    }

    /// Replaces only the `${name}`s whose name is in `names`; one that still
    /// has no value is an error.
    public static func fill(_ template: String, names: [String], _ variables: [String: String]) throws -> String {
        var missing: String?
        let out = scan(template) { name, whole in
            guard names.contains(name) else { return whole }
            if let v = variables[name] {
                if missing == nil, let call = pluginCall(in: v) { missing = call }
                return v
            }
            if missing == nil { missing = name }
            return whole
        }
        if let missing { throw MissingVariable(name: missing) }
        return out
    }
}
