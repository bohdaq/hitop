import Foundation
import HitopKit

/// Secret variable values: the Keychain in the app (HitopKit's `KeychainSecretStore`, service
/// `hitop`), memory in tests. Accounts: `ios/<environment id>/<name>`, globals `ios/global/<name>`.
protocol SecretVault: Sendable {
    func value(environment: String, name: String) -> String?
    func setValue(_ value: String?, environment: String, name: String)
    /// `value`, but a vault that can't be read now throws (it isn't "no value").
    func read(environment: String, name: String) throws -> String?
}

extension SecretVault {
    func read(environment: String, name: String) throws -> String? { value(environment: environment, name: name) }

    static func account(_ environment: String, _ name: String) -> String { "ios/\(environment)/\(name)" }
}

struct KeychainVault: SecretVault {
    private let store: SecretStore

    init(store: SecretStore = platformSecretStore()) { self.store = store }

    func value(environment: String, name: String) -> String? {
        (try? store.get(account: Self.account(environment, name))) ?? nil
    }

    func read(environment: String, name: String) throws -> String? { try store.get(account: Self.account(environment, name)) }

    func setValue(_ value: String?, environment: String, name: String) {
        let account = Self.account(environment, name)
        if let value, !value.isEmpty {
            try? store.set(account: account, value: value)
        } else {
            _ = try? store.delete(account: account)
        }
    }
}

final class MemoryVault: SecretVault, @unchecked Sendable {
    private let lock = NSLock()
    private var values: [String: String] = [:]

    init(_ values: [String: String] = [:]) { self.values = values }

    func value(environment: String, name: String) -> String? {
        lock.lock(); defer { lock.unlock() }
        return values[Self.account(environment, name)]
    }

    func setValue(_ value: String?, environment: String, name: String) {
        lock.lock(); defer { lock.unlock() }
        values[Self.account(environment, name)] = value
    }
}

/// The id the globals' secrets use in the vault.
let globalsVaultID = "global"
