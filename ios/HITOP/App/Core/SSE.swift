import Foundation

/// One Server-Sent Event as the Events view lists it.
struct SSEEvent: Hashable, Sendable, Identifiable {
    /// Its place in the stream (0, 1, …).
    var id: Int
    var event: String
    var data: String
    /// The `id:` field, when the event has one.
    var lastEventID: String?
    var at: Date
}

/// Server-Sent Events, the web app's rules (`features.js` `parseEvents`, `asksForEvents`): events
/// end at a blank line; `data:` lines join with "\n"; `event:` names it ("message" by default);
/// `id:` is kept; comment lines (`:`) and events without data are dropped; one space after the
/// colon is taken off.
enum SSE {
    static let contentType = "text/event-stream"

    /// `Accept: text/event-stream` on an enabled row.
    static func asksForEvents(_ headers: [KeyValue]) -> Bool {
        headers.contains { $0.enabled && $0.name.lowercased() == "accept" && $0.value.lowercased().contains(contentType) }
    }

    static func isEventStream(contentType value: String) -> Bool { value.lowercased().contains(contentType) }

    struct Parsed: Equatable {
        var event: String
        var data: String
        var id: String?
    }

    /// The complete events in `text` and what is left after the last blank line.
    static func parse(_ text: String) -> (events: [Parsed], rest: String) {
        // Blocks end at "\n\n" (or "\r\n\r\n"); the last piece is not finished yet.
        let normal = text.replacingOccurrences(of: "\r\n", with: "\n")
        var blocks = normal.components(separatedBy: "\n\n")
        let rest = blocks.removeLast()
        var out: [Parsed] = []
        for block in blocks {
            var name = "message"
            var data: [String] = []
            var id: String?
            for line in block.split(separator: "\n", omittingEmptySubsequences: true) {
                if line.hasPrefix(":") { continue }
                let field: Substring
                var value: Substring
                if let colon = line.firstIndex(of: ":") {
                    field = line[..<colon]
                    value = line[line.index(after: colon)...]
                    if value.hasPrefix(" ") { value = value.dropFirst() }
                } else {
                    field = line
                    value = ""
                }
                switch field {
                case "data": data.append(String(value))
                case "event": name = value.isEmpty ? "message" : String(value)
                case "id": id = String(value)
                default: break
                }
            }
            if !data.isEmpty { out.append(Parsed(event: name, data: data.joined(separator: "\n"), id: id)) }
        }
        return (out, rest)
    }

    /// Reads a stream piece by piece: bytes may end in the middle of a UTF-8 character or an event.
    struct Reader: Sendable {
        private var pending = Data()
        private var rest = ""
        private(set) var count = 0

        init() {}

        /// The events completed by `chunk`.
        mutating func feed(_ chunk: Data, at: Date = Date()) -> [SSEEvent] {
            pending.append(chunk)
            // Up to the last newline: never inside a multi-byte character.
            guard let last = pending.lastIndex(of: 0x0A) else { return [] }
            let head = pending[pending.startIndex...last]
            pending = Data(pending[pending.index(after: last)...])
            return take(rest + String(decoding: head, as: UTF8.self), at: at)
        }

        /// A whole body that came back at once (not read as a stream): its events, the last one
        /// ended by the end of the body (the web app parses `body + "\n\n"`).
        mutating func finish(at: Date = Date()) -> [SSEEvent] {
            let text = rest + String(decoding: pending, as: UTF8.self) + "\n\n"
            pending = Data()
            return take(text, at: at)
        }

        private mutating func take(_ text: String, at: Date) -> [SSEEvent] {
            let (events, left) = SSE.parse(text)
            rest = left
            return events.map { p in
                defer { count += 1 }
                return SSEEvent(id: count, event: p.event, data: p.data, lastEventID: p.id, at: at)
            }
        }
    }

    /// The events of a whole body (an event stream that came back complete).
    static func events(inBody body: Data, at: Date = Date()) -> [SSEEvent] {
        var r = Reader()
        return r.feed(body, at: at) + r.finish(at: at)
    }
}

/// The editor's hold on a streamed response: events as they arrive, and Stop.
final class StreamControl: @unchecked Sendable {
    private let lock = NSLock()
    private var _stopped = false
    private var _events: [SSEEvent] = []
    private var reader = SSE.Reader()
    /// Called (on the client's thread) when the response turned out to be an event stream, then
    /// with every event as it completes.
    let onOpen: @Sendable (Int, [[String]]) -> Void
    let onEvents: @Sendable ([SSEEvent]) -> Void

    init(onOpen: @escaping @Sendable (Int, [[String]]) -> Void = { _, _ in }, onEvents: @escaping @Sendable ([SSEEvent]) -> Void = { _ in }) {
        self.onOpen = onOpen
        self.onEvents = onEvents
    }

    var isStopped: Bool { lock.withLock { _stopped } }
    var events: [SSEEvent] { lock.withLock { _events } }

    /// Ends the stream: the response is what came so far (scripts and tests run on it).
    func stop() { lock.withLock { _stopped = true } }

    func feed(_ chunk: Data) {
        let new: [SSEEvent] = lock.withLock {
            let e = reader.feed(chunk)
            _events += e
            if _events.count > 1000 { _events.removeFirst(_events.count - 1000) }
            return e
        }
        if !new.isEmpty { onEvents(new) }
    }
}
