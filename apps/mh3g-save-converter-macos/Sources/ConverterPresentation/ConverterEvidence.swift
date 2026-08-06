import Foundation

enum ConverterEvidence {
    static func isValidSHA256(_ value: String?) -> Bool {
        guard let value, value.count == 64 else { return false }
        return value.utf8.allSatisfy { byte in
            (48...57).contains(byte) || (97...102).contains(byte)
        }
    }

    static func hasPath(_ value: String?) -> Bool {
        guard let value else { return false }
        return !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    static func path(_ value: String?, equals expected: URL) -> Bool {
        guard hasPath(value), let value else { return false }
        return URL(fileURLWithPath: value).standardizedFileURL == expected.standardizedFileURL
    }
}
