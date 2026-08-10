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
            var request = URLRequest(url: Self.releaseEndpoint)
            request.timeoutInterval = 8
            request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
            request.setValue("2022-11-28", forHTTPHeaderField: "X-GitHub-Api-Version")
            request.setValue("MH3GSaveConverter/\(safeUserAgentVersion)", forHTTPHeaderField: "User-Agent")

            let (data, response) = try await session.data(for: request)
            guard let http = response as? HTTPURLResponse, http.statusCode == 200 else {
                throw UpdateCheckError.invalidResponse
            }
            guard data.count <= 2 * 1_024 * 1_024 else {
                throw UpdateCheckError.responseTooLarge
            }

            let release = try JSONDecoder().decode(GitHubConverterRelease.self, from: data)
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
    case invalidResponse
    case responseTooLarge
    case invalidRelease
    case invalidVersion

    var errorDescription: String? {
        switch self {
        case .invalidResponse:
            "GitHub Release API returned an unexpected response."
        case .responseTooLarge:
            "GitHub Release API response exceeded the accepted size."
        case .invalidRelease:
            "GitHub Release metadata did not match the official MHToolkit repository."
        case .invalidVersion:
            "The current or latest release version could not be compared safely."
        }
    }
}
