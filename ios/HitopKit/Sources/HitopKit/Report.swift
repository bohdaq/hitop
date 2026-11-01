import Foundation

/// A run report shaped like `report.json` (`spec/run-report.md`,
/// `reportVersion` 1), so the app shows what `hitop run` would write and a
/// report can be shared as the same file. Steps are masked before they get
/// here, so a report never holds a secret value.
public struct Report: Sendable {
    public static let version = 1

    public struct Counts: Sendable, Equatable {
        public var total = 0
        public var passed = 0
        public var failed = 0

        mutating func add(_ ok: Bool) {
            total += 1
            if ok { passed += 1 } else { failed += 1 }
        }

        var json: JSONValue {
            .object([("total", .number("\(total)")), ("passed", .number("\(passed)")), ("failed", .number("\(failed)"))])
        }
    }

    public struct Assertion: Sendable {
        public var name: String
        public var passed: Bool
        /// What was found instead; nil when it passed.
        public var message: String?
    }

    public struct RequestReport: Sendable {
        public var name: String
        public var method: String
        public var url: String
        public var passed: Bool
        public var status: Int?
        public var responseTimeMs: Int?
        public var durationMs: Int
        public var error: String?
        public var failure: String?
        public var assertions: [Assertion]
        /// `<folder>/requests/<path>.yaml`.
        public var file: String?
        /// What the run left out on this platform (added key; readers ignore it).
        public var notes: [String]

        init(_ s: StepResult, file: String?) {
            assertions = s.tests.map { Assertion(name: $0.name, passed: $0.pass, message: $0.pass ? nil : ($0.detail.isEmpty ? "failed" : $0.detail)) }
            name = s.requestName
            method = s.method
            url = s.url
            passed = s.passed
            status = s.error == nil ? s.response?.status : nil
            responseTimeMs = s.error == nil ? s.response?.durationMs : nil
            durationMs = s.durationMs
            error = s.error
            let failed = assertions.filter { !$0.passed }.map { "\($0.name): \($0.message ?? "failed")" }
            if passed {
                failure = nil
            } else if let e = error {
                failure = e
            } else if !failed.isEmpty {
                failure = failed.joined(separator: "; ")
            } else {
                failure = "HTTP \(status ?? 0)"
            }
            self.file = file
            notes = s.notes
        }

        var json: JSONValue {
            var o: [(String, JSONValue)] = [
                ("name", .string(name)), ("method", .string(method)), ("url", .string(url)), ("passed", .bool(passed)),
                ("status", status.map { .number("\($0)") } ?? .null),
                ("responseTimeMs", responseTimeMs.map { .number("\($0)") } ?? .null),
                ("durationMs", .number("\(durationMs)")),
                ("error", error.map(JSONValue.string) ?? .null),
                ("failure", failure.map(JSONValue.string) ?? .null),
                ("assertions", .array(assertions.map { a in
                    .object([("name", .string(a.name)), ("passed", .bool(a.passed)), ("message", a.message.map(JSONValue.string) ?? .null)])
                })),
                ("file", file.map(JSONValue.string) ?? .null),
            ]
            if !notes.isEmpty { o.append(("notes", .array(notes.map(JSONValue.string)))) }
            return .object(o)
        }
    }

    public struct Iteration: Sendable {
        public var iteration: Int
        public var startedAt: String
        public var durationMs: Int
        public var data: [String: String]?
        public var passed: Bool
        public var requests: [RequestReport]
    }

    public struct CollectionReport: Sendable {
        public var name: String
        public var environment: String?
        public var passed: Bool
        public var iterations: [Iteration]
    }

    public var toolName = "hitop"
    public var toolVersion: String
    public var startedAt: String
    public var durationMs: Int
    public var passed: Bool
    public var bailed: Bool
    public var collections: [CollectionReport]
    public var summaryIterations: Int
    public var requests: Counts
    public var assertions: Counts

    public var json: JSONValue {
        .object([
            ("reportVersion", .number("\(Report.version)")),
            ("tool", .object([("name", .string(toolName)), ("version", .string(toolVersion))])),
            ("startedAt", .string(startedAt)),
            ("durationMs", .number("\(durationMs)")),
            ("passed", .bool(passed)),
            ("bailed", .bool(bailed)),
            ("summary", .object([
                ("collections", .number("\(collections.count)")),
                ("iterations", .number("\(summaryIterations)")),
                ("requests", requests.json),
                ("assertions", assertions.json),
            ])),
            ("collections", .array(collections.map { c in
                .object([
                    ("name", .string(c.name)),
                    ("environment", c.environment.map(JSONValue.string) ?? .null),
                    ("passed", .bool(c.passed)),
                    ("iterations", .array(c.iterations.map { i in
                        .object([
                            ("iteration", .number("\(i.iteration)")),
                            ("startedAt", .string(i.startedAt)),
                            ("durationMs", .number("\(i.durationMs)")),
                            ("data", i.data.map(JSONValue.strings) ?? .null),
                            ("passed", .bool(i.passed)),
                            ("requests", .array(i.requests.map(\.json))),
                        ])
                    })),
                ])
            })),
        ])
    }

    /// `report.json`'s text: pretty, ending with a new line.
    public var jsonText: String { json.pretty + "\n" }

    /// RFC 3339, UTC, whole seconds.
    public static func timestamp(_ d: Date) -> String {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        f.timeZone = TimeZone(identifier: "UTC")
        return f.string(from: d)
    }
}

/// Collects iterations while a run goes, then the report.
public struct ReportBuilder: Sendable {
    let started = Date()
    var iterationStart: Date?
    let toolVersion: String
    var collections: [Report.CollectionReport] = []
    /// `<folder>/` for the `file` of each request; nil outside a folder.
    var filePrefix: String?

    public init(toolVersion: String = HitopKitInfo.version) { self.toolVersion = toolVersion }

    public mutating func collection(_ name: String, environment: String?, folderName: String? = nil) {
        collections.append(Report.CollectionReport(name: name, environment: environment, passed: true, iterations: []))
        filePrefix = folderName.map { $0.isEmpty || $0 == "." ? "" : $0 + "/" }
    }

    public mutating func startIteration() { iterationStart = Date() }

    public mutating func iteration(data: [String: String]? = nil, steps: [StepResult]) {
        let at = iterationStart ?? Date()
        iterationStart = nil
        if collections.isEmpty { collection("", environment: nil) }
        let prefix = filePrefix
        let requests = steps.map { s in Report.RequestReport(s, file: prefix.map { "\($0)requests/\(s.path).\(s.fileExtension)" }) }
        let passed = requests.allSatisfy(\.passed)
        let i = collections.count - 1
        collections[i].passed = collections[i].passed && passed
        collections[i].iterations.append(Report.Iteration(
            iteration: collections[i].iterations.count + 1, startedAt: Report.timestamp(at),
            durationMs: Int(Date().timeIntervalSince(at) * 1000), data: data, passed: passed, requests: requests))
    }

    public func finish(bailed: Bool) -> Report {
        var requests = Report.Counts()
        var assertions = Report.Counts()
        var iterations = 0
        for c in collections {
            for i in c.iterations {
                iterations += 1
                for r in i.requests {
                    requests.add(r.passed)
                    for a in r.assertions { assertions.add(a.passed) }
                }
            }
        }
        return Report(toolVersion: toolVersion, startedAt: Report.timestamp(started), durationMs: Int(Date().timeIntervalSince(started) * 1000),
                      passed: requests.failed == 0, bailed: bailed, collections: collections, summaryIterations: iterations,
                      requests: requests, assertions: assertions)
    }
}

public enum HitopKitInfo {
    /// This package's version; reports name it as the tool version with an `-ios` suffix.
    public static let version = "1.0.0-ios"
}
