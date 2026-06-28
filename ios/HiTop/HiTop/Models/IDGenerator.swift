import Foundation

enum IDGenerator {
    private static var counter: UInt64 = UInt64(Date().timeIntervalSince1970 * 1000)

    static func next() -> UInt64 {
        counter += 1
        return counter
    }
}
