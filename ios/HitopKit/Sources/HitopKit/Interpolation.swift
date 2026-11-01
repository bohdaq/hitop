import Foundation

/// `${name}` substitution, one pass (a value isn't scanned again), as
/// hitop-core's `interpolation.rs` (`\$\{([^}]+)\}`).
public enum Interpolation {
    public struct MissingVariable: Error, CustomStringConvertible, LocalizedError, Sendable {
        public let name: String
        public var description: String { "variable '\(name)' is not defined" }
        public var errorDescription: String? { description }
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
            if let v = variables[name] { return v }
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
            if let v = variables[name] { return v }
            if missing == nil { missing = name }
            return whole
        }
        if let missing { throw MissingVariable(name: missing) }
        return out
    }
}
