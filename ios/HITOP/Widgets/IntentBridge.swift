import Foundation

/// The extension's side of the intents its views start. They are `LiveActivityIntent`s, which the
/// system performs in the app's process (the app's `IntentBridge` does the work there); if one
/// ever runs here, it says to open HITOP: the extension has no collections, runner or secrets.
@MainActor
enum IntentBridge {
    static func send(requestID: String) async throws -> String {
        throw IntentFailure("Open HITOP to send this request.")
    }

    static func runLast() async throws -> String {
        throw IntentFailure("Open HITOP to run the collection.")
    }
}
