import Foundation
import XCTest
@testable import ConverterPresentation

final class ConverterCommandClientTests: XCTestCase {
    func testClientPassesIndependentArgvElementsWithoutAShell() async throws {
        let client = ConverterCommandClient()
        let result = try await client.run(
            ConverterCommand(
                executable: URL(fileURLWithPath: "/usr/bin/printf"),
                arguments: ["%s|%s", "source with spaces/user2", "--dry-run"]
            )
        )

        XCTAssertEqual(String(decoding: result.stdout, as: UTF8.self), "source with spaces/user2|--dry-run")
    }

    func testClientKeepsNonzeroExitAndStderrAsAnError() async {
        let client = ConverterCommandClient()

        do {
            _ = try await client.run(
                ConverterCommand(executable: URL(fileURLWithPath: "/usr/bin/false"), arguments: [])
            )
            XCTFail("a nonzero child exit must not be reported as success")
        } catch let error as ConverterCommandError {
            guard case let .failed(exitCode, _) = error else {
                return XCTFail("unexpected command error: \(error)")
            }
            XCTAssertNotEqual(exitCode, 0)
        } catch {
            XCTFail("unexpected error type: \(error)")
        }
    }
}
