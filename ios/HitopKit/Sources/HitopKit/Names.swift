import Foundation

/// Ids and path checks for collection folders (spec sections 1 and 6).
public enum Names {
    /// FNV-1a 64 of `key`, low 53 bits (fits a JavaScript number).
    public static func derivedID(_ key: String) -> UInt64 {
        var h: UInt64 = 0xcbf2_9ce4_8422_2325
        for b in key.utf8 {
            h ^= UInt64(b)
            h = h &* 0x0000_0100_0000_01b3
        }
        return h & ((1 << 53) - 1)
    }

    /// The id of a request file without an `id`: from `requests/<path>`.
    public static func requestID(_ path: String) -> UInt64 { derivedID("requests/\(path)") }

    /// The id of a collection without an `id`: from `collection/<name>`.
    public static func collectionID(_ name: String) -> UInt64 { derivedID("collection/\(name)") }

    /// A path read from a collection file (`order` entries, request files):
    /// relative, `/`-separated, no empty, `.`, `..` or hidden segments, no `\`
    /// or `:`. Folders come from untrusted repositories, so nothing may point
    /// outside the folder.
    public static func checkRelPath(_ path: String) throws {
        if path.isEmpty { throw FormatError("empty path") }
        for seg in path.split(separator: "/", omittingEmptySubsequences: false) {
            if seg.isEmpty || seg.hasPrefix(".") || seg.contains("\\") || seg.contains(":") || seg.contains("\0") {
                throw FormatError("invalid path '\(path)': use relative paths like 'orders/create-order'")
            }
        }
    }
}
