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

public enum GitHubReleaseAtomFeed {
    public static func stableRelease(from data: Data, expectedReleaseURL: URL) throws -> GitHubConverterRelease {
        guard isOfficialTagURL(expectedReleaseURL),
              let tagName = expectedReleaseURL.lastPathComponent.removingPercentEncoding,
              ConverterSemanticVersion(tagName) != nil
        else {
            throw GitHubReleaseAtomFeedError.invalidReleaseURL
        }

        let delegate = GitHubReleaseAtomParser()
        let parser = XMLParser(data: data)
        parser.delegate = delegate
        guard parser.parse(), parser.parserError == nil else {
            throw GitHubReleaseAtomFeedError.invalidXML
        }
        guard let entry = delegate.entries.first(where: {
            $0.link?.absoluteString == expectedReleaseURL.absoluteString
        }) else {
            throw GitHubReleaseAtomFeedError.releaseNotFound
        }

        let release = GitHubConverterRelease(
            tagName: tagName,
            name: entry.title.nilIfBlank,
            body: plainText(fromHTML: entry.content).nilIfBlank,
            htmlURL: expectedReleaseURL,
            publishedAt: entry.updated.nilIfBlank,
            draft: false,
            prerelease: false
        )
        guard release.isOfficialStableRelease else {
            throw GitHubReleaseAtomFeedError.invalidReleaseURL
        }
        return release
    }

    public static func isOfficialTagURL(_ url: URL) -> Bool {
        url.scheme == "https"
            && url.host?.lowercased() == "github.com"
            && url.query == nil
            && url.fragment == nil
            && url.path.hasPrefix("/MHToolkit/mh-save-sync/releases/tag/")
            && url.pathComponents.count == 6
    }

    private static func plainText(fromHTML html: String) -> String {
        var text = html.replacingOccurrences(
            of: #"(?i)<li(?:\s[^>]*)?>"#,
            with: "• ",
            options: .regularExpression
        )
        text = text.replacingOccurrences(
            of: #"(?i)<br\s*/?>|</li\s*>|</p\s*>|</h[1-6]\s*>|</blockquote\s*>|</pre\s*>"#,
            with: "\n",
            options: .regularExpression
        )
        text = text.replacingOccurrences(of: #"<[^>]+>"#, with: "", options: .regularExpression)
        return text
            .split(whereSeparator: \Character.isNewline)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
            .joined(separator: "\n")
            .prefix(32_000)
            .description
    }
}

private enum GitHubReleaseAtomFeedError: Error {
    case invalidReleaseURL
    case invalidXML
    case releaseNotFound
}

private final class GitHubReleaseAtomParser: NSObject, XMLParserDelegate {
    struct Entry {
        var title = ""
        var updated = ""
        var content = ""
        var link: URL?
    }

    private(set) var entries: [Entry] = []
    private var currentEntry: Entry?
    private var capturedElement: String?
    private var characters = ""

    func parser(
        _ parser: XMLParser,
        didStartElement elementName: String,
        namespaceURI: String?,
        qualifiedName qName: String?,
        attributes attributeDict: [String: String] = [:]
    ) {
        if elementName == "entry" {
            currentEntry = Entry()
            return
        }
        guard currentEntry != nil else { return }
        if elementName == "link",
           attributeDict["rel"] == "alternate",
           let href = attributeDict["href"],
           let url = URL(string: href)
        {
            currentEntry?.link = url
        } else if elementName == "title" || elementName == "updated" || elementName == "content" {
            capturedElement = elementName
            characters = ""
        }
    }

    func parser(_ parser: XMLParser, foundCharacters string: String) {
        guard capturedElement != nil else { return }
        characters += string
    }

    func parser(
        _ parser: XMLParser,
        didEndElement elementName: String,
        namespaceURI: String?,
        qualifiedName qName: String?
    ) {
        if elementName == capturedElement {
            switch elementName {
            case "title":
                currentEntry?.title = characters.trimmingCharacters(in: .whitespacesAndNewlines)
            case "updated":
                currentEntry?.updated = characters.trimmingCharacters(in: .whitespacesAndNewlines)
            case "content":
                currentEntry?.content = characters
            default:
                break
            }
            capturedElement = nil
            characters = ""
        }
        if elementName == "entry", let currentEntry {
            entries.append(currentEntry)
            self.currentEntry = nil
        }
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
    var nilIfBlank: String? {
        let trimmed = trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    func trimmingPrefix(_ prefix: Character) -> String {
        first == prefix ? String(dropFirst()) : self
    }
}
