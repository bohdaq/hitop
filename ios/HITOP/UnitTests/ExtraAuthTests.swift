import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import HitopKit
import XCTest
@testable import HITOP

/// Answers 401 with a Digest challenge until an answer comes; records what it got.
final class ChallengeProtocol: URLProtocol {
    nonisolated(unsafe) static var seen: [URLRequest] = []
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        Self.seen.append(request)
        let digest = request.value(forHTTPHeaderField: "Authorization")?.hasPrefix("Digest ") == true
        let open = request.url?.path != "/digest"
        let r = HTTPURLResponse(url: request.url!, statusCode: digest || open ? 200 : 401, httpVersion: "HTTP/1.1",
                                headerFields: digest || open ? [:] : ["WWW-Authenticate": "Digest realm=\"r\", nonce=\"n\", qop=\"auth\""])!
        client?.urlProtocol(self, didReceive: r, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Data("{}".utf8))
        client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}

    static let configuration: @Sendable () -> URLSessionConfiguration = {
        let c = URLSessionConfiguration.ephemeral
        c.protocolClasses = [ChallengeProtocol.self]
        return c
    }
}

@MainActor
final class ExtraAuthTests: XCTestCase {
    private func send(_ auth: AuthSpec, path: String = "/x", collectionAuth: AuthSpec? = nil, variables: [String: String] = ["key": "AK"]) async -> SendResult {
        ChallengeProtocol.seen = []
        var r = SavedRequest(name: "R", url: "http://auth.test\(path)", method: "GET")
        r.auth = collectionAuth == nil ? auth : nil
        let c = SavedCollection(name: "C", requests: [r], variables: variables, auth: collectionAuth)
        var context = VariableContext()
        context.collection = variables
        return await Sender(configuration: ChallengeProtocol.configuration).send(r, collection: c, context: context)
    }

    private func status(_ r: SendResult) -> Int? { if case let .success(o) = r.outcome { return o.status }; return nil }
    private func error(_ r: SendResult) -> String { if case let .failure(f) = r.outcome { return f.message }; return "" }

    func testAWSSignsWithVariablesFilledIn() async {
        let aws = AuthSpec.other(type: "awsv4", fields: ["accessKey": .string("${key}"), "secretKey": .string("SK"), "region": .string("eu-west-1"), "service": .string("execute-api")])
        for collection in [nil, aws] {
            let r = await send(aws, collectionAuth: collection)
            XCTAssertEqual(status(r), 200, error(r))
            let h = ChallengeProtocol.seen.last?.value(forHTTPHeaderField: "Authorization") ?? ""
            XCTAssertTrue(h.hasPrefix("AWS4-HMAC-SHA256 Credential=AK/"), h)
        }
    }

    func testDigestAnswersTheChallenge() async {
        let r = await send(.other(type: "digest", fields: ["username": .string("u"), "password": .string("p")]), path: "/digest")
        XCTAssertEqual(status(r), 200, error(r))
        XCTAssertEqual(ChallengeProtocol.seen.count, 2)
        XCTAssertTrue(ChallengeProtocol.seen[1].value(forHTTPHeaderField: "Authorization")?.contains("username=\"u\"") == true)
    }

    func testNothingIsSentWithoutItsAuth() async {
        let code = AuthSpec.other(type: "oauth2", fields: ["grant": .string("authorization_code"), "authUrl": .string("https://id.test/a"), "tokenUrl": .string("https://id.test/t"), "clientId": .string("c")])
        let asks = await send(code)
        XCTAssertTrue(error(asks).contains("sign in first"), error(asks))
        let unknown = await send(.other(type: "ntlm", fields: [:]))
        XCTAssertTrue(error(unknown).contains("ntlm auth isn't supported"), error(unknown))
        let missing = await send(.other(type: "digest", fields: ["username": .string("${nope}")]), path: "/digest")
        XCTAssertTrue(error(missing).contains("nope"), error(missing))
    }
}
