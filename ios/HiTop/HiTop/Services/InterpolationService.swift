import Foundation

enum InterpolationService {
    // Replaces ${varName} tokens with values from variables dict.
    static func interpolate(_ input: String, variables: [String: String]) -> String {
        var result = input
        for (key, value) in variables {
            result = result.replacingOccurrences(of: "${\(key)}", with: value)
        }
        return result
    }

    static func interpolateHeaders(_ headers: [Header], variables: [String: String]) -> [Header] {
        headers.map { h in
            Header(
                name: interpolate(h.name, variables: variables),
                value: interpolate(h.value, variables: variables)
            )
        }
    }
}
