import ConverterPresentation
import Foundation

enum UpdateCheckStatus: Equatable {
    case idle
    case checking
    case upToDate(String)
    case updateAvailable(String)
    case failed(String)
}

@MainActor
final class GitHubUpdateChecker: ObservableObject {
    static let releaseEndpoint = URL(string: "https://api.github.com/repos/MHToolkit/mh-save-sync/releases/latest")!
    static let releaseWebEndpoint = URL(string: "https://github.com/MHToolkit/mh-save-sync/releases/latest")!
    static let releaseAtomEndpoint = URL(string: "https://github.com/MHToolkit/mh-save-sync/releases.atom")!
    static let lastAttemptDefaultsKey = "MH3GSaveConverter.LastUpdateCheckAttempt"

    @Published private(set) var status: UpdateCheckStatus = .idle
    @Published var availableRelease: GitHubConverterRelease?

    let currentVersion: String

    private let session: URLSession
    private let defaults: UserDefaults

    init(
        currentVersion: String? = nil,
        session: URLSession? = nil,
        defaults: UserDefaults = .standard
    ) {
        self.currentVersion = currentVersion ?? Self.bundleVersion
        self.defaults = defaults
        if let session {
            self.session = session
        } else {
            let configuration = URLSessionConfiguration.ephemeral
            configuration.timeoutIntervalForRequest = 8
            configuration.timeoutIntervalForResource = 12
            self.session = URLSession(configuration: configuration)
        }
    }

    var isChecking: Bool {
        status == .checking
    }

    func checkAutomaticallyIfNeeded(now: Date = .now, calendar: Calendar = .current) async {
        guard !isChecking,
              DailyUpdateCheckGate.shouldCheck(
                  lastAttempt: defaults.object(forKey: Self.lastAttemptDefaultsKey) as? Date,
                  now: now,
                  calendar: calendar
              )
        else {
            return
        }

        // Persist before networking. A blocked or offline GitHub request must
        // not be repeated on every launch during the same local calendar day.
        defaults.set(now, forKey: Self.lastAttemptDefaultsKey)
        await performCheck(manual: false)
    }

    func checkManually() async {
        guard !isChecking else { return }
        await performCheck(manual: true)
    }

    private func performCheck(manual: Bool) async {
        status = .checking
        do {
            let release = try await fetchLatestRelease()
            try validate(release: release)
            switch ConverterUpdateDecision.decide(current: currentVersion, latest: release.tagName) {
            case .updateAvailable:
                availableRelease = release
                status = .updateAvailable(release.tagName)
            case .upToDate:
                availableRelease = nil
                status = .upToDate(release.tagName)
            case .invalidVersion:
                throw UpdateCheckError.invalidVersion
            }
        } catch {
            availableRelease = nil
            // Automatic checks stay silent and never obstruct the local save
            // workflow. Manual checks retain a diagnostic for the About pane.
            status = manual ? .failed(error.localizedDescription) : .idle
        }
    }

    private func fetchLatestRelease() async throws -> GitHubConverterRelease {
        do {
            return try await fetchLatestReleaseFromWebFeed()
        } catch {
            let webFailure = error.localizedDescription
            do {
                return try await fetchLatestReleaseFromAPI()
            } catch {
                throw UpdateCheckError.allSourcesUnavailable(
                    web: webFailure,
                    api: error.localizedDescription
                )
            }
        }
    }

    private func fetchLatestReleaseFromWebFeed() async throws -> GitHubConverterRelease {
        var latestRequest = configuredRequest(url: Self.releaseWebEndpoint, accept: "text/html")
        // One byte is enough: URLSession follows GitHub's /releases/latest
        // redirect, and the final response URL carries the stable tag. This
        // avoids downloading the full release page.
        latestRequest.setValue("bytes=0-0", forHTTPHeaderField: "Range")
        let (redirectProbe, latestResponse) = try await session.data(for: latestRequest)
        guard let latestHTTP = latestResponse as? HTTPURLResponse,
              latestHTTP.statusCode == 200 || latestHTTP.statusCode == 206,
              redirectProbe.count <= 2 * 1_024 * 1_024,
              let stableReleaseURL = latestHTTP.url,
              GitHubReleaseAtomFeed.isOfficialTagURL(stableReleaseURL)
        else {
            throw UpdateCheckError.invalidResponse(source: "GitHub release page")
        }

        let atomRequest = configuredRequest(url: Self.releaseAtomEndpoint, accept: "application/atom+xml")
        let (atomData, atomResponse) = try await session.data(for: atomRequest)
        guard let atomHTTP = atomResponse as? HTTPURLResponse, atomHTTP.statusCode == 200 else {
            throw UpdateCheckError.invalidResponse(source: "GitHub release feed")
        }
        guard atomData.count <= 2 * 1_024 * 1_024 else {
            throw UpdateCheckError.responseTooLarge(source: "GitHub release feed")
        }
        return try GitHubReleaseAtomFeed.stableRelease(
            from: atomData,
            expectedReleaseURL: stableReleaseURL
        )
    }

    private func fetchLatestReleaseFromAPI() async throws -> GitHubConverterRelease {
        var request = configuredRequest(url: Self.releaseEndpoint, accept: "application/vnd.github+json")
        request.setValue("2022-11-28", forHTTPHeaderField: "X-GitHub-Api-Version")
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse, http.statusCode == 200 else {
            let status = (response as? HTTPURLResponse)?.statusCode
            throw UpdateCheckError.invalidResponse(
                source: status.map { "GitHub Release API (HTTP \($0))" } ?? "GitHub Release API"
            )
        }
        guard data.count <= 2 * 1_024 * 1_024 else {
            throw UpdateCheckError.responseTooLarge(source: "GitHub Release API")
        }
        let release = try JSONDecoder().decode(GitHubConverterRelease.self, from: data)
        try validate(release: release)
        return release
    }

    private func configuredRequest(url: URL, accept: String) -> URLRequest {
        var request = URLRequest(url: url)
        request.timeoutInterval = 8
        request.setValue(accept, forHTTPHeaderField: "Accept")
        request.setValue("MH3GSaveConverter/\(safeUserAgentVersion)", forHTTPHeaderField: "User-Agent")
        return request
    }

    private func validate(release: GitHubConverterRelease) throws {
        guard release.isOfficialStableRelease else {
            throw UpdateCheckError.invalidRelease
        }
    }

    private var safeUserAgentVersion: String {
        ConverterSemanticVersion(currentVersion)?.description ?? "0.0.0"
    }

    private static var bundleVersion: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "development"
    }
}

private enum UpdateCheckError: LocalizedError {
    case invalidResponse(source: String)
    case responseTooLarge(source: String)
    case invalidRelease
    case invalidVersion
    case allSourcesUnavailable(web: String, api: String)

    var errorDescription: String? {
        switch self {
        case let .invalidResponse(source):
            "\(source) returned an unexpected response."
        case let .responseTooLarge(source):
            "\(source) response exceeded the accepted size."
        case .invalidRelease:
            "GitHub Release metadata did not match the official MHToolkit repository."
        case .invalidVersion:
            "The current or latest release version could not be compared safely."
        case let .allSourcesUnavailable(web, api):
            "GitHub release page/feed and API were unavailable. Web: \(web) API: \(api)"
        }
    }
}
