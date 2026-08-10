import Foundation
import XCTest
@testable import ConverterPresentation

final class UpdateCheckingTests: XCTestCase {
    func testSemanticVersionAcceptsReleaseTagsAndComparesNumerically() throws {
        let current = try XCTUnwrap(ConverterSemanticVersion("0.0.16"))
        let latest = try XCTUnwrap(ConverterSemanticVersion("v0.0.17"))
        let twoDigit = try XCTUnwrap(ConverterSemanticVersion("0.0.100"))

        XCTAssertLessThan(current, latest)
        XCTAssertLessThan(latest, twoDigit)
        XCTAssertEqual(ConverterSemanticVersion("1.2"), ConverterSemanticVersion("1.2.0"))
    }

    func testMalformedVersionsFailClosed() {
        XCTAssertNil(ConverterSemanticVersion("release-latest"))
        XCTAssertNil(ConverterSemanticVersion("1..2"))
        XCTAssertEqual(
            ConverterUpdateDecision.decide(current: "development", latest: "v0.0.17"),
            .invalidVersion
        )
    }

    func testGitHubReleasePayloadDecodesExpectedFields() throws {
        let payload = #"{"tag_name":"v0.0.17","name":"MH3G 0.0.17","body":"Fix notes","html_url":"https://github.com/MHToolkit/mh-save-sync/releases/tag/v0.0.17","published_at":"2026-08-10T00:00:00Z","draft":false,"prerelease":false}"#.data(using: .utf8)!
        let release = try JSONDecoder().decode(GitHubConverterRelease.self, from: payload)

        XCTAssertEqual(release.tagName, "v0.0.17")
        XCTAssertEqual(release.name, "MH3G 0.0.17")
        XCTAssertEqual(release.body, "Fix notes")
        XCTAssertFalse(release.draft)
        XCTAssertFalse(release.prerelease)
        XCTAssertTrue(release.isOfficialStableRelease)
    }

    func testReleaseLinkMustBelongToTheOfficialRepository() throws {
        let spoofed = GitHubConverterRelease(
            tagName: "v9.9.9",
            name: nil,
            body: nil,
            htmlURL: try XCTUnwrap(URL(string: "https://github.com.evil.example/MHToolkit/mh-save-sync/releases/tag/v9.9.9")),
            publishedAt: nil,
            draft: false,
            prerelease: false
        )

        XCTAssertFalse(spoofed.isOfficialStableRelease)
    }

    func testAtomFeedConvertsTheExpectedStableReleaseAndPlainTextNotes() throws {
        let feed = #"""
        <?xml version="1.0" encoding="UTF-8"?>
        <feed xmlns="http://www.w3.org/2005/Atom">
          <entry>
            <updated>2026-08-10T00:00:00Z</updated>
            <link rel="alternate" type="text/html" href="https://github.com/MHToolkit/mh-save-sync/releases/tag/v0.0.17"/>
            <title>MH3G 0.0.17</title>
            <content type="html">&lt;h2&gt;Fixes&lt;/h2&gt;&lt;ul&gt;&lt;li&gt;Rate-limit fallback&lt;/li&gt;&lt;/ul&gt;</content>
          </entry>
        </feed>
        """#.data(using: .utf8)!
        let expectedURL = try XCTUnwrap(
            URL(string: "https://github.com/MHToolkit/mh-save-sync/releases/tag/v0.0.17")
        )

        let release = try GitHubReleaseAtomFeed.stableRelease(
            from: feed,
            expectedReleaseURL: expectedURL
        )

        XCTAssertEqual(release.tagName, "v0.0.17")
        XCTAssertEqual(release.name, "MH3G 0.0.17")
        XCTAssertEqual(release.body, "Fixes\n• Rate-limit fallback")
        XCTAssertEqual(release.publishedAt, "2026-08-10T00:00:00Z")
        XCTAssertTrue(release.isOfficialStableRelease)
    }

    func testAtomFeedRequiresTheExactOfficialTagURLAndMatchingEntry() throws {
        let feed = #"""
        <feed xmlns="http://www.w3.org/2005/Atom">
          <entry>
            <link rel="alternate" href="https://github.com/MHToolkit/mh-save-sync/releases/tag/v0.0.17"/>
            <title>v0.0.17</title>
          </entry>
        </feed>
        """#.data(using: .utf8)!
        let spoofed = try XCTUnwrap(
            URL(string: "https://github.com.evil.example/MHToolkit/mh-save-sync/releases/tag/v0.0.17")
        )
        let missing = try XCTUnwrap(
            URL(string: "https://github.com/MHToolkit/mh-save-sync/releases/tag/v0.0.18")
        )

        XCTAssertThrowsError(
            try GitHubReleaseAtomFeed.stableRelease(from: feed, expectedReleaseURL: spoofed)
        )
        XCTAssertThrowsError(
            try GitHubReleaseAtomFeed.stableRelease(from: feed, expectedReleaseURL: missing)
        )
    }

    func testDailyGateRunsOnlyOncePerLocalCalendarDay() throws {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = try XCTUnwrap(TimeZone(secondsFromGMT: 8 * 3600))
        let lastAttempt = try XCTUnwrap(calendar.date(from: DateComponents(year: 2026, month: 8, day: 10, hour: 1)))
        let sameDay = try XCTUnwrap(calendar.date(from: DateComponents(year: 2026, month: 8, day: 10, hour: 23)))
        let nextDay = try XCTUnwrap(calendar.date(from: DateComponents(year: 2026, month: 8, day: 11, hour: 0)))

        XCTAssertFalse(DailyUpdateCheckGate.shouldCheck(lastAttempt: lastAttempt, now: sameDay, calendar: calendar))
        XCTAssertTrue(DailyUpdateCheckGate.shouldCheck(lastAttempt: lastAttempt, now: nextDay, calendar: calendar))
        XCTAssertTrue(DailyUpdateCheckGate.shouldCheck(lastAttempt: nil, now: sameDay, calendar: calendar))
    }
}
