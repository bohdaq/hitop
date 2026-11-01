import Foundation
import XCTest
@testable import HitopKit

/// The repository root (this file is ios/HitopKit/Tests/HitopKitTests/Support.swift).
let repoRoot: URL = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent()

/// A program on PATH, or nil.
func which(_ name: String) -> URL? {
    for dir in (ProcessInfo.processInfo.environment["PATH"] ?? "/usr/bin:/bin").split(separator: ":") {
        let u = URL(fileURLWithPath: String(dir)).appendingPathComponent(name)
        if FileManager.default.isExecutableFile(atPath: u.path) { return u }
    }
    return nil
}

/// Runs a program to the end; (exit status, stdout, stderr).
@discardableResult
func runProgram(_ exe: URL, _ args: [String], stdin: String = "", cwd: URL? = nil, env: [String: String]? = nil) throws -> (Int32, String, String) {
    let p = Process()
    p.executableURL = exe
    p.arguments = args
    if let cwd { p.currentDirectoryURL = cwd }
    if let env { p.environment = env }
    let (i, o, e) = (Pipe(), Pipe(), Pipe())
    p.standardInput = i
    p.standardOutput = o
    p.standardError = e
    try p.run()
    // Read both pipes while the program runs, so a full pipe can't block it.
    final class Box: @unchecked Sendable { var data = Data() }
    let (out, err) = (Box(), Box())
    let group = DispatchGroup()
    for (pipe, box) in [(o, out), (e, err)] {
        group.enter()
        DispatchQueue.global().async {
            box.data = pipe.fileHandleForReading.readDataToEndOfFile()
            group.leave()
        }
    }
    i.fileHandleForWriting.write(Data(stdin.utf8))
    try i.fileHandleForWriting.close()
    group.wait()
    p.waitUntilExit()
    return (p.terminationStatus, String(decoding: out.data, as: UTF8.self), String(decoding: err.data, as: UTF8.self))
}

/// Node.js as the script engine on Linux: the same JavaScript hitop builds
/// for JavaScriptCore on iOS, evaluated with `vm.runInNewContext` (which
/// returns the last expression's value, as JSC's evaluateScript does).
struct NodeScriptEngine: ScriptEngine {
    let node: URL

    init?() {
        guard let n = which("node") else { return nil }
        node = n
    }

    static let runner = """
    const vm = require('vm'); let s = ''; process.stdin.setEncoding('utf8');
    process.stdin.on('data', d => s += d);
    process.stdin.on('end', () => {
      try { const r = vm.runInNewContext(s, {}, { timeout: 5000 }); process.stdout.write(String(r)); }
      catch (e) { process.stderr.write(String(e && e.message || e)); process.exit(1); }
    });
    """

    func evaluate(_ source: String) throws -> String {
        let (status, out, err) = try runProgram(node, ["-e", NodeScriptEngine.runner], stdin: source)
        if status != 0 { throw ScriptError(err.isEmpty ? "node exited with \(status)" : err) }
        return out
    }
}

/// The engine for tests: JavaScriptCore on Apple platforms, Node elsewhere.
let testEngine: ScriptEngine? = {
    #if canImport(JavaScriptCore)
    return JavaScriptCoreEngine()
    #else
    return NodeScriptEngine()
    #endif
}()

func requireEngine() throws -> ScriptEngine {
    guard let e = testEngine else { throw XCTSkip("no JavaScript engine (install node)") }
    return e
}

/// `mock_server.py` on 127.0.0.1, started once for all tests. It exits when
/// the test process ends (its stdin closes).
final class MockServer: @unchecked Sendable {
    static let shared = MockServer()

    let port: Int?
    let failure: String?
    private let process = Process()
    private let input = Pipe()

    private init() {
        guard let python = which("python3") else {
            port = nil
            failure = "python3 not found"
            return
        }
        let script = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("mock_server.py")
        process.executableURL = python
        process.arguments = ["-I", script.path]
        let out = Pipe()
        process.standardInput = input
        process.standardOutput = out
        do {
            try process.run()
        } catch {
            port = nil
            failure = "starting the mock server: \(error)"
            return
        }
        // The first line is the port.
        var line = Data()
        while true {
            let byte = out.fileHandleForReading.readData(ofLength: 1)
            if byte.isEmpty || byte == Data("\n".utf8) { break }
            line.append(byte)
        }
        port = Int(String(decoding: line, as: UTF8.self))
        failure = port == nil ? "the mock server printed no port" : nil
    }

    var base: String { "http://127.0.0.1:\(port ?? 0)" }

    static func require() throws -> MockServer {
        let s = shared
        if let f = s.failure { throw XCTSkip(f) }
        return s
    }

    /// Every request the server got, oldest first.
    func requests() throws -> [JSONValue] {
        let data = try Data(contentsOf: URL(string: "\(base)/__requests")!)
        return try JSONValue.parse(String(decoding: data, as: UTF8.self)).arrayValue ?? []
    }

    func reset() throws {
        _ = try Data(contentsOf: URL(string: "\(base)/__reset")!)
    }
}

/// A copy of `cli/fixtures/sample` in a new temporary folder, with
/// `environments/mock.yaml` pointing at the mock server.
func sampleCopy(port: Int, name: String = "sample") throws -> URL {
    let dir = FileManager.default.temporaryDirectory.appendingPathComponent("hitopkit-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    let target = dir.appendingPathComponent(name)
    try FileManager.default.copyItem(at: repoRoot.appendingPathComponent("cli/fixtures/sample"), to: target)
    try """
    variables:
      base: http://127.0.0.1:\(port)
      apiKey: mock-key
      token: mock-token
    """.write(to: target.appendingPathComponent("environments/mock.yaml"), atomically: true, encoding: .utf8)
    return target
}

/// A folder made from `files` (path → text) in a new temporary directory.
func makeFolder(_ files: [String: String], name: String = "api") throws -> URL {
    let dir = FileManager.default.temporaryDirectory.appendingPathComponent("hitopkit-\(UUID().uuidString)").appendingPathComponent(name)
    for (path, text) in files {
        let u = dir.appendingPathComponent(path)
        try FileManager.default.createDirectory(at: u.deletingLastPathComponent(), withIntermediateDirectories: true)
        try text.write(to: u, atomically: true, encoding: .utf8)
    }
    return dir
}
