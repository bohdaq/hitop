import Testing
import Foundation
@testable import HiTop

// MARK: - InterpolationService

@Suite("InterpolationService")
struct InterpolationServiceTests {

    @Test("Replaces a single variable")
    func singleVariable() {
        let result = InterpolationService.interpolate("Hello ${name}!", variables: ["name": "World"])
        #expect(result == "Hello World!")
    }

    @Test("Leaves unknown variables unchanged")
    func missingVariableUnchanged() {
        let result = InterpolationService.interpolate("${missing}", variables: [:])
        #expect(result == "${missing}")
    }

    @Test("Replaces multiple variables in one string")
    func multipleVariables() {
        let result = InterpolationService.interpolate(
            "${proto}://${host}/api",
            variables: ["proto": "https", "host": "example.com"]
        )
        #expect(result == "https://example.com/api")
    }

    @Test("Passes through strings with no tokens")
    func noVariables() {
        let result = InterpolationService.interpolate("https://example.com", variables: [:])
        #expect(result == "https://example.com")
    }

    @Test("Interpolates headers")
    func headerInterpolation() {
        let headers = [
            Header(name: "Authorization", value: "Bearer ${token}"),
            Header(name: "X-Tenant", value: "${tenant}")
        ]
        let result = InterpolationService.interpolateHeaders(
            headers,
            variables: ["token": "abc123", "tenant": "acme"]
        )
        #expect(result[0].value == "Bearer abc123")
        #expect(result[1].value == "acme")
    }
}

// MARK: - Collection Codable

@Suite("Collection Codable")
@MainActor
struct CollectionCodableTests {

    @Test("Full round-trip encode → decode")
    func roundTrip() throws {
        let req = HTTPRequest(
            name: "Get users",
            url: "https://api.example.com/users",
            method: "GET",
            headers: [Header(name: "Accept", value: "application/json")]
        )
        let col = Collection(name: "Test Suite", requests: [req], variables: ["host": "example.com"])

        let data = try JSONEncoder().encode(col)
        let decoded = try JSONDecoder().decode(Collection.self, from: data)

        #expect(decoded.name == "Test Suite")
        #expect(decoded.requests.count == 1)
        #expect(decoded.requests[0].name == "Get users")
        #expect(decoded.requests[0].headers[0].name == "Accept")
        #expect(decoded.variables["host"] == "example.com")
    }

    @Test("Missing variables field defaults to empty dict")
    func missingVariablesDefaulted() throws {
        let json = #"{"id":1,"name":"A","requests":[]}"#
        let col = try JSONDecoder().decode(Collection.self, from: Data(json.utf8))
        #expect(col.variables.isEmpty)
    }

    @Test("Script fields use camelCase JSON keys")
    func scriptFieldsAreCamelCase() throws {
        let req = HTTPRequest(
            preRequestScript: "setUrl('x')",
            postRequestScript: "setContext('k','v')"
        )
        let data = try JSONEncoder().encode(req)
        let json = String(data: data, encoding: .utf8) ?? ""
        #expect(json.contains("preRequestScript"))
        #expect(json.contains("postRequestScript"))
    }

    @Test("Array of collections round-trips")
    func collectionArrayRoundTrip() throws {
        let cols = [
            Collection(name: "A", requests: [HTTPRequest(name: "R1")]),
            Collection(name: "B", requests: [HTTPRequest(name: "R2")])
        ]
        let data = try JSONEncoder().encode(cols)
        let decoded = try JSONDecoder().decode([Collection].self, from: data)
        #expect(decoded.count == 2)
        #expect(decoded[0].name == "A")
        #expect(decoded[1].requests[0].name == "R2")
    }
}

// MARK: - AppViewModel CRUD

@Suite("AppViewModel")
@MainActor
struct AppViewModelTests {

    @Test("addCollection appends and persists")
    func addCollection() {
        let vm = AppViewModel()
        vm.collections = []
        vm.addCollection(name: "API Tests")
        #expect(vm.collections.count == 1)
        #expect(vm.collections[0].name == "API Tests")
    }

    @Test("addCollection uses fallback name when empty")
    func addCollectionEmptyName() {
        let vm = AppViewModel()
        vm.collections = []
        vm.addCollection(name: "")
        #expect(vm.collections[0].name == "New Collection")
    }

    @Test("deleteCollection removes by id")
    func deleteCollection() {
        let vm = AppViewModel()
        vm.collections = [Collection(name: "A"), Collection(name: "B")]
        let idA = vm.collections[0].id
        vm.deleteCollection(id: idA)
        #expect(vm.collections.count == 1)
        #expect(vm.collections[0].name == "B")
    }

    @Test("deleteCollection clears activeCollectionId when matched")
    func deleteActiveCollection() {
        let vm = AppViewModel()
        let col = Collection(name: "A")
        vm.collections = [col]
        vm.activeCollectionId = col.id
        vm.deleteCollection(id: col.id)
        #expect(vm.activeCollectionId == nil)
    }

    @Test("renameCollection updates name")
    func renameCollection() {
        let vm = AppViewModel()
        vm.collections = [Collection(name: "Old")]
        let id = vm.collections[0].id
        vm.renameCollection(id: id, name: "New")
        #expect(vm.collections[0].name == "New")
    }

    @Test("duplicateCollection creates copy with new id")
    func duplicateCollection() {
        let vm = AppViewModel()
        vm.collections = [Collection(name: "Original")]
        let originalId = vm.collections[0].id
        vm.duplicateCollection(id: originalId)
        #expect(vm.collections.count == 2)
        #expect(vm.collections[1].name == "Original (Copy)")
        #expect(vm.collections[1].id != originalId)
    }

    @Test("addRequest appends and auto-loads")
    func addRequest() {
        let vm = AppViewModel()
        let col = Collection(name: "A")
        vm.collections = [col]
        vm.addRequest(to: col.id)
        #expect(vm.collections[0].requests.count == 1)
        #expect(vm.activeRequestId == vm.collections[0].requests[0].id)
        #expect(vm.activeCollectionId == col.id)
    }

    @Test("loadRequest sets all active fields")
    func loadRequest() {
        let vm = AppViewModel()
        let req = HTTPRequest(
            name: "Login",
            url: "https://api.example.com/auth",
            method: "POST",
            headers: [Header(name: "Content-Type", value: "application/json")],
            body: #"{"user":"test"}"#
        )
        let col = Collection(name: "Auth", requests: [req])
        vm.collections = [col]

        vm.loadRequest(req, collectionId: col.id)

        #expect(vm.activeURL == "https://api.example.com/auth")
        #expect(vm.activeMethod == "POST")
        #expect(vm.activeBody == #"{"user":"test"}"#)
        #expect(vm.activeHeaders.contains { $0.name == "Content-Type" })
        #expect(vm.selectedRequestId == req.id)
        #expect(vm.activeCollectionId == col.id)
    }

    @Test("deleteRequest removes from collection and clears active if matched")
    func deleteActiveRequest() {
        let vm = AppViewModel()
        let req = HTTPRequest(name: "R")
        let col = Collection(name: "A", requests: [req])
        vm.collections = [col]
        vm.activeRequestId = req.id
        vm.activeCollectionId = col.id

        vm.deleteRequest(id: req.id, collectionId: col.id)

        #expect(vm.collections[0].requests.isEmpty)
        #expect(vm.activeRequestId == nil)
    }

    @Test("renameRequest updates name inside collection")
    func renameRequest() {
        let vm = AppViewModel()
        let req = HTTPRequest(name: "Old Name")
        let col = Collection(name: "A", requests: [req])
        vm.collections = [col]

        vm.renameRequest(id: req.id, collectionId: col.id, name: "New Name")

        #expect(vm.collections[0].requests[0].name == "New Name")
    }

    @Test("updateVariables stores key-value pairs")
    func updateVariables() {
        let vm = AppViewModel()
        let col = Collection(name: "A")
        vm.collections = [col]

        vm.updateVariables(["host": "example.com", "version": "v2"], for: col.id)

        let vars = vm.variables(for: col.id)
        #expect(vars["host"] == "example.com")
        #expect(vars["version"] == "v2")
    }

    @Test("variables interpolated into URL")
    func variablesInterpolated() {
        let vm = AppViewModel()
        let col = Collection(name: "A", variables: ["host": "api.example.com", "version": "v1"])
        vm.collections = [col]

        let url = InterpolationService.interpolate(
            "https://${host}/${version}/users",
            variables: vm.variables(for: col.id)
        )
        #expect(url == "https://api.example.com/v1/users")
    }

    @Test("exportJSON and importJSON round-trip")
    func exportImportRoundTrip() throws {
        let vm = AppViewModel()
        let req = HTTPRequest(name: "Get Posts", url: "https://jsonplaceholder.typicode.com/posts", method: "GET")
        vm.collections = [Collection(name: "JSONPlaceholder", requests: [req])]

        let json = vm.exportJSON()
        #expect(!json.isEmpty)
        #expect(json.contains("JSONPlaceholder"))

        let vm2 = AppViewModel()
        try vm2.importJSON(json)
        #expect(vm2.collections.count == 1)
        #expect(vm2.collections[0].name == "JSONPlaceholder")
        #expect(vm2.collections[0].requests[0].url == "https://jsonplaceholder.typicode.com/posts")
    }

    @Test("importJSON throws on invalid data")
    func importJSONThrowsOnBadData() {
        let vm = AppViewModel()
        #expect(throws: (any Error).self) {
            try vm.importJSON("not valid json {{")
        }
    }

    @Test("updateContext merges key-value pairs")
    func updateContext() {
        let vm = AppViewModel()
        let col = Collection(name: "A")
        vm.collections = [col]

        vm.updateContext(["token": "abc"], for: col.id)
        vm.updateContext(["userId": "42"], for: col.id)

        let ctx = vm.context(for: col.id)
        #expect(ctx["token"] == "abc")
        #expect(ctx["userId"] == "42")
    }
}
