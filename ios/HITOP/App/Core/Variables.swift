import Foundation

/// Where `${name}` gets its value, in the order hitop applies scopes (CLAUDE.md, "Globals"):
/// globals < collection < environment < script-set < secrets. Secret values come from the vault.
struct VariableContext: Sendable {
    var globals: [String: String] = [:]
    var globalSecrets: [String: String] = [:]
    var collection: [String: String] = [:]
    var environmentName: String?
    var environment: [String: String] = [:]
    var environmentSecrets: [String: String] = [:]
    /// Secret names (with or without a value on this device).
    var secretNames: Set<String> = []
    var scriptSet: [String: String] = [:]

    struct Source: Equatable, Sendable {
        var value: String
        var scope: String
        var secret: Bool
        /// A secret with no value on this device.
        var missing: Bool
    }

    /// Every value, the higher scope winning.
    var merged: [String: String] {
        var out = globals
        for (k, v) in globalSecrets { out[k] = v }
        for (k, v) in collection { out[k] = v }
        for (k, v) in environment { out[k] = v }
        for (k, v) in scriptSet { out[k] = v }
        for (k, v) in environmentSecrets { out[k] = v }
        return out
    }

    /// Where `name` gets its value; secret values are returned as they are (the view masks them).
    func source(_ name: String) -> Source? {
        if name.hasPrefix("secret:") { return Source(value: "", scope: "secret manager reference", secret: true, missing: true) }
        let envLabel = environmentName.map { "environment \($0)" } ?? "environment"
        if let v = environmentSecrets[name], !v.isEmpty { return Source(value: v, scope: envLabel, secret: true, missing: false) }
        if let v = scriptSet[name] { return Source(value: v, scope: "set by script", secret: false, missing: false) }
        if let v = environment[name] { return Source(value: v, scope: envLabel, secret: false, missing: false) }
        if let v = collection[name] { return Source(value: v, scope: "collection", secret: false, missing: false) }
        if let v = globalSecrets[name], !v.isEmpty { return Source(value: v, scope: "globals", secret: true, missing: false) }
        if let v = globals[name] { return Source(value: v, scope: "globals", secret: false, missing: false) }
        if secretNames.contains(name) { return Source(value: "", scope: envLabel, secret: true, missing: true) }
        return nil
    }

    /// The names a text uses that have no value.
    func missing(in text: String) -> [String] {
        VariableText.names(text).filter { source($0).map { $0.missing } ?? true }
    }
}
