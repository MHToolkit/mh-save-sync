@preconcurrency import Foundation

/// The only production process bridge.  It hands an argv array directly to
/// `Process`, starts both pipe drains before launch completion, and resumes the
/// caller only after the child terminates and both streams are fully collected.
/// No shell is involved.
public struct ConverterCommandClient: ConverterCommandExecuting, Sendable {
    public init() {}

    public func run(_ command: ConverterCommand) async throws -> ConverterCommandResult {
        let result: ConverterCommandResult = try await withCheckedThrowingContinuation { continuation in
            let process = Process()
            let stdout = Pipe()
            let stderr = Pipe()
            process.executableURL = command.executable
            process.arguments = command.arguments
            process.standardOutput = stdout
            process.standardError = stderr

            let stdoutDrain = Task.detached(priority: .userInitiated) {
                stdout.fileHandleForReading.readDataToEndOfFile()
            }
            let stderrDrain = Task.detached(priority: .userInitiated) {
                stderr.fileHandleForReading.readDataToEndOfFile()
            }

            process.terminationHandler = { terminated in
                Task.detached(priority: .userInitiated) {
                    let output = await stdoutDrain.value
                    let error = await stderrDrain.value
                    continuation.resume(
                        returning: ConverterCommandResult(
                            exitCode: terminated.terminationStatus,
                            stdout: output,
                            stderr: error
                        )
                    )
                }
            }

            do {
                try process.run()
            } catch {
                stdout.fileHandleForReading.closeFile()
                stderr.fileHandleForReading.closeFile()
                continuation.resume(throwing: ConverterCommandError.launchFailed(error.localizedDescription))
            }
        }

        guard result.exitCode == 0 else {
            throw ConverterCommandError.failed(
                exitCode: result.exitCode,
                stderr: String(decoding: result.stderr, as: UTF8.self)
            )
        }
        return result
    }

    public func decodeReport(_ result: ConverterCommandResult) throws -> ConverterReport {
        do {
            return try JSONDecoder().decode(ConverterReport.self, from: result.stdout)
        } catch {
            throw ConverterCommandError.invalidJSON(error.localizedDescription)
        }
    }
}
