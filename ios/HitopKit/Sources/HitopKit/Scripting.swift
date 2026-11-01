import Foundation
#if canImport(JavaScriptCore)
import JavaScriptCore
#endif

/// Runs JavaScript and returns the value of its last expression as text.
/// hitop's scripts are built the way hitop-core builds them (`scripting.rs`):
/// a preamble with the script API, the script, then one `JSON.stringify`
/// of what it changed, so an engine only has to evaluate a string.
public protocol ScriptEngine: Sendable {
    func evaluate(_ source: String) throws -> String
}

public struct ScriptError: Error, CustomStringConvertible, LocalizedError, Sendable {
    public let description: String
    public init(_ description: String) { self.description = description }
    public var errorDescription: String? { description }
}

#if canImport(JavaScriptCore)
/// JavaScriptCore, on iOS and macOS. A fresh context per evaluation.
public struct JavaScriptCoreEngine: ScriptEngine {
    public init() {}

    public func evaluate(_ source: String) throws -> String {
        guard let ctx = JSContext() else { throw ScriptError("JavaScriptCore is not available") }
        let result = ctx.evaluateScript(source)
        if let e = ctx.exception {
            throw ScriptError(e.toString() ?? "script error")
        }
        return result?.toString() ?? ""
    }
}
#endif

/// The engine of this platform: JavaScriptCore on Apple platforms, none on
/// Linux (scripts are then reported as not run, unless a caller passes one).
public let defaultScriptEngine: ScriptEngine? = {
    #if canImport(JavaScriptCore)
    return JavaScriptCoreEngine()
    #else
    return nil
    #endif
}()

/// Variables set (a value) or unset (nil) by a script.
public typealias VariableUpdates = [String: String?]

/// What a pre-request script can change.
public struct PreRequestInput: Sendable {
    public var url: String
    public var method: String
    public var headers: [(String, String)]
    public var body: String
    /// Every scope merged (globals lowest).
    public var variables: [String: String]
    public var context: [String: JSONValue]
    /// The globals scope alone, for `pm.globals`.
    public var globals: [String: String]
}

public struct PreRequestResult: Sendable {
    public var url: String
    public var headers: [(String, String)]
    public var body: String
    public var contextUpdates: [String: JSONValue]
    public var variables: VariableUpdates
    public var globals: VariableUpdates
}

/// The request as sent, `pm.request` in a post-request script.
public struct SentRequest: Sendable {
    public var url: String
    public var method: String
    public var headers: [(String, String)]
    public var body: String
}

public struct PostRequestInput: Sendable {
    public var responseBody: String
    /// Names in lower case.
    public var responseHeaders: [String: String]
    public var statusCode: Int
    public var durationMs: Int
    public var variables: [String: String]
    public var context: [String: JSONValue]
    public var request: SentRequest?
    /// nil: `pm.globals` shares the merged variables.
    public var globals: [String: String]?
}

/// One `assert.*` (or `pm.test`, `tests[...]`) result.
public struct TestResult: Hashable, Sendable {
    public var name: String
    public var pass: Bool
    public var detail: String
}

public struct PostRequestResult: Sendable {
    public var contextUpdates: [String: JSONValue]
    public var tests: [TestResult]
    public var variables: VariableUpdates
    public var globals: VariableUpdates
}

public enum Scripts {
    /// The Postman `pm.*` shim, the same file as `shared/pm-shim.js` (a test checks).
    public static let pmShim: String = {
        guard let url = Bundle.module.url(forResource: "pm-shim", withExtension: "js"),
              let text = try? String(contentsOf: url, encoding: .utf8) else { return "" }
        return text
    }()

    /// True when a script may use Postman's API, so the shim is loaded.
    static func usesPostmanAPI(_ script: String) -> Bool {
        let words = ["pm", "postman", "tests", "responseBody", "responseCode", "responseTime", "environment", "globals"]
        let pattern = "\\b(" + words.joined(separator: "|") + ")\\b"
        guard let re = try? NSRegularExpression(pattern: pattern) else { return false }
        return re.firstMatch(in: script, range: NSRange(script.startIndex..., in: script)) != nil
    }

    static func withShim(api: String, script: String) -> String {
        "\(pmShim)\nconst __p = __hitopPm(\(api));\n(function (pm, tests, postman, environment, globals, responseBody, responseCode, responseTime) {\n\(script)\n}).call(undefined, __p.pm, __p.tests, __p.postman, __p.environment, __p.globals, __p.responseBody, __p.responseCode, __p.responseTime);\n__p.finish();\n"
    }

    static let setVarJS = "function (k, v) { __varUpdates[k] = v; if (v === null) delete __vars[k]; else __vars[k] = v; }"
    static let setGlobalJS = "function (k, v) { __globalUpdates[k] = v; }"

    static func object(_ m: [String: JSONValue]) -> JSONValue {
        .object(m.keys.sorted().map { ($0, m[$0]!) })
    }

    static func pairs(_ h: [(String, String)]) -> JSONValue {
        .array(h.map { .array([.string($0.0), .string($0.1)]) })
    }

    static func variableUpdates(_ v: JSONValue?) -> VariableUpdates {
        var out: VariableUpdates = [:]
        for (k, value) in v?.objectValue ?? [] {
            out[k] = .some(value.stringValue)
        }
        return out
    }

    static func contextUpdates(_ v: JSONValue?) -> [String: JSONValue] {
        var out: [String: JSONValue] = [:]
        for (k, value) in v?.objectValue ?? [] { out[k] = value }
        return out
    }

    public static func runPreRequest(_ script: String, _ input: PreRequestInput, engine: ScriptEngine) throws -> PreRequestResult {
        if script.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return PreRequestResult(url: input.url, headers: input.headers, body: input.body, contextUpdates: [:], variables: [:], globals: [:])
        }
        let preamble = """

        const __vars = \(JSONValue.strings(input.variables).compact);
        const __ctx  = \(object(input.context).compact);
        const __ctxUpdates = {};
        const __varUpdates = {};
        const __globals = \(JSONValue.strings(input.globals).compact);
        const __globalUpdates = {};
        let __url    = \(JSONValue.quote(input.url));
        let __body   = \(JSONValue.quote(input.body));
        let __headers = \(pairs(input.headers).compact);
        const __method = \(JSONValue.quote(input.method));

        function getVariable(k) { return __vars[k]; }
        function getContext(k)  { return __ctxUpdates[k] !== undefined ? __ctxUpdates[k] : __ctx[k]; }
        function setContext(k, v) { __ctxUpdates[k] = v; }
        function setUrl(u)  { __url = u; }
        function setBody(b) { __body = b; }
        function setHeader(name, value) {
            let found = false;
            for (let h of __headers) { if (h[0] === name) { h[1] = value; found = true; break; } }
            if (!found) __headers.push([name, value]);
        }
        function __removeHeader(name) {
            const n = String(name).toLowerCase();
            __headers = __headers.filter(h => String(h[0]).toLowerCase() !== n);
        }

        """
        let postamble = "\nJSON.stringify({ url: __url, body: __body, headers: __headers, ctx: __ctxUpdates, vars: __varUpdates, globals: __globalUpdates })\n"
        let body: String
        if usesPostmanAPI(script) {
            let api = "{ vars: __vars, setVar: \(setVarJS), globals: __globals, setGlobal: \(setGlobalJS), assert: { ok: function () {} }, "
                + "request: { get url() { return __url; }, method: __method, get headers() { return __headers; }, get body() { return __body; } }, "
                + "setUrl: setUrl, setHeader: setHeader, removeHeader: __removeHeader, setBody: setBody, response: null }"
            body = withShim(api: api, script: script)
        } else {
            body = script
        }
        let text: String
        do { text = try engine.evaluate("\(preamble)\n\(body)\n\(postamble)") } catch {
            throw ScriptError("pre-request script error: \(error)")
        }
        guard let out = try? JSONValue.parse(text) else { throw ScriptError("pre-request script returned invalid JSON") }
        var headers = input.headers
        if let arr = out["headers"]?.arrayValue {
            headers = arr.compactMap { h in
                guard let pair = h.arrayValue, pair.count >= 2, let k = pair[0].stringValue, let v = pair[1].stringValue else { return nil }
                return (k, v)
            }
        }
        return PreRequestResult(
            url: out["url"]?.stringValue ?? input.url,
            headers: headers,
            body: out["body"]?.stringValue ?? input.body,
            contextUpdates: contextUpdates(out["ctx"]),
            variables: variableUpdates(out["vars"]),
            globals: variableUpdates(out["globals"])
        )
    }

    public static func runPostRequest(_ script: String, _ input: PostRequestInput, engine: ScriptEngine) throws -> PostRequestResult {
        if script.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return PostRequestResult(contextUpdates: [:], tests: [], variables: [:], globals: [:])
        }
        let parsed = (try? JSONValue.parse(input.responseBody)) ?? .null
        let duration = input.durationMs
        let preamble = """

        const response        = \(parsed.compact);
        const responseText    = \(JSONValue.quote(input.responseBody));
        const responseHeaders = \(JSONValue.strings(input.responseHeaders).compact);
        const statusCode      = \(input.statusCode);
        const __vars = \(JSONValue.strings(input.variables).compact);
        const __ctx  = \(object(input.context).compact);
        const __ctxUpdates = {};
        const __varUpdates = {};
        const __globals = \(input.globals.map { JSONValue.strings($0).compact } ?? "null");
        const __globalUpdates = {};

        function getVariable(k) { return __vars[k]; }
        function getContext(k)  { return __ctxUpdates[k] !== undefined ? __ctxUpdates[k] : __ctx[k]; }
        function setContext(k, v) { __ctxUpdates[k] = v; }
        function getResponseValue(path) {
            const keys = path.split('.');
            let val = response;
            for (const k of keys) { if (val && typeof val === 'object') val = val[k]; else return undefined; }
            return val;
        }
        function getResponseHeader(name) { return responseHeaders[name.toLowerCase()]; }
        const __tests = [];
        function __record(name, pass, detail) { __tests.push({ name: String(name), pass: !!pass, detail: pass ? "" : String(detail) }); }
        const assert = {
            status(e) { __record("status is " + e, statusCode === Number(e), "got " + statusCode); },
            bodyContains(f) { __record("body contains \\"" + f + "\\"", String(responseText).includes(String(f)), "not found in the response body"); },
            header(n, e) {
                const a = responseHeaders[String(n).toLowerCase()];
                if (e === undefined) __record("has header " + n, a !== undefined, "header missing");
                else __record("header " + n + " is \\"" + e + "\\"", a !== undefined && a.includes(String(e)), a === undefined ? "header missing" : "got \\"" + a + "\\"");
            },
            responseTime(ms) { __record("responds within " + ms + " ms", \(duration) <= Number(ms), "took \(duration) ms"); },
            equal(a, e, name) { __record(name === undefined ? JSON.stringify(a) + " equals " + JSON.stringify(e) : name, JSON.stringify(a) === JSON.stringify(e), "got " + JSON.stringify(a)); },
            ok(c, name, detail) { __record(name === undefined ? "condition is true" : name, c, detail === undefined || detail === "" ? "was false" : detail); },
        };

        """
        let postamble = "JSON.stringify({ ctx: __ctxUpdates, tests: __tests, vars: __varUpdates, globals: __globalUpdates })"
        let body: String
        if usesPostmanAPI(script) {
            let sent = input.request ?? SentRequest(url: "", method: "", headers: [], body: "")
            let api = "{ vars: __vars, setVar: \(setVarJS), globals: __globals || undefined, setGlobal: \(setGlobalJS), assert: assert, "
                + "request: { url: \(JSONValue.quote(sent.url)), method: \(JSONValue.quote(sent.method)), headers: \(pairs(sent.headers).compact), body: \(JSONValue.quote(sent.body)) }, "
                + "response: { code: statusCode, text: responseText, headers: responseHeaders, time: \(duration) } }"
            body = withShim(api: api, script: script)
        } else {
            body = script
        }
        let text: String
        do { text = try engine.evaluate("\(preamble)\n\(body)\n\(postamble)") } catch {
            throw ScriptError("post-request script error: \(error)")
        }
        let out = (try? JSONValue.parse(text)) ?? .null
        let tests: [TestResult] = (out["tests"]?.arrayValue ?? []).compactMap { t in
            guard let name = t["name"]?.stringValue else { return nil }
            var pass = false
            if case .bool(true)? = t["pass"] { pass = true }
            return TestResult(name: name, pass: pass, detail: t["detail"]?.stringValue ?? "")
        }
        return PostRequestResult(contextUpdates: contextUpdates(out["ctx"]), tests: tests, variables: variableUpdates(out["vars"]), globals: variableUpdates(out["globals"]))
    }
}
