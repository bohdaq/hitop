import Foundation
import HitopKit

/// The UI tests' and screenshots' shared collections (I2b): Grace, another person with her own
/// phone, runs in this process against `FakeCloud` with the app's own `SharedSync`, so what the
/// app receives is made and sealed by the real client code. Her device key is fixed, so the
/// invite code the UI test types is known: `inviteCode`.
@MainActor
enum SharedScenario {
    struct FixedKeyMaker: DeviceKeyMaker {
        let key: DeviceKey
        func make() -> DeviceKey { key }
    }

    static let me = "ada@example.com"
    static let peerEmail = "grace@example.com"
    static let peerSecret = CloudCrypto.base64url((0..<32).map { UInt8(0x40 + $0) })
    static let inviteToken = "TEAMINVITE"

    static var peerKey: DeviceKey { DeviceKey(secret: peerSecret, publicKey: (try? SyncCore.publicKey(secret: peerSecret)) ?? "") }
    static var peerFingerprint: String { (try? SyncCore.fingerprint(peerKey.publicKey)) ?? "" }
    /// What Grace sends Ada: `<token>.<her device's fingerprint>`.
    static var inviteCode: String { "\(inviteToken).\(SyncCore.normalize(fingerprint: peerFingerprint))" }

    /// Grace's phone (kept alive while the app runs).
    nonisolated(unsafe) static var peer: (cloud: HitopCloud, model: AppModel, shared: SharedSync)?

    static func makePeer(_ fake: FakeCloud) -> (cloud: HitopCloud, model: AppModel, shared: SharedSync) {
        let model = AppModel(store: MemoryLibraryStore(), vault: MemoryVault())
        model.load()
        let cloud = HitopCloud(transport: fake, credentials: CloudCredentials(store: MemorySecretStore()),
                               settingsStore: MemoryCloudSettingsStore(CloudSettings(server: FakeCloud.server)), keyMaker: FixedKeyMaker(key: peerKey), deviceName: "Grace's Mac")
        let shared = SharedSync(cloud: cloud, model: model, store: MemorySharedSyncStore())
        shared.pushDelay = .seconds(3600)
        let p = (cloud, model, shared)
        peer = p
        return p
    }

    static func signIn(_ cloud: HitopCloud, _ email: String) async throws {
        try await cloud.startEmailSignIn(server: FakeCloud.server, email: email)
        try await cloud.finishEmailSignIn("mail-\(email)")
        _ = try await cloud.loadOrgs()
    }

    /// Grace's "Team API" in Acme (or another collection), shared from her phone.
    static func peerCollection(_ p: (cloud: HitopCloud, model: AppModel, shared: SharedSync), _ name: String) async throws -> UInt64 {
        var c = SavedCollection(name: name, variables: ["baseUrl": "https://api.acme.test"])
        c.org = "org-acme"
        c.requests = name == "Team API" ? [
            SavedRequest(name: "List orders", url: "${baseUrl}/orders", method: "GET"),
            SavedRequest(name: "Create order", url: "${baseUrl}/orders", method: "POST", body: "{\n  \"item\": \"book\"\n}", bodyType: "json"),
            SavedRequest(name: "Get order", url: "${baseUrl}/orders/42", method: "GET"),
        ] : [
            SavedRequest(name: "Partner status", url: "${baseUrl}/partner/status", method: "GET"),
            SavedRequest(name: "Partner payouts", url: "${baseUrl}/partner/payouts", method: "GET"),
        ]
        p.model.applyRemote(collection: c)
        try await p.shared.share(c.id)
        return c.id
    }

    /// Grace invites Ada and adds her as soon as she accepted (checking her device's key, as an
    /// owner does by comparing fingerprints).
    static func addWhenAccepted(_ p: (cloud: HitopCloud, model: AppModel, shared: SharedSync), _ id: UInt64, role: String, attempts: Int = 600) async throws {
        for _ in 0..<attempts {
            let info = try await p.shared.members(id)
            if let a = info.invites.first(where: { $0.acceptedBy != nil })?.acceptedBy {
                p.shared.trust(a.keys)
                let keys = a.keys.map { k in var t = k; t.trust = p.shared.trustOf(k.id, k.publicKey); return t }
                try await p.shared.addMember(id, email: a.email, role: role, keys: keys)
                return
            }
            try await Task.sleep(for: .milliseconds(500))
        }
    }

    /// `-UITestSharedInvite`: Ada is signed in here; Grace shares Team API and sends the code
    /// (`inviteCode`); once Ada accepts it, Grace adds her as an editor.
    static func invite(app cloud: HitopCloud, fake: FakeCloud) async throws {
        fake.fixedInviteToken = inviteToken
        try await signIn(cloud, me)
        cloud.selectOrg("org-acme")
        let p = makePeer(fake)
        try await signIn(p.cloud, peerEmail)
        let id = try await peerCollection(p, "Team API")
        _ = try await p.shared.invite(id, email: me, role: "editor")
        Task { try? await addWhenAccepted(p, id, role: "editor") }
    }

    /// `-UITestSharedJoined`: Ada already has Grace's Team API (editor) and Partner API (viewer)
    /// open here; Grace then changes "List orders" in Team API, held back by the server until
    /// Ada writes too (so Ada's next edit of it meets a conflict).
    static func joined(app cloud: HitopCloud, shared: SharedSync, fake: FakeCloud) async throws {
        try await signIn(cloud, me)
        cloud.selectOrg("org-acme")
        let p = makePeer(fake)
        try await signIn(p.cloud, peerEmail)
        for (name, role) in [("Team API", "editor"), ("Partner API", "viewer")] {
            let id = try await peerCollection(p, name)
            fake.fixedInviteToken = "\(inviteToken)\(role)"
            let code = try await p.shared.invite(id, email: me, role: role).code
            _ = try await shared.accept(try InviteCode.parse(code))
            try await addWhenAccepted(p, id, role: role, attempts: 1)
            try await shared.refreshWorkspaces()
            guard let ws = p.shared.link(id)?.workspace else { continue }
            try await shared.join(ws)
        }
        fake.fixedInviteToken = nil
        guard let team = p.model.collections.first(where: { $0.name == "Team API" }), var r = team.requests.first(where: { $0.name == "List orders" }) else { return }
        fake.holdWritesFrom = peerEmail
        r.url = "${baseUrl}/orders?status=open"
        p.model.save(r)
        try await p.shared.syncNow(team.id)
    }
}
