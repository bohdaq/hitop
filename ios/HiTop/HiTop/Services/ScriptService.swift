import Foundation
import JavaScriptCore

struct ScriptResult {
    var url: String
    var headers: [Header]
    var body: String
    var contextUpdates: [String: String]
}

final class ScriptService {
    static let shared = ScriptService()

    func executePreRequest(
        script: String,
        url: String,
        method: String,
        headers: [Header],
        body: String,
        variables: [String: String],
        context: [String: String]
    ) throws -> ScriptResult {
        guard !script.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return ScriptResult(url: url, headers: headers, body: body, contextUpdates: [:])
        }

        let ctx = JSContext()!
        ctx.exceptionHandler = { _, exception in
            // Captured below via evaluateScript return
        }

        var mutableURL = url
        var mutableHeaders = headers
        var mutableBody = body
        var contextUpdates: [String: String] = [:]

        ctx.setObject(mutableURL, forKeyedSubscript: "_url" as NSString)

        let setUrlBlock: @convention(block) (String) -> Void = { newURL in
            mutableURL = newURL
        }
        ctx.setObject(setUrlBlock, forKeyedSubscript: "setUrl" as NSString)

        let setBodyBlock: @convention(block) (String) -> Void = { newBody in
            mutableBody = newBody
        }
        ctx.setObject(setBodyBlock, forKeyedSubscript: "setBody" as NSString)

        let setHeaderBlock: @convention(block) (String, String) -> Void = { name, value in
            if let idx = mutableHeaders.firstIndex(where: { $0.name == name }) {
                mutableHeaders[idx].value = value
            } else {
                mutableHeaders.append(Header(name: name, value: value))
            }
        }
        ctx.setObject(setHeaderBlock, forKeyedSubscript: "setHeader" as NSString)

        let setContextBlock: @convention(block) (String, String) -> Void = { key, value in
            contextUpdates[key] = value
        }
        ctx.setObject(setContextBlock, forKeyedSubscript: "setContext" as NSString)

        let getContextBlock: @convention(block) (String) -> String? = { key in
            contextUpdates[key] ?? context[key]
        }
        ctx.setObject(getContextBlock, forKeyedSubscript: "getContext" as NSString)

        let getVariableBlock: @convention(block) (String) -> String? = { key in
            variables[key]
        }
        ctx.setObject(getVariableBlock, forKeyedSubscript: "getVariable" as NSString)

        ctx.evaluateScript(script)

        if let exception = ctx.exception {
            throw NSError(domain: "ScriptError", code: 0, userInfo: [NSLocalizedDescriptionKey: exception.toString() ?? "Script error"])
        }

        return ScriptResult(url: mutableURL, headers: mutableHeaders, body: mutableBody, contextUpdates: contextUpdates)
    }

    func executePostRequest(
        script: String,
        responseBody: String,
        responseHeaders: [String: String],
        statusCode: Int,
        context: [String: String]
    ) throws -> [String: String] {
        guard !script.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return [:]
        }

        let ctx = JSContext()!
        var contextUpdates: [String: String] = [:]

        ctx.setObject(responseBody, forKeyedSubscript: "responseText" as NSString)
        ctx.setObject(statusCode, forKeyedSubscript: "statusCode" as NSString)

        let parsedResponse: Any
        if let data = responseBody.data(using: .utf8),
           let json = try? JSONSerialization.jsonObject(with: data) {
            parsedResponse = json
        } else {
            parsedResponse = responseBody
        }
        ctx.setObject(parsedResponse, forKeyedSubscript: "response" as NSString)

        let setContextBlock: @convention(block) (String, String) -> Void = { key, value in
            contextUpdates[key] = value
        }
        ctx.setObject(setContextBlock, forKeyedSubscript: "setContext" as NSString)

        let getContextBlock: @convention(block) (String) -> String? = { key in
            contextUpdates[key] ?? context[key]
        }
        ctx.setObject(getContextBlock, forKeyedSubscript: "getContext" as NSString)

        let getResponseHeaderBlock: @convention(block) (String) -> String? = { name in
            responseHeaders[name.lowercased()] ?? responseHeaders[name]
        }
        ctx.setObject(getResponseHeaderBlock, forKeyedSubscript: "getResponseHeader" as NSString)

        ctx.evaluateScript(script)

        if let exception = ctx.exception {
            throw NSError(domain: "ScriptError", code: 0, userInfo: [NSLocalizedDescriptionKey: exception.toString() ?? "Script error"])
        }

        return contextUpdates
    }
}
