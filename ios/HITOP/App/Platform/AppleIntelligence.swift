import Foundation
import FoundationModels

/// I5: `LanguageAssistant` on Apple's on-device model (Foundation Models framework). Every prompt
/// is built and masked in Core (`Prompts`, `ModelInput`); the model runs on this device only and
/// nothing is sent anywhere. Checks and requests use guided generation (`@Generable`), so the
/// answer is always the shape Core maps (and Core still checks it).
@MainActor
final class AppleIntelligenceAssistant: LanguageAssistant {
    private var model: SystemLanguageModel { SystemLanguageModel.default }

    var availability: AssistantAvailability {
        switch model.availability {
        case .available:
            return .available
        case let .unavailable(reason):
            switch reason {
            case .deviceNotEligible: return .notEligible
            case .appleIntelligenceNotEnabled: return .turnedOff
            case .modelNotReady: return .notReady
            @unknown default: return .unsupported
            }
        }
    }

    func explain(_ prompt: ModelPrompt, onText: @escaping @MainActor (String) -> Void) async throws -> String {
        let session = LanguageModelSession(instructions: prompt.instructions)
        do {
            var text = ""
            let stream = session.streamResponse(to: prompt.text)
            for try await snapshot in stream {
                text = snapshot.content
                onText(text)
            }
            return text
        } catch {
            throw Self.failure(error)
        }
    }

    func suggestChecks(_ prompt: ModelPrompt) async throws -> [SuggestedCheck] {
        let session = LanguageModelSession(instructions: prompt.instructions)
        do {
            let answer = try await session.respond(to: prompt.text, generating: GeneratedChecks.self)
            return answer.content.checks.map(\.check)
        } catch {
            throw Self.failure(error)
        }
    }

    func requestDraft(_ prompt: ModelPrompt) async throws -> RequestDraft {
        let session = LanguageModelSession(instructions: prompt.instructions)
        do {
            let answer = try await session.respond(to: prompt.text, generating: GeneratedRequest.self)
            return answer.content.draft
        } catch {
            throw Self.failure(error)
        }
    }

    /// The framework's errors in a few words for the screen.
    static func failure(_ error: Error) -> AssistantFailure {
        if let f = error as? AssistantFailure { return f }
        guard let e = error as? LanguageModelSession.GenerationError else { return .other }
        switch e {
        case .exceededContextWindowSize: return .tooLong
        case .guardrailViolation: return .refused
        case .unsupportedLanguageOrLocale: return .language
        case .rateLimited, .concurrentRequests: return .busy
        case .assetsUnavailable: return AssistantFailure(AssistantAvailability.notReady.reason ?? "")
        default: return .other
        }
    }
}

// MARK: - Guided generation shapes (mapped onto Core's `SuggestedCheck` and `RequestDraft`)

@Generable
enum GeneratedCheckKind {
    case status, header, equals, exists, type, responseTime, bodyContains
}

@Generable
struct GeneratedCheck {
    @Guide(description: "What is checked: status (the status code), header (a response header), equals (a body field has a value), exists (a body field is present), type (a body field's JSON type), responseTime, bodyContains (the body contains a text)")
    var kind: GeneratedCheckKind
    @Guide(description: "For equals, exists and type: the JSON path of the body field, like status, total.amount or items[0].sku. For header: the header name. Otherwise empty.")
    var path: String
    @Guide(description: "For status: the code, like 201. For equals: the value as JSON, like \"processing\" or 6480 or true. For type: string, number, boolean, array, object or null. For header: text the value contains, or empty. For responseTime: milliseconds. For bodyContains: the text. For exists: empty.")
    var expected: String
    @Guide(description: "A short test name that says what the check means, like \"order is still processing\"")
    var name: String

    var check: SuggestedCheck {
        let k: SuggestedCheck.Kind = switch kind {
        case .status: .status
        case .header: .header
        case .equals: .equals
        case .exists: .exists
        case .type: .type
        case .responseTime: .responseTime
        case .bodyContains: .bodyContains
        }
        return SuggestedCheck(k, path: path, expected: expected, name: name)
    }
}

@Generable
struct GeneratedChecks {
    @Guide(description: "Three to five checks this response passes")
    var checks: [GeneratedCheck]
}

@Generable
enum GeneratedMethod {
    case GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS
}

@Generable
enum GeneratedBodyKind {
    case empty, json, text, form
}

@Generable
struct GeneratedHeader {
    @Guide(description: "Header name, like Content-Type")
    var name: String
    @Guide(description: "Header value; for credentials a variable like Bearer ${token}")
    var value: String
}

@Generable
struct GeneratedRequest {
    @Guide(description: "A short name for the request, a few words")
    var name: String
    var method: GeneratedMethod
    @Guide(description: "The full URL, starting with https://, http:// or a variable like ${baseUrl}")
    var url: String
    var headers: [GeneratedHeader]
    var bodyKind: GeneratedBodyKind
    @Guide(description: "The body: valid JSON for json, name=value pairs joined with & for form, empty for none")
    var body: String

    var draft: RequestDraft {
        let m: String = switch method {
        case .GET: "GET"
        case .POST: "POST"
        case .PUT: "PUT"
        case .PATCH: "PATCH"
        case .DELETE: "DELETE"
        case .HEAD: "HEAD"
        case .OPTIONS: "OPTIONS"
        }
        let b: String = switch bodyKind {
        case .empty: "none"
        case .json: "json"
        case .text: "text"
        case .form: "form"
        }
        return RequestDraft(name: name, method: m, url: url, headers: headers.map { .init(name: $0.name, value: $0.value) }, bodyKind: b, body: body)
    }
}
