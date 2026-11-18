import Foundation
import HitopKit
import XCTest
@testable import HITOP

/// The personal library sync (LibrarySync) against `FakeCloud`: pull, push, conflict copies,
/// tombstones, the offline queue across a relaunch, the first-link rules, secrets.
@MainActor
final class LibrarySyncTests: XCTestCase {
    var fake: FakeCloud!
    var store: MemoryLibraryStore!
    var vault: MemoryVault!
    var syncStore: MemoryLibrarySyncStateStore!
    var secrets: MemorySecretStore!
    var settings: MemoryCloudSettingsStore!
    var model: AppModel!
    var cloud: HitopCloud!
    var sync: LibrarySync!
    let ada = "ada@example.com"

    override func setUp() async throws {
        fake = FakeCloud()
        store = MemoryLibraryStore()
        vault = MemoryVault()
        syncStore = MemoryLibrarySyncStateStore()
        secrets = MemorySecretStore()
        settings = MemoryCloudSettingsStore(CloudSettings(server: FakeCloud.server))
        launch()
    }

    /// The app starting (again): the same files, Keychain and state.
    func launch() {
        model = AppModel(store: store, vault: vault)
        model.load()
        cloud = HitopCloud(transport: fake, credentials: CloudCredentials(store: secrets), settingsStore: settings)
        sync = LibrarySync(cloud: cloud, model: model, stateStore: syncStore)
        // The tests drive every round themselves.
        cloud.onAccountChange = nil
        sync.pushDelay = .seconds(3600)
        sync.retryDelay = .seconds(3600)
    }

    func signIn() async throws {
        let start = try await cloud.startBrowserSignIn("google", server: FakeCloud.server)
        let code = URLComponents(url: start.url, resolvingAgainstBaseURL: false)!.queryItems!.first { $0.name == "code" }!.value!
        try await cloud.finishBrowserSignIn(code)
        await sync.sync()
    }

    func serverCollections() -> [String] {
        fake.items(ada).filter { $0.kind == "collection" && !$0.deleted }.compactMap { $0.data["name"]?.string }.sorted()
    }

    /// A collection another device saved (the web app's JSON).
    func webCollection(_ id: UInt64, _ name: String, requests: [JSONAny] = []) -> JSONAny {
        .object(["id": .number(Double(id)), "name": .string(name), "requests": .array(requests), "variables": .object([:])])
    }

    // MARK: First link

    func testFirstLinkUploadsWhatIsHereButAnEmptyMyRequests() async throws {
        let shop = model.addCollection(name: "shop-api")
        _ = model.newRequest(in: shop.id)
        XCTAssertEqual(model.collections.map(\.name).sorted(), ["My requests", "shop-api"])
        try await signIn()
        XCTAssertEqual(serverCollections(), ["shop-api"])
        XCTAssertTrue(sync.isLocalOnly(model.myRequests().id))
        XCTAssertEqual(sync.badges[shop.id], .synced)
        XCTAssertEqual(sync.badges[model.myRequests().id], .local)
        // Something added to My requests later doesn't upload it either: it was here before the link.
        // "Save to HITOP Cloud" does.
        let mine = model.myRequests()
        await sync.upload(mine.id)
        XCTAssertEqual(serverCollections(), ["My requests", "shop-api"])
    }

    func testAnEmptyMyRequestsMakesWayForTheLibrarysOne() async throws {
        fake.write(ada, kind: "collection", id: "42", data: webCollection(42, "My requests", requests: [
            .object(["id": .number(7), "name": .string("GET /users"), "url": .string("https://api.example.com/users"), "method": .string("GET")]),
        ]))
        try await signIn()
        XCTAssertEqual(model.collections.filter { $0.name == "My requests" }.map(\.id), [42])
        XCTAssertEqual(serverCollections(), ["My requests"])
    }

    func testTheUntouchedSampleIsNotUploaded() {
        var sample = SavedCollection(name: LibraryRules.sampleName)
        for (name, s) in LibraryRules.sampleRequests {
            sample.requests.append(SavedRequest(name: name, url: s[1], method: s[0], body: s[2], preRequestScript: s[3], postRequestScript: s[4]))
        }
        XCTAssertTrue(LibraryRules.untouched(sample))
        sample.requests[0].url += "&x=1"
        XCTAssertFalse(LibraryRules.untouched(sample))
        XCTAssertTrue(LibraryRules.untouched(SavedCollection(name: "Default", requests: [SavedRequest()])))
        XCTAssertFalse(LibraryRules.untouched(SavedCollection(name: "My requests", variables: ["a": "1"])))
    }

    func testAnotherAccountLaterUploadsNothingFromHere() async throws {
        model.addCollection(name: "ada's")
        try await signIn()
        await cloud.signOut()
        sync.checkAccount()
        fake.signInEmail = "bea@example.com"
        model.addCollection(name: "made while signed out")
        try await signIn()
        XCTAssertTrue(fake.items("bea@example.com").filter { $0.kind == "collection" }.isEmpty)
        XCTAssertTrue(model.collections.allSatisfy { sync.badges[$0.id] == .local })
    }

    // MARK: Pull and push

    func testPullBringsOtherDevicesChangesAndPushSendsOurs() async throws {
        try await signIn()
        fake.write(ada, kind: "collection", id: "100", data: webCollection(100, "from the web", requests: [
            .object(["id": .number(1), "name": .string("List"), "url": .string("https://x.test/list"), "method": .string("GET"),
                     "auth": .object(["type": .string("oauth2"), "grantType": .string("client_credentials"), "tokenUrl": .string("https://x.test/token")]),
                     "examples": .array([.object(["name": .string("ok"), "status": .number(200)])])]),
        ]))
        await sync.pull()
        let c = try XCTUnwrap(model.collection(100))
        XCTAssertEqual(c.requests.first?.name, "List")
        XCTAssertEqual(sync.badges[100], .synced)
        // An edit here goes back whole, with what this app doesn't edit kept.
        var r = c.requests[0]
        r.name = "List all"
        model.save(r)
        XCTAssertEqual(sync.badges[100], .waiting)
        XCTAssertTrue(sync.dirty)
        await sync.flush()
        let item = try XCTUnwrap(fake.item(ada, kind: "collection", id: "100"))
        XCTAssertEqual(item.version, 2)
        let req = item.data["requests"]?.array?.first
        XCTAssertEqual(req?["name"]?.string, "List all")
        XCTAssertEqual(req?["auth"]?["type"]?.string, "oauth2")
        XCTAssertEqual(req?["auth"]?["tokenUrl"]?.string, "https://x.test/token")
        XCTAssertEqual(req?["examples"]?.array?.count, 1)
        XCTAssertEqual(sync.badges[100], .synced)
        XCTAssertFalse(sync.dirty)
        // Only what changed is sent; the next pull skips our own write.
        fake.clearLog()
        let cursor = sync.state.cursor!
        await sync.sync()
        XCTAssertEqual(fake.log, ["GET /v1/library?since=\(cursor)"])
        XCTAssertFalse(sync.dirty)
    }

    func testDeletesLeaveTombstonesBothWays() async throws {
        let mine = model.addCollection(name: "mine")
        _ = model.newRequest(in: mine.id)
        try await signIn()
        model.deleteCollection(mine.id)
        await sync.flush()
        XCTAssertEqual(fake.item(ada, kind: "collection", id: String(mine.id))?.deleted, true)

        fake.write(ada, kind: "collection", id: "200", data: webCollection(200, "theirs"))
        await sync.pull()
        XCTAssertNotNil(model.collection(200))
        fake.deleteItem(ada, kind: "collection", id: "200")
        await sync.pull()
        XCTAssertNil(model.collection(200))
        XCTAssertFalse(store.loadCollections().contains { $0.id == 200 })
    }

    func testATombstoneLeavesAlonePlacesCollectionThisDeviceNeverHadFromTheLibrary() async throws {
        let local = model.addCollection(name: "local")
        try await signIn()
        await cloud.signOut()
        fake.signInEmail = "bea@example.com"
        try await signIn()
        // Bea's library deletes an item with the same id: this device's own collection stays.
        fake.write("bea@example.com", kind: "collection", id: String(local.id), data: webCollection(local.id, "x"))
        fake.deleteItem("bea@example.com", kind: "collection", id: String(local.id))
        await sync.pull()
        XCTAssertNotNil(model.collection(local.id))
    }

    // MARK: Conflicts

    func testAConflictKeepsTheirsAndAThisIPhoneCopy() async throws {
        let shop = model.addCollection(name: "shop-api")
        let r = model.newRequest(in: shop.id)
        try await signIn()
        // Another browser changes it first...
        var theirs = try XCTUnwrap(fake.item(ada, kind: "collection", id: String(shop.id))).data.object!
        theirs["name"] = .string("shop-api v2")
        fake.write(ada, kind: "collection", id: String(shop.id), data: .object(theirs))
        // ...while this iPhone renames a request.
        var mine = try XCTUnwrap(model.request(r.id))
        mine.name = "Mine"
        model.save(mine)
        await sync.flush()
        let names = model.collections.map(\.name).sorted()
        XCTAssertEqual(names, ["My requests", "shop-api (this iPhone)", "shop-api v2"])
        XCTAssertEqual(model.collection(shop.id)?.name, "shop-api v2")
        let copy = try XCTUnwrap(model.collections.first { $0.name == "shop-api (this iPhone)" })
        XCTAssertEqual(copy.requests.first?.name, "Mine")
        XCTAssertNotEqual(copy.id, shop.id)
        XCTAssertTrue(sync.notices.first?.contains("shop-api (this iPhone)") ?? false)
        // The copy is synced like any other collection.
        XCTAssertEqual(serverCollections(), ["shop-api (this iPhone)", "shop-api v2"])
        XCTAssertEqual(sync.badges[copy.id], .synced)
    }

    func testADeleteMeetingAnEditBringsTheirsBack() async throws {
        let shop = model.addCollection(name: "shop-api")
        try await signIn()
        var theirs = try XCTUnwrap(fake.item(ada, kind: "collection", id: String(shop.id))).data.object!
        theirs["name"] = .string("still used")
        fake.write(ada, kind: "collection", id: String(shop.id), data: .object(theirs))
        model.deleteCollection(shop.id)
        await sync.flush()
        XCTAssertEqual(model.collection(shop.id)?.name, "still used")
        XCTAssertFalse(model.collections.contains { $0.name.contains("(this iPhone)") })
    }

    // MARK: Offline

    func testOfflineChangesWaitAndSurviveARelaunch() async throws {
        let shop = model.addCollection(name: "shop-api")
        try await signIn()
        fake.offline = true
        let r = model.newRequest(in: shop.id)
        model.renameRequest(r.id, to: "Made offline")
        let gone = model.addCollection(name: "to delete")
        await sync.push()
        model.deleteCollection(gone.id)
        await sync.push()
        XCTAssertNotNil(sync.status.error)
        XCTAssertTrue(sync.dirty)
        XCTAssertEqual(sync.badges[shop.id], .waiting)

        // The app quits and starts again, still offline: the change waits in the files.
        launch()
        XCTAssertTrue(cloud.signedIn)
        XCTAssertTrue(sync.checkAccount())
        XCTAssertTrue(sync.dirty)
        XCTAssertEqual(sync.badges[shop.id], .waiting)
        fake.offline = false
        await sync.sync()
        XCTAssertFalse(sync.dirty)
        XCTAssertNil(sync.status.error)
        let item = try XCTUnwrap(fake.item(ada, kind: "collection", id: String(shop.id)))
        XCTAssertEqual(item.data["requests"]?.array?.first?["name"]?.string, "Made offline")
        XCTAssertEqual(serverCollections(), ["shop-api"])
    }

    func testAServerErrorStopsTheRoundAndARefusalStaysHere() async throws {
        let shop = model.addCollection(name: "shop-api")
        try await signIn()
        fake.libraryFailure = 503
        model.renameCollection(shop.id, to: "shop")
        await sync.push()
        XCTAssertEqual(sync.badges[shop.id], .waiting)
        XCTAssertTrue(sync.status.error?.contains("503") ?? false)
        // 402: the plan's storage is full. It stays on this iPhone, and isn't retried until it changes.
        fake.libraryFailure = 402
        await sync.push()
        XCTAssertEqual(sync.badges[shop.id], .refused)
        XCTAssertTrue(sync.status.error?.contains("stays on this iPhone") ?? false)
        fake.libraryFailure = nil
        fake.clearLog()
        await sync.push()
        XCTAssertFalse(fake.log.contains { $0.hasPrefix("PUT /v1/library/collection") })
        model.renameCollection(shop.id, to: "shop 2")
        await sync.push()
        XCTAssertEqual(sync.badges[shop.id], .synced)
    }

    func testAnEndedSessionStopsAndSignsOut() async throws {
        try await signIn()
        CloudCredentials(store: secrets).save(FakeCloud.server, CloudSecret(session: "expired", secret: "x"))
        model.addCollection(name: "after")
        await sync.push()
        XCTAssertFalse(cloud.signedIn)
        XCTAssertNotNil(cloud.lastError)
    }

    // MARK: Environments, globals, secrets

    func testEnvironmentsAndGlobalsSyncWithSecretsApart() async throws {
        let staging = model.addEnvironment(name: "Staging")
        model.setVariable(staging, name: "baseUrl", value: "https://staging.test", secret: false)
        model.setVariable(model.environments[0], name: "apiKey", value: "sk_live_1", secret: true)
        model.setActiveEnvironment(staging.id)
        model.setVariable(model.globals, name: "token", value: "g-secret", secret: true)
        try await signIn()
        let envs = try XCTUnwrap(fake.item(ada, kind: "environments", id: "all"))
        let e = try XCTUnwrap(envs.data["environments"]?.array?.first)
        XCTAssertEqual(e["variables"]?["baseUrl"]?.string, "https://staging.test")
        // The secret's value is never in the item's data, only in `secrets` (sealed by the server).
        XCTAssertEqual(e["variables"]?["apiKey"]?.string, "")
        XCTAssertEqual(e["secrets"]?.array, [.string("apiKey")])
        XCTAssertEqual(envs.secrets?[staging.id]?["apiKey"]?.string, "sk_live_1")
        XCTAssertEqual(envs.data["activeEnvironmentId"]?.string, staging.id)
        let g = try XCTUnwrap(fake.item(ada, kind: "globals", id: "all"))
        XCTAssertEqual(g.data["variables"]?["token"]?.string, "")
        XCTAssertEqual(g.secrets?["token"]?.string, "g-secret")
        // Not in this device's library files either: only in the vault (the Keychain).
        let files = String(data: try JSONEncoder().encode(store.loadEnvironments()), encoding: .utf8)!
        XCTAssertFalse(files.contains("sk_live_1"))

        // Another browser changes a value and a secret.
        var data = envs.data.object!
        var list = data["environments"]!.array!
        var first = list[0].object!
        var vars = first["variables"]!.object!
        vars["baseUrl"] = .string("https://staging2.test")
        first["variables"] = .object(vars)
        list[0] = .object(first)
        data["environments"] = .array(list)
        fake.write(ada, kind: "environments", id: "all", data: .object(data), secrets: .object([staging.id: .object(["apiKey": .string("sk_live_2")])]))
        await sync.pull()
        XCTAssertEqual(model.environments.first?.variables["baseUrl"], "https://staging2.test")
        XCTAssertNil(model.environments.first?.variables["apiKey"])
        XCTAssertEqual(vault.value(environment: staging.id, name: "apiKey"), "sk_live_2")
        XCTAssertFalse(sync.dirty)
    }

    func testAServerWithoutAHostKeyKeepsTheSecretsHere() async throws {
        fake.secretsStored = false
        let env = model.addEnvironment(name: "Prod")
        model.setVariable(env, name: "apiKey", value: "local-only", secret: true)
        try await signIn()
        XCTAssertEqual(sync.status.secretsStored, false)
        XCTAssertNil(fake.item(ada, kind: "environments", id: "all")?.secrets)
        // The item comes back without secrets: this device's values stay.
        var data = fake.item(ada, kind: "environments", id: "all")!.data.object!
        data["activeEnvironmentId"] = .string(env.id)
        fake.write(ada, kind: "environments", id: "all", data: .object(data))
        await sync.pull()
        XCTAssertEqual(vault.value(environment: env.id, name: "apiKey"), "local-only")
    }

    func testEmptySettingsAreNotUploadedFirst() async throws {
        try await signIn()
        XCTAssertNil(fake.item(ada, kind: "environments", id: "all"))
        XCTAssertNil(fake.item(ada, kind: "globals", id: "all"))
    }

    func testOlderWebEnvironmentIdsStayNumbers() throws {
        let e = try JSONDecoder().decode(SavedEnvironment.self, from: Data(#"{"id":1700000000000,"name":"Old","variables":{"a":"1"},"org":"o1"}"#.utf8))
        XCTAssertEqual(e.id, "1700000000000")
        let back = try JSONAny.from(e)
        XCTAssertEqual(back["id"], .number(1_700_000_000_000))
        XCTAssertEqual(back["org"]?.string, "o1")
    }

    // MARK: Organizations

    func testOrganizationsShowTheirOwnCollections() async throws {
        try await signIn()
        fake.write(ada, kind: "collection", id: "300", data: .object(["id": .number(300), "name": .string("Acme API"), "requests": .array([]), "org": .string("org-acme")]))
        await sync.pull()
        try await cloud.loadOrgs()
        model.orgScope = OrgScope(current: nil, personal: cloud.personalOrg?.id)
        XCTAssertFalse(model.visibleCollections.contains { $0.name == "Acme API" })
        model.orgScope = OrgScope(current: "org-acme", personal: cloud.personalOrg?.id)
        XCTAssertEqual(model.visibleCollections.map(\.name), ["Acme API"])
        let c = model.addCollection(name: "Acme admin")
        XCTAssertEqual(c.org, "org-acme")
        await sync.flush()
        XCTAssertEqual(fake.item(ada, kind: "collection", id: String(c.id))?.data["org"]?.string, "org-acme")
    }
}

/// The web app's JSON survives a round trip through this app's model (an edit here pushes the
/// whole collection to every device).
final class WebShapeTests: XCTestCase {
    func testWebCollectionRoundTrip() throws {
        let text = #"""
        {"id":1731000000001,"name":"Web","org":"o1","inlineAuth":true,"folders":["orders"],
         "variables":{"baseUrl":"https://x.test"},
         "auth":{"type":"awsv4","accessKey":"${ak}","secretKey":"${sk}","region":"eu-west-1","service":"execute-api"},
         "openapi":{"source":"spec.yaml"},
         "requests":[
          {"id":1,"name":"Create","url":"${baseUrl}/orders","method":"post","headers":[{"name":"X","value":"1","enabled":false}],
           "body":"{}","preRequestScript":"","postRequestScript":"pm.test()","bodyMode":"raw","bodyType":"json",
           "auth":{"type":"digest","username":"u","password":"p"},"file":"orders/create","examples":[{"name":"ok","status":201,"headers":[],"body":"{}"}]},
          {"id":2,"name":"Q","url":"${baseUrl}/graphql","method":"POST","bodyType":"graphql","body":"query { a }",
           "graphql":{"variables":{"id":1},"operationName":"A"}},
          {"id":3,"name":"WS","url":"wss://x.test/ws","method":"GET","kind":"websocket","websocket":{"subprotocols":["chat"],"messages":[{"name":"hi","content":"hi"}]}},
          {"id":4,"name":"Form","url":"https://x.test/f","method":"POST","bodyMode":"multipart","formFields":[{"key":"a","value":"1","type":"text","enabled":true}],
           "disabledParams":[{"key":"page","value":"2"}],"auth":{"type":"oauth2","grantType":"authorization_code","clientId":"c","usePkce":true}}
         ]}
        """#
        let original = try JSONDecoder().decode(JSONAny.self, from: Data(text.utf8))
        let c = try original.decode(SavedCollection.self)
        let back = try JSONAny.from(c)
        // Method is upper-cased (as every app sends it); everything else comes back as it was.
        var expected = original.object!
        var requests = expected["requests"]!.array!
        var first = requests[0].object!
        first["method"] = .string("POST")
        requests[0] = .object(first)
        expected["requests"] = .array(requests)
        func strip(_ j: JSONAny) -> JSONAny {
            // Defaults this app writes for every request (empty strings, raw body mode) don't count.
            guard case let .object(o) = j else {
                if case let .array(a) = j { return .array(a.map(strip)) }
                return j
            }
            var out: [String: JSONAny] = [:]
            for (k, v) in o {
                if ["body", "preRequestScript", "postRequestScript", "headers"].contains(k), v == .string("") || v == .array([]) { continue }
                if k == "bodyMode", v == .string("raw") { continue }
                out[k] = strip(v)
            }
            return .object(out)
        }
        XCTAssertEqual(strip(back), strip(.object(expected)))
        XCTAssertEqual(c.requests[0].auth?.kind, "digest")
        XCTAssertEqual(c.auth?.kind, "awsv4")
        XCTAssertEqual(c.org, "o1")
    }
}
