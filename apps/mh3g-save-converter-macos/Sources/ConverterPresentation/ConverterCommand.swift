import Foundation

public struct ConverterCommand: Equatable, Sendable {
    public let executable: URL
    public let arguments: [String]

    public init(executable: URL, arguments: [String]) {
        self.executable = executable.standardizedFileURL
        self.arguments = arguments
    }
}

public struct ConverterCommandResult: Sendable {
    public let exitCode: Int32
    public let stdout: Data
    public let stderr: Data

    public init(exitCode: Int32, stdout: Data, stderr: Data) {
        self.exitCode = exitCode
        self.stdout = stdout
        self.stderr = stderr
    }
}

public protocol ConverterCommandExecuting: Sendable {
    func run(_ command: ConverterCommand) async throws -> ConverterCommandResult
}

public enum ConverterCommandError: Error, Equatable, Sendable {
    case launchFailed(String)
    case failed(exitCode: Int32, stderr: String)
    case invalidJSON(String)
}
