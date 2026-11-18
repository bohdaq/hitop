import CHitopSync
import Foundation

/// hitop sync's client logic, the same Rust every hitop client runs (ios/HITOP/SyncFFI: hitop-core's
/// sync plan and collection folder format, hitop-sync-crypto's wraps, `accept_wraps`, blobs and
/// shared environment values), through one C call. Nothing cryptographic is written in Swift.
/// Binary values are base64url strings, as hitop-wasm passes them to the web app.
enum SyncCore {
    struct Failure: Error, LocalizedError, Equatable {
        var message: String
        var errorDescription: String? { message }
    }

    /// `op` with `args`: the result's `ok` value, or `Failure` with the Rust error text.
    static func call(_ op: String, _ args: [String: JSONAny] = [:]) throws -> JSONAny {
        let text = String(decoding: JSONAny.object(args).canonical, as: UTF8.self)
        let raw: String = op.withCString { o in
            text.withCString { a in
                guard let p = hitop_sync_call(o, a) else { return #"{"error":"no answer"}"# }
                defer { hitop_sync_free(p) }
                return String(cString: p)
            }
        }
        guard let j = try? JSONDecoder().decode(JSONAny.self, from: Data(raw.utf8)) else { throw Failure(message: "\(op): unreadable answer") }
        if let e = j["error"]?.string { throw Failure(message: e) }
        return j["ok"] ?? .null
    }

    static func string(_ op: String, _ args: [String: JSONAny] = [:]) throws -> String {
        guard let s = try call(op, args).string else { throw Failure(message: "\(op): not text") }
        return s
    }

    // MARK: Device keys

    static func keygen() -> DeviceKey {
        let k = (try? call("keygen")) ?? .null
        return DeviceKey(secret: k["secret"]?.string ?? "", publicKey: k["publicKey"]?.string ?? "")
    }

    static func publicKey(secret: String) throws -> String { try string("publicKey", ["secret": .string(secret)]) }

    /// "62bd a406 …": eight groups of four hex digits (hitop-sync-crypto `fingerprint`).
    static func fingerprint(_ publicKey: String) throws -> String { try string("fingerprint", ["publicKey": .string(publicKey)]) }

    /// Hex digits only, lower case.
    static func normalize(fingerprint: String) -> String {
        (try? string("normalizeFingerprint", ["fingerprint": .string(fingerprint)])) ?? ""
    }

    /// "62bda406…" → "62bd a406 …".
    static func grouped(_ fp: String) -> String {
        let n = normalize(fingerprint: fp)
        return stride(from: 0, to: n.count, by: 4).map { i in
            let s = n.index(n.startIndex, offsetBy: i)
            return String(n[s..<(n.index(s, offsetBy: 4, limitedBy: n.endIndex) ?? n.endIndex)])
        }.joined(separator: " ")
    }

    static func newKey() throws -> String { try string("newKey") }
    static func randomId() throws -> String { try string("randomId") }

    // MARK: Wraps

    static func sealWrap(secret: String, sender: String, recipient: String, recipientPublicKey: String, workspace: String, epoch: Int, head: Int, key: String) throws -> String {
        try string("sealWrap", ["secret": .string(secret), "senderKeyId": .string(sender), "recipientKeyId": .string(recipient),
                                "recipientPublicKey": .string(recipientPublicKey), "workspace": .string(workspace),
                                "epoch": .number(Double(epoch)), "head": .number(Double(head)), "key": .string(key)])
    }

    struct WrapHeader: Equatable {
        var legacy: Bool
        var sender: String?
        var head: Int?
    }

    static func wrapHeader(_ wrapped: String) throws -> WrapHeader {
        let h = try call("wrapHeader", ["wrapped": .string(wrapped)])
        return WrapHeader(legacy: h["legacy"]?.bool ?? true, sender: h["sender"]?.string, head: h["head"]?.int)
    }

    /// A refused key (`accept_wraps`'s error): kind untrustedSender, forged, keyChanged, rollback,
    /// legacyRefused, missingKey or malformed.
    struct KeyRefusal: Error, LocalizedError, Equatable {
        var workspace: String
        var kind: String
        var message: String
        var epoch: Int?
        var senderKeyId: String?
        var fingerprint: String?

        /// What a substituted key or a hidden rotation looks like: always a security warning.
        var security: Bool { ["forged", "keyChanged", "rollback"].contains(kind) }

        var errorDescription: String? {
            switch kind {
            case "missingKey": "This device has no key for the shared collection yet: a member opens it on their device to hand the key over."
            case "untrustedSender": message
            default: security ? "Security warning: \(message)" : message
            }
        }
    }

    struct Keyring: Equatable {
        /// Epoch → workspace key.
        var keys: [Int: String]
        var current: Int
        var notices: [String]
        var newPins: [String: String]
        var memory: JSONAny
        var legacy: Bool
    }

    /// hitop-sync-crypto `accept_wraps`: the keys this device may use, or `KeyRefusal`.
    static func acceptWraps(secret: String, keyId: String, workspace: String, currentEpoch: Int, wraps: [(epoch: Int, wrapped: String)],
                            pinned: [String: String], listed: [String: String], memory: JSONAny) throws -> Keyring {
        let r = try call("acceptWraps", [
            "secret": .string(secret), "keyId": .string(keyId), "workspace": .string(workspace), "currentEpoch": .number(Double(currentEpoch)),
            "wraps": .array(wraps.map { .object(["epoch": .number(Double($0.epoch)), "wrapped": .string($0.wrapped)]) }),
            "pinned": .object(pinned.mapValues(JSONAny.string)), "listed": .object(listed.mapValues(JSONAny.string)), "memory": memory,
        ])
        guard r["ok"]?.bool == true else {
            let e = r["error"] ?? .null
            throw KeyRefusal(workspace: workspace, kind: e["kind"]?.string ?? "malformed", message: e["message"]?.string ?? "the workspace key was refused",
                             epoch: e["epoch"]?.int, senderKeyId: e["senderKeyId"]?.string, fingerprint: e["fingerprint"]?.string)
        }
        var keys: [Int: String] = [:]
        for (e, k) in r["keys"]?.object ?? [:] { if let n = Int(e), let s = k.string { keys[n] = s } }
        return Keyring(keys: keys, current: r["current"]?.int ?? 0, notices: (r["notices"]?.array ?? []).compactMap { $0["message"]?.string },
                       newPins: (r["newPins"]?.object ?? [:]).compactMapValues(\.string), memory: r["memory"] ?? .object([:]), legacy: r["legacy"]?.bool ?? false)
    }

    // MARK: Blobs

    static func blobEpoch(_ envelope: String) throws -> Int {
        guard let n = try call("blobEpoch", ["envelope": .string(envelope)]).int else { throw Failure(message: "blobEpoch: not a number") }
        return n
    }

    /// One file sealed as blob `blob`, version `version`, under `epoch`.
    static func seal(key: String, workspace: String, blob: String, version: Int, epoch: Int, path: String, text: String) throws -> String {
        try string("seal", ["key": .string(key), "workspace": .string(workspace), "blob": .string(blob), "version": .number(Double(version)),
                            "epoch": .number(Double(epoch)), "path": .string(path), "text": .string(text)])
    }

    static func open(key: String, workspace: String, blob: String, version: Int, envelope: String) throws -> (path: String, text: String) {
        let r = try call("open", ["key": .string(key), "workspace": .string(workspace), "blob": .string(blob), "version": .number(Double(version)), "envelope": .string(envelope)])
        return (r["path"]?.string ?? "", r["text"]?.string ?? "")
    }

    // MARK: Collection folders

    /// A collection (and its folder's environments) → its files, as the CLI writes them.
    static func toFiles(_ collection: SavedCollection, environments: [JSONAny] = []) throws -> [String: String] {
        let c = String(decoding: try JSONEncoder().encode(collection), as: UTF8.self)
        let envs = String(decoding: JSONAny.array(environments).canonical, as: UTF8.self)
        let folder = "{\"collection\":\(c),\"environments\":\(envs)}"
        return (try call("toFiles", ["folder": .string(folder)]).object ?? [:]).compactMapValues(\.string)
    }

    /// Files → the collection they hold (`name` when hitop.yaml has none).
    static func readFiles(_ files: [String: String], name: String) throws -> SavedCollection {
        let text = try string("readFiles", ["files": .object(files.mapValues(JSONAny.string)), "name": .string(name)])
        struct Folder: Decodable { var collection: SavedCollection }
        return try JSONDecoder().decode(Folder.self, from: Data(text.utf8)).collection
    }

    /// What to write and remove in a folder holding `existing` so it holds `files`
    /// (hitop-core `plan_sync`: an existing `.yml` keeps its extension).
    static func planFolder(existing: [String], files: [String: String]) throws -> (write: [String: String], remove: [String]) {
        let r = try call("planFolder", ["existing": .array(existing.map(JSONAny.string)), "files": .object(files.mapValues(JSONAny.string))])
        return ((r["write"]?.object ?? [:]).compactMapValues(\.string), (r["remove"]?.array ?? []).compactMap(\.string))
    }

    static func isSyncedPath(_ path: String) -> Bool { (try? call("isSyncedPath", ["path": .string(path)]).bool) ?? false }
}
