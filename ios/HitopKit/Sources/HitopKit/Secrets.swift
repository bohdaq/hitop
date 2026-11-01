import Foundation
#if canImport(Security)
import Security
#endif

/// Secret variables (spec section 5). An environment lists the names of its
/// secrets; their values never go into files, reports or the run status. At
/// run time each is resolved, first match wins:
///
/// 1. the `HITOP_SECRET_<ENV>_<NAME>` environment variable (tests, macOS
///    command line; an iOS app has no environment),
/// 2. values the caller passes (the CLI's `--secret NAME=value`),
/// 3. the keychain: service `hitop`, account `<workspace>/<env>/<name>`.
///
/// There is no prompt: the app asks for missing values itself and stores them.
public enum SecretNames {
    public static let service = "hitop"
    /// What a secret value is replaced with in output.
    public static let mask = "••••"
    /// The environment name the globals' secrets go by.
    public static let globalsEnvironment = "global"

    public static func account(workspace: String, environment: String, name: String) -> String {
        "\(workspace)/\(environment)/\(name)"
    }

    /// `HITOP_SECRET_<ENV>_<NAME>`: upper case, every character other than an
    /// ASCII letter or digit turned into `_`.
    public static func envVar(environment: String, name: String) -> String {
        func part(_ s: String) -> String {
            String(s.unicodeScalars.map { $0.isASCII && ($0.properties.isAlphabetic || ("0"..."9").contains($0)) ? Character(String($0).uppercased()) : "_" })
        }
        return "HITOP_SECRET_\(part(environment))_\(part(name))"
    }
}

/// Why a keychain call failed.
public enum SecretStoreError: Error, CustomStringConvertible, Sendable {
    /// No keychain here.
    case unavailable(String)
    case failed(String)

    public var description: String {
        switch self {
        case let .unavailable(why): return "no keychain available: \(why)"
        case let .failed(why): return "keychain error: \(why)"
        }
    }
}

/// Where secret values are kept.
public protocol SecretStore: Sendable {
    func get(account: String) throws -> String?
    func set(account: String, value: String) throws
    /// True when there was something to delete.
    @discardableResult func delete(account: String) throws -> Bool
}

/// In memory: tests, and Linux, where there is no keychain.
public final class MemorySecretStore: SecretStore, @unchecked Sendable {
    private var values: [String: String] = [:]
    private let lock = NSLock()

    public init(_ values: [String: String] = [:]) { self.values = values }

    public func get(account: String) throws -> String? { lock.withLock { values[account] } }
    public func set(account: String, value: String) throws { lock.withLock { values[account] = value } }
    public func delete(account: String) throws -> Bool { lock.withLock { values.removeValue(forKey: account) != nil } }
}

/// A store that is never there.
public struct NoSecretStore: SecretStore {
    public let why: String
    public init(_ why: String) { self.why = why }
    public func get(account: String) throws -> String? { throw SecretStoreError.unavailable(why) }
    public func set(account: String, value: String) throws { throw SecretStoreError.unavailable(why) }
    public func delete(account: String) throws -> Bool { throw SecretStoreError.unavailable(why) }
}

#if canImport(Security)
/// The iOS or macOS keychain: generic passwords, service `hitop`. On iOS the
/// items belong to the app (or its `accessGroup`), so they aren't shared
/// with the `hitop` CLI on a Mac.
public struct KeychainSecretStore: SecretStore {
    public let accessGroup: String?

    public init(accessGroup: String? = nil) { self.accessGroup = accessGroup }

    private func query(_ account: String) -> [String: Any] {
        var q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: SecretNames.service,
            kSecAttrAccount as String: account,
        ]
        if let accessGroup { q[kSecAttrAccessGroup as String] = accessGroup }
        return q
    }

    private func error(_ status: OSStatus) -> SecretStoreError {
        let text = (SecCopyErrorMessageString(status, nil) as String?) ?? "OSStatus \(status)"
        return status == errSecNotAvailable || status == errSecInteractionNotAllowed ? .unavailable(text) : .failed(text)
    }

    public func get(account: String) throws -> String? {
        var q = query(account)
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: CFTypeRef?
        let status = SecItemCopyMatching(q as CFDictionary, &out)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = out as? Data else { throw error(status) }
        return String(decoding: data, as: UTF8.self)
    }

    public func set(account: String, value: String) throws {
        let data = Data(value.utf8)
        let status = SecItemUpdate(query(account) as CFDictionary, [kSecValueData as String: data] as CFDictionary)
        if status == errSecItemNotFound {
            var q = query(account)
            q[kSecValueData as String] = data
            q[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
            let added = SecItemAdd(q as CFDictionary, nil)
            guard added == errSecSuccess else { throw error(added) }
        } else if status != errSecSuccess {
            throw error(status)
        }
    }

    public func delete(account: String) throws -> Bool {
        let status = SecItemDelete(query(account) as CFDictionary)
        if status == errSecItemNotFound { return false }
        guard status == errSecSuccess else { throw error(status) }
        return true
    }
}
#endif

/// The keychain on Apple platforms, else a store that says there is none.
public func platformSecretStore(accessGroup: String? = nil) -> SecretStore {
    #if canImport(Security)
    return KeychainSecretStore(accessGroup: accessGroup)
    #else
    return NoSecretStore("this platform has no keychain; use HITOP_SECRET_* environment variables")
    #endif
}

/// Where a secret's value came from.
public enum SecretOrigin: String, Sendable {
    case envVar = "env var"
    case given = "given"
    case keychain = "keychain"
}

/// Resolved secret values, by variable name.
public struct Secrets: Sendable {
    public private(set) var values: [String: (value: String, origin: SecretOrigin)] = [:]

    public init() {}

    public var isEmpty: Bool { values.isEmpty }
    public subscript(name: String) -> String? { values[name]?.value }
    public var names: [String] { values.keys.sorted() }

    public mutating func insert(_ name: String, _ value: String, origin: SecretOrigin) {
        values[name] = (value, origin)
    }

    /// The secrets over the variables of the same names.
    public func apply(to variables: inout [String: String]) {
        for (k, v) in values { variables[k] = v.value }
    }

    public func masker(with other: Secrets = Secrets()) -> Masker {
        Masker(values.values.map(\.value) + other.values.values.map(\.value))
    }
}

/// Missing secrets: the run can't start (the CLI's exit code 2).
public struct MissingSecrets: Error, CustomStringConvertible, LocalizedError, Sendable {
    /// Variable names, sorted.
    public let names: [String]
    public let environment: String
    public let note: String?

    public var description: String {
        let s = names.count == 1 ? "" : "s"
        var text = "no value for secret\(s) \(names.joined(separator: ", ")) (\(environment)): set it in the app, or HITOP_SECRET_* environment variables"
        if let note { text += " (\(note))" }
        return text
    }

    public var errorDescription: String? { description }
}

/// Resolves the secrets of `environment` plus the `given` names, in the order
/// of `SecretNames`.
public func resolveSecrets(workspace: String, environment: Environment?, given: [String: String] = [:], store: SecretStore,
                           lookupEnv: (String) -> String? = { ProcessInfo.processInfo.environment[$0] }) throws -> (secrets: Secrets, notes: [String]) {
    var names = Set(environment?.secrets ?? [])
    names.formUnion(given.keys)
    var secrets = Secrets()
    var notes: [String] = []
    var keychainDown: String?
    var missing: [String] = []
    for name in names.sorted() {
        if let env = environment?.name, let v = lookupEnv(SecretNames.envVar(environment: env, name: name)) {
            secrets.insert(name, v, origin: .envVar)
            continue
        }
        if let v = given[name] {
            secrets.insert(name, v, origin: .given)
            continue
        }
        if let env = environment?.name, keychainDown == nil {
            do {
                if let v = try store.get(account: SecretNames.account(workspace: workspace, environment: env, name: name)) {
                    secrets.insert(name, v, origin: .keychain)
                    continue
                }
            } catch {
                keychainDown = "\(error)"
                notes.append("\(error); secrets come from HITOP_SECRET_* environment variables instead")
            }
        }
        missing.append(name)
    }
    if !missing.isEmpty {
        throw MissingSecrets(names: missing, environment: environment?.name ?? "no environment", note: keychainDown)
    }
    return (secrets, notes)
}

/// Replaces secret values with `••••` in anything that leaves the run:
/// reports, results, the run status. Each value is masked in its
/// percent-encoded form too (as it appears in a query string), longest first.
public struct Masker: Sendable {
    let values: [String]

    public init(_ values: [String]) {
        var all: [String] = []
        for v in values where !v.isEmpty {
            all.append(v)
            let enc = Masker.percentEncode(v)
            if enc != v { all.append(enc) }
        }
        all.sort { $0.utf8.count != $1.utf8.count ? $0.utf8.count > $1.utf8.count : $0.utf8.lexicographicallyPrecedes($1.utf8) }
        var seen = Set<String>()
        self.values = all.filter { seen.insert($0).inserted }
    }

    public var isEmpty: Bool { values.isEmpty }

    public func mask(_ text: String) -> String {
        var out = text
        for v in values where out.contains(v) {
            out = out.replacingOccurrences(of: v, with: SecretNames.mask)
        }
        return out
    }

    static func percentEncode(_ s: String) -> String {
        var out = ""
        for b in s.utf8 {
            switch b {
            case 0x41...0x5A, 0x61...0x7A, 0x30...0x39, 0x2D, 0x2E, 0x5F, 0x7E: out.unicodeScalars.append(Unicode.Scalar(b))
            default: out += String(format: "%%%02X", b)
            }
        }
        return out
    }
}

/// `${secret:vault:...}`, `${secret:aws:...}`, `${secret:azure:...}` and
/// `${secret:op:...}` references (spec section 5b). The companion can't reach
/// a secret manager, so a request that uses one fails with this error.
public struct SecretManagerReference: Error, CustomStringConvertible, LocalizedError, Sendable {
    public let reference: String
    public var description: String {
        "${\(reference)} is a secret manager reference; the iOS app can't resolve secret manager references (run this request with the hitop CLI or desktop app)"
    }
    public var errorDescription: String? { description }

    /// The first `${secret:...}` reference in `text`, its inner text.
    static func find(_ text: String) -> String? {
        guard text.contains("${secret:") else { return nil }
        var found: String?
        _ = Interpolation.scan(text) { name, whole in
            if found == nil, name.hasPrefix("secret:") { found = name }
            return whole
        }
        return found
    }
}
