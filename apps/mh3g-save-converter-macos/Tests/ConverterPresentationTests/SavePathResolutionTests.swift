import Foundation
import XCTest
@testable import ConverterPresentation

final class SavePathResolutionTests: XCTestCase {
    private var root: URL!
    private let fileManager = FileManager.default

    override func setUpWithError() throws {
        root = fileManager.temporaryDirectory
            .appendingPathComponent("mh3g-save-path-resolution-\(UUID().uuidString)", isDirectory: true)
        try fileManager.createDirectory(at: root, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? fileManager.removeItem(at: root)
    }

    func testSourceDirectoryResolvesTheExplicitSlotChild() throws {
        let sourceDirectory = root.appendingPathComponent("3ds", isDirectory: true)
        try fileManager.createDirectory(at: sourceDirectory, withIntermediateDirectories: true)
        let user2 = sourceDirectory.appendingPathComponent("user2")
        XCTAssertTrue(fileManager.createFile(atPath: user2.path, contents: Data([0x2B])))

        let resolved = try SavePathResolver.resolveSource(selection: sourceDirectory, slot: .user2)

        XCTAssertEqual(resolved, user2.standardizedFileURL)
    }

    func testSourceFileMustMatchTheSelectedSlot() throws {
        let user2 = root.appendingPathComponent("user2")
        XCTAssertTrue(fileManager.createFile(atPath: user2.path, contents: Data([0x2B])))

        XCTAssertThrowsError(try SavePathResolver.resolveSource(selection: user2, slot: .user1)) { error in
            XCTAssertEqual(
                error as? SavePathResolutionError,
                .slotNameMismatch(expected: .user1, actual: "user2")
            )
        }
    }

    func testOutputDirectoryResolvesToSameNamedSlotWithoutCreatingIt() throws {
        let downloads = root.appendingPathComponent("Downloads", isDirectory: true)
        try fileManager.createDirectory(at: downloads, withIntermediateDirectories: true)

        let resolved = try SavePathResolver.resolveTarget(selection: downloads, slot: .user2)

        XCTAssertEqual(resolved, downloads.appendingPathComponent("user2").standardizedFileURL)
        XCTAssertFalse(fileManager.fileExists(atPath: resolved.path))
    }

    func testExtDataRootResolvesOnlyItsDirectUserChild() throws {
        let extDataRoot = root.appendingPathComponent("00000481", isDirectory: true)
        let user = extDataRoot.appendingPathComponent("user", isDirectory: true)
        try fileManager.createDirectory(at: user, withIntermediateDirectories: true)

        let resolved = try SavePathResolver.resolveExtDataUserDirectory(selection: extDataRoot)

        XCTAssertEqual(resolved, user.standardizedFileURL)
    }
}
