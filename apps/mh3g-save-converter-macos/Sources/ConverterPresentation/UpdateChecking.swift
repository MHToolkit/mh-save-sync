import Foundation

public struct ConverterSemanticVersion: Comparable, Equatable, Sendable, CustomStringConvertible {
    public let components: [Int]

    public init?(_ value: String) {
        let normalized = value
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .trimmingPrefix("v")
            .split(separator: "+", maxSplits: 1, omittingEmptySubsequences: true)[0]
            .split(separator: "-", maxSplits: 1, omittingEmptySubsequences: true)[0]
        let parts = normalized.split(separator: ".", omittingEmptySubsequences: false)
        guard !parts.isEmpty,
              parts.allSatisfy({ !$0.isEmpty && $0.allSatisfy(\.isNumber) }),
              parts.allSatisfy({ Int($0) != nil })
        else {
            return nil
        }
        components = parts.map { Int($0)! }
    }

    public static func < (lhs: Self, rhs: Self) -> Bool {
        let count = max(lhs.components.count, rhs.components.count)
        for index in 0..<count {
            let left = index < lhs.components.count ? lhs.components[index] : 0
            let right = index < rhs.components.count ? rhs.components[index] : 0
            if left != right {
                return left < right
            }
        }
        return false
    }

    public static func == (lhs: Self, rhs: Self) -> Bool {
        !(lhs < rhs) && !(rhs < lhs)
    }

    public var description: String {
        components.map(String.init).joined(separator: ".")
    }
}

public struct GitHubConverterRelease: Codable, Equatable, Identifiable, Sendable {
    public let tagName: String
    public let name: String?
    public let body: String?
    public let htmlURL: URL
    public let publishedAt: String?
    public let draft: Bool
    public let prerelease: Bool

    public var id: String { tagName }

    public var isOfficialStableRelease: Bool {
        !draft
            && !prerelease
            && htmlURL.scheme == "https"
            && htmlURL.host?.lowercased() == "github.com"
            && htmlURL.path.hasPrefix("/MHToolkit/mh-save-sync/releases/")
    }

    public init(
        tagName: String,
        name: String?,
        body: String?,
        htmlURL: URL,
        publishedAt: String?,
        draft: Bool,
        prerelease: Bool
    ) {
        self.tagName = tagName
        self.name = name
        self.body = body
        self.htmlURL = htmlURL
        self.publishedAt = publishedAt
        self.draft = draft
        self.prerelease = prerelease
    }

    enum CodingKeys: String, CodingKey {
        case tagName = "tag_name"
        case name
        case body
        case htmlURL = "html_url"
        case publishedAt = "published_at"
        case draft
        case prerelease
    }
}

public enum ConverterUpdateDecision: Equatable, Sendable {
    case updateAvailable
    case upToDate
    case invalidVersion

    public static func decide(current: String, latest: String) -> Self {
        guard let currentVersion = ConverterSemanticVersion(current),
              let latestVersion = ConverterSemanticVersion(latest)
        else {
            return .invalidVersion
        }
        return currentVersion < latestVersion ? .updateAvailable : .upToDate
    }
}

public enum DailyUpdateCheckGate {
    public static func shouldCheck(
        lastAttempt: Date?,
        now: Date,
        calendar: Calendar = .current
    ) -> Bool {
        guard let lastAttempt else { return true }
        return !calendar.isDate(lastAttempt, inSameDayAs: now)
    }
}

private extension String {
    func trimmingPrefix(_ prefix: Character) -> String {
        first == prefix ? String(dropFirst()) : self
    }
}
