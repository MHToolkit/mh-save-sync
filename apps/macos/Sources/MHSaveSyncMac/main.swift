import AppKit
import Foundation

struct MacConfig: Codable {
    var serverURL: String? = nil
    var saveRootPath: String? = nil
    var recoverySecretFile: String? = nil
    var autoUploadOnExit: Bool? = nil

    enum CodingKeys: String, CodingKey {
        case serverURL = "server_url"
        case saveRootPath = "save_root_path"
        case recoverySecretFile = "recovery_secret_file"
        case autoUploadOnExit = "auto_upload_on_exit"
    }
}

struct MacSyncContext {
    let serverURL: String?
    let profile: String
    let emulator: String
    let saveRootHint: String
    let saveRootPath: String?
    let recoverySecretFile: String?
    let autoUploadOnExit: Bool

    var serverLabel: String {
        guard let serverURL, !serverURL.isEmpty else {
            return "未配置（请设置服务器地址）"
        }
        return serverURL
    }

    var saveRootLabel: String {
        guard let saveRootPath, !saveRootPath.isEmpty else {
            return "未配置（请选择 Mac Nemessix 存档目录）"
        }
        return saveRootPath
    }

    var recoverySecretFileLabel: String {
        guard let recoverySecretFile, !recoverySecretFile.isEmpty else {
            return "未配置（请选择 ~/Documents/Secrets 下的恢复密钥文件）"
        }
        return recoverySecretFile
    }

    var autoUploadLabel: String {
        autoUploadOnExit ? "已开启：菜单栏运行时，检测到 Nemessix 退出后上传稳定快照" : "已关闭：只手动同步"
    }

    var hasServerURL: Bool {
        !(serverURL?.isEmpty ?? true)
    }

    var hasSaveRoot: Bool {
        !(saveRootPath?.isEmpty ?? true)
    }

    var hasRecoverySecretFile: Bool {
        !(recoverySecretFile?.isEmpty ?? true)
    }

    var onboardingComplete: Bool {
        hasServerURL && hasSaveRoot && hasRecoverySecretFile
    }
}


private let mh3gNemessixLogicalSaveID =
    "243773e91e82488191606da57fbe807ae3c04958e4c571f5e9c7f3fdb29a41d2"

struct HttpProbeResult {
    let statusCode: Int?
    let body: String
    let error: String?
}

func configDirectory() -> URL {
    let home = ProcessInfo.processInfo.environment["HOME"]
        .map { URL(fileURLWithPath: $0, isDirectory: true) }
        ?? FileManager.default.homeDirectoryForCurrentUser
    return home
        .appendingPathComponent("Library", isDirectory: true)
        .appendingPathComponent("Application Support", isDirectory: true)
        .appendingPathComponent("MH Save Sync", isDirectory: true)
}

func configFileURL() -> URL {
    configDirectory().appendingPathComponent("config.json")
}


func documentsSecretsDirectory() -> URL {
    let home = ProcessInfo.processInfo.environment["HOME"]
        .map { URL(fileURLWithPath: $0, isDirectory: true) }
        ?? FileManager.default.homeDirectoryForCurrentUser
    return home
        .appendingPathComponent("Documents", isDirectory: true)
        .appendingPathComponent("Secrets", isDirectory: true)
}

func expandedStandardPath(_ raw: String?) -> String? {
    let trimmed = raw?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    guard !trimmed.isEmpty else { return nil }
    let expanded: String
    if trimmed == "~" {
        expanded = FileManager.default.homeDirectoryForCurrentUser.path
    } else if trimmed.hasPrefix("~/") {
        expanded = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(String(trimmed.dropFirst(2)))
            .path
    } else {
        expanded = trimmed
    }
    return URL(fileURLWithPath: expanded).standardizedFileURL.path
}

func isPathUnder(_ path: String, root: URL) -> Bool {
    let value = URL(fileURLWithPath: path).standardizedFileURL.path
    let rootPath = root.standardizedFileURL.path
    return value == rootPath || value.hasPrefix(rootPath + "/")
}

func httpGet(_ url: URL, timeout: TimeInterval = 2.5) -> HttpProbeResult {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/curl")
    process.arguments = [
        "-m", String(format: "%.1f", timeout),
        "-sS",
        "-w", "\n%{http_code}",
        url.absoluteString,
    ]
    let stdout = Pipe()
    let stderr = Pipe()
    process.standardOutput = stdout
    process.standardError = stderr
    do {
        try process.run()
        process.waitUntilExit()
    } catch {
        return HttpProbeResult(statusCode: nil, body: "", error: "\(error)")
    }
    let out = String(data: stdout.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
    let err = String(data: stderr.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
    let lines = out.split(separator: "\n", omittingEmptySubsequences: false)
    guard let statusText = lines.last, let statusCode = Int(statusText.trimmingCharacters(in: .whitespacesAndNewlines)) else {
        return HttpProbeResult(statusCode: nil, body: out, error: err.isEmpty ? "no-status" : err)
    }
    let body = lines.dropLast().joined(separator: "\n")
    let error = process.terminationStatus == 0 ? nil : (err.isEmpty ? "curl-exit-\(process.terminationStatus)" : err)
    return HttpProbeResult(statusCode: statusCode, body: body, error: error)
}

func trimSnapshotBody(_ raw: String) -> String {
    raw.trimmingCharacters(in: .whitespacesAndNewlines)
        .trimmingCharacters(in: CharacterSet(charactersIn: "\""))
}

func loadConfig() -> MacConfig {
    let url = configFileURL()
    guard let data = try? Data(contentsOf: url) else {
        return MacConfig()
    }
    return (try? JSONDecoder().decode(MacConfig.self, from: data)) ?? MacConfig()
}

func saveConfig(_ config: MacConfig) throws {
    let dir = configDirectory()
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    let data = try encoder.encode(config)
    try data.write(to: configFileURL(), options: [.atomic])
}


func normalizedServerURL(_ raw: String?) -> String? {
    var trimmed = raw?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    while trimmed.count > 1 && trimmed.hasSuffix("/") {
        trimmed.removeLast()
    }
    return trimmed.isEmpty ? nil : trimmed
}

func configuredServerURL() -> String? {
    if let envURL = normalizedServerURL(ProcessInfo.processInfo.environment["MH_SAVE_SYNC_SERVER_URL"]) {
        return envURL
    }
    return normalizedServerURL(loadConfig().serverURL)
}

func loadContext() -> MacSyncContext {
    let config = loadConfig()
    return MacSyncContext(
        serverURL: configuredServerURL(),
        profile: "MH3G / macOS Nemessix",
        emulator: "Nemessix",
        saveRootHint: "~/Library/Application Support/Nemessix/sdmc/Nintendo 3DS/.../data/00000001/",
        saveRootPath: expandedStandardPath(config.saveRootPath),
        recoverySecretFile: expandedStandardPath(config.recoverySecretFile),
        autoUploadOnExit: config.autoUploadOnExit ?? true
    )
}


func persistServerURL(_ raw: String) throws -> String {
    guard let serverURL = normalizedServerURL(raw) else {
        throw CommandFailure(command: ["MHSaveSyncMac", "--set-server-url"], status: 2, stderr: "服务器地址不能为空。\n")
    }
    var config = loadConfig()
    config.serverURL = serverURL
    try saveConfig(config)
    return serverURL
}

func persistSaveRootPath(_ raw: String) throws -> String {
    guard let path = expandedStandardPath(raw) else {
        throw CommandFailure(command: ["MHSaveSyncMac", "--set-save-root"], status: 2, stderr: "Mac 存档目录不能为空。\n")
    }
    var isDirectory: ObjCBool = false
    guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory), isDirectory.boolValue else {
        throw CommandFailure(command: ["MHSaveSyncMac", "--set-save-root"], status: 2, stderr: "Mac 存档目录不存在或不是目录：\(path)\n")
    }
    var config = loadConfig()
    config.saveRootPath = path
    try saveConfig(config)
    return path
}

func persistRecoverySecretFile(_ raw: String) throws -> String {
    guard let path = expandedStandardPath(raw) else {
        throw CommandFailure(command: ["MHSaveSyncMac", "--set-recovery-secret-file"], status: 2, stderr: "恢复密钥文件不能为空。\n")
    }
    var isDirectory: ObjCBool = false
    guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory), !isDirectory.boolValue else {
        throw CommandFailure(command: ["MHSaveSyncMac", "--set-recovery-secret-file"], status: 2, stderr: "恢复密钥文件不存在或不是文件。请把 64 位 hex 恢复密钥放在 ~/Documents/Secrets 下。\n")
    }
    guard isPathUnder(path, root: documentsSecretsDirectory()) else {
        throw CommandFailure(command: ["MHSaveSyncMac", "--set-recovery-secret-file"], status: 2, stderr: "恢复密钥文件必须放在 ~/Documents/Secrets 下；配置文件只保存文件路径，不保存密钥内容。\n")
    }
    var config = loadConfig()
    config.recoverySecretFile = path
    try saveConfig(config)
    return path
}

func persistAutoUploadOnExit(_ enabled: Bool) throws -> Bool {
    var config = loadConfig()
    config.autoUploadOnExit = enabled
    try saveConfig(config)
    return enabled
}

func saveServerURL(_ raw: String) throws {
    let serverURL = try persistServerURL(raw)
    print("已保存服务器地址：\(serverURL)")
    print("配置文件：~/Library/Application Support/MH Save Sync/config.json")
    print("下一步：选择 Mac Nemessix 存档目录和 ~/Documents/Secrets 下的恢复密钥文件，然后在菜单栏点「立即上传 Mac 存档到服务器」。")
}

func saveRootPath(_ raw: String) throws {
    let path = try persistSaveRootPath(raw)
    print("已保存 Mac Nemessix 存档目录：\(path)")
    print("不会立刻上传；手动上传或检测到 Nemessix 退出后，才会创建稳定快照。")
}

func saveRecoverySecretFile(_ raw: String) throws {
    let path = try persistRecoverySecretFile(raw)
    print("已保存恢复密钥文件路径：\(path)")
    print("配置文件只保存路径，不保存恢复密钥内容；菜单动作执行时才读取该文件。")
}

func saveAutoUploadOnExit(_ raw: String) throws {
    let normalized = raw.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    let enabled: Bool
    if ["on", "true", "1", "yes", "开", "开启"].contains(normalized) {
        enabled = true
    } else if ["off", "false", "0", "no", "关", "关闭"].contains(normalized) {
        enabled = false
    } else {
        throw CommandFailure(command: ["MHSaveSyncMac", "--auto-upload-on-exit"], status: 2, stderr: "参数必须是 on/off。\n")
    }
    _ = try persistAutoUploadOnExit(enabled)
    print(enabled ? "已开启：菜单栏运行时，检测到 Nemessix 退出后上传稳定快照。" : "已关闭：只保留手动同步。")
}


func printStatus(_ context: MacSyncContext) {
    print("""
    MH 云存档同步 · macOS Alpha
    下一步：\(nextActionText(context))
    同步到服务器：\(context.serverLabel)
    当前同步对象：\(context.profile)
    模拟器：\(context.emulator)
    Mac 存档目录：\(context.saveRootLabel)
    存档目录提示：\(context.saveRootHint)
    恢复密钥文件：\(context.recoverySecretFileLabel)
    自动同步：\(context.autoUploadLabel)
    菜单栏可点动作：启动前检查 / 立即上传 Mac 存档到服务器 / 查看云端状态 / 云端覆盖本地（先备份，需停止 Nemessix） / 我已退出 MH3G：立即对账上传。
    自动化边界：菜单栏只监听 Nemessix 进程退出并触发稳定快照；文件变化只提醒工具复查；运行中禁止云端覆盖本地。
    本机 App：运行 ./scripts/install-macos-app.sh 后打开 /Applications/MH Save Sync.app；屏幕右上角出现「MH 云存档」。
    常用命令：--set-server-url <url> / --set-save-root <path> / --set-recovery-secret-file <path> / --auto-upload-on-exit on|off / --prelaunch-check / --server-upload / --server-status / --server-restore / --app
    """)
}

func nextActionText(_ context: MacSyncContext) -> String {
    if !context.hasServerURL {
        return "设置服务器地址；Mac 和 Android 必须填同一个地址。"
    }
    if !context.hasSaveRoot {
        return "选择 Mac Nemessix 存档目录；通常是 MH3G 的 data/00000001 目录。"
    }
    if !context.hasRecoverySecretFile {
        return "选择恢复密钥文件；必须在 ~/Documents/Secrets 下，配置只保存路径。"
    }
    return "启动 MH3G 前点「启动前检查」；退出后点「我已退出 MH3G：立即对账上传」，或开启自动同步。"
}

func onboardingChecklistText(_ context: MacSyncContext) -> String {
    let server = context.hasServerURL ? "✅ 服务器：\(context.serverLabel)" : "⬜ 服务器：点「设置服务器地址…」"
    let saveRoot = context.hasSaveRoot ? "✅ Mac 存档目录：\(context.saveRootLabel)" : "⬜ Mac 存档目录：点「选择 Mac Nemessix 存档目录…」"
    let secret = context.hasRecoverySecretFile ? "✅ 恢复密钥文件：已配置文件" : "⬜ 恢复密钥文件：点「选择恢复密钥文件…」"
    return """
    当前配置进度：
    \(server)
    \(saveRoot)
    \(secret)

    下一步：\(nextActionText(context))
    """
}

func quickGuideText(_ context: MacSyncContext) -> String {
    """
    你现在用的是菜单栏 App，不会出现在 Dock。请看屏幕右上角的「MH 云存档」。

    \(onboardingChecklistText(context))

    第一次使用：
    1. 点「设置服务器地址…」，Mac 和 Android 填同一个地址。
    2. 点「选择 Mac Nemessix 存档目录…」。
    3. 点「选择恢复密钥文件…」，文件必须放在 ~/Documents/Secrets；配置只保存路径，不保存密钥内容。
    4. 启动 MH3G 前点「启动前检查」。退出后点「我已退出 MH3G：立即对账上传」，或开启自动同步让菜单栏检测 Nemessix 退出后上传。

    手动同步：点「立即上传 Mac 存档到服务器」。
    查看云端：点「查看云端状态」。
    从云端恢复：点「云端覆盖本地」，但 Nemessix 必须先退出，恢复前会先备份当前本地。

    底线：运行中的 Nemessix 不会被云端覆盖；文件变化不会直接上传，必须等稳定快照通过校验。
    """
}


func prelaunchCheckText(_ context: MacSyncContext) -> String {
    if context.serverURL?.isEmpty ?? true {
        return """
        启动前检查：云端未配置
        - 你可以继续使用本地 Mac 存档游玩。
        - 不会把空云端或不可用云端覆盖到本地。
        - 配置 MH_SAVE_SYNC_SERVER_URL 后，Mac/Android 会同步到同一个服务器。
        """
    }
    let serverURL = context.serverLabel
    guard let readyURL = URL(string: "\(serverURL)/ready") else {
        return """
        启动前检查：服务器地址无效
        - 服务器：\(serverURL)
        - 不会打开 Nemessix，也不会修改本地存档。
        """
    }
    let ready = httpGet(readyURL)
    guard let readyStatus = ready.statusCode, (200...299).contains(readyStatus) else {
        return """
        启动前检查：云端暂时不可用
        - 服务器：\(serverURL)
        - 详情：\(ready.error ?? "ready=\(ready.statusCode.map(String.init) ?? "no-response")")
        - 可以继续使用本地 Mac 存档游玩；退出后本地队列会在云端恢复后再补传。
        - 不会把空云端或不可用云端覆盖到本地。
        """
    }
    let headURL = URL(string: "\(serverURL)/v1/heads/\(mh3gNemessixLogicalSaveID)")!
    let head = httpGet(headURL)
    var report = """
    启动前检查：\(context.profile)
    - 服务器：\(context.serverLabel)
    - 云端连通：服务器健康检查已通过。
    """
    if let headStatus = head.statusCode, (200...299).contains(headStatus) {
        let snapshot = trimSnapshotBody(head.body)
        report += """

        - 云端已有 MH3G 版本：\(snapshot.isEmpty ? "版本信息暂不可读" : snapshot)。不会自动打开 Nemessix；请先选择恢复云端、本地继续或保留冲突分支。
        """
    } else if head.statusCode == 404 {
        report += """

        - 云端还没有 MH3G 版本：可以启动本地游戏；退出后本地稳定快照会上传到服务器。
        """
    } else {
        report += """

        - 云端版本查询失败：\(head.error ?? "head=\(head.statusCode.map(String.init) ?? "no-response")")。可以继续使用本地；退出后排队补传。
        """
    }
    report += """

    - 若云端较新：先下载到本机安全缓存，展示版本信息，用户确认后才恢复。
    - 若本地/云端冲突：列出设备、时间、上一版、大小和校验摘要；用户选择「本地替换云端」或「云端覆盖本地」。
    - 若云端不可用：明确提示，可继续本地游玩，退出后排队补传。
    - 恢复前置：Nemessix 必须停止，且先快照当前本地状态。
    """
    return report
}

func printPrelaunchCheck(_ context: MacSyncContext) {
    print(prelaunchCheckText(context))
}

func printConflictDemo() {
    print("""
    冲突示例（不会静默 last-write-wins）
    本地：Mac Nemessix · 时间=本机快照时间 · 上一版=snap-a · 53 KB · 校验摘要=63ae25d28d41
    云端：Android Nemessix · 时间=云端快照时间 · 上一版=snap-a · 47 KB · 校验摘要=dd93905a1a8e
    可选动作：
    1. 本地替换云端：上传 Mac 当前快照为新的云端版本，云端旧版本保留为冲突分支。
    2. 云端覆盖本地：先下载到缓存，确认 Nemessix 已停止，备份当前本地，再安全替换。
    3. 暂不处理：继续本地游玩，但状态保持冲突待处理，不会自动覆盖任何一边。
    """)
}

func printCloudUnavailable() {
    print("""
    云端不可用策略
    - 本地原始存档永远不因远端故障被破坏。
    - 可以继续使用本地存档；后台保留待上传队列。
    - 恢复网络后执行退出对账/手动同步，按版本父子关系判断可直接推进或需要用户处理冲突。
    """)
}

func printContinueLocal() {
    print("""
    已选择继续使用本地存档
    - 当前不会从云端覆盖本地，也不会把未验证中间态上传。
    - 如果需要打开 Nemessix，请先确认你接受本次离线/本地分支；退出后再执行对账补传。
    - 后续如果云端也发生修改，会进入冲突分支，不会按最新时间静默覆盖。
    """)
}

func openNemessixApp() -> Bool {
    let url = URL(fileURLWithPath: "/Applications/Nemessix.app", isDirectory: true)
    return NSWorkspace.shared.open(url)
}


struct CommandFailure: Error, CustomStringConvertible {
    let command: [String]
    let status: Int32
    let stderr: String

    var description: String {
        "命令执行失败（exit=\(status)）：\(command.joined(separator: " "))\n\(stderr)"
    }
}

func optionValue(_ name: String, in args: [String]) -> String? {
    guard let index = args.firstIndex(of: name), args.indices.contains(index + 1) else {
        return nil
    }
    return args[index + 1]
}

func requireOption(_ name: String, in args: [String]) throws -> String {
    guard let value = optionValue(name, in: args), !value.isEmpty else {
        throw CommandFailure(command: ["MHSaveSyncMac", name], status: 2, stderr: "缺少参数 \(name)\n")
    }
    return value
}

func mhSaveCLIPath() -> String {
    let fm = FileManager.default
    if let configured = ProcessInfo.processInfo.environment["MH_SAVE_SYNC_CLI"], !configured.isEmpty {
        return configured
    }

    let bundled = Bundle.main.bundleURL
        .appendingPathComponent("Contents", isDirectory: true)
        .appendingPathComponent("MacOS", isDirectory: true)
        .appendingPathComponent("mh-save")
        .path
    if fm.isExecutableFile(atPath: bundled) {
        return bundled
    }

    return fm.currentDirectoryPath + "/target/debug/mh-save"
}

func serverURLOrThrow(_ context: MacSyncContext) throws -> String {
    guard let serverURL = context.serverURL, !serverURL.isEmpty else {
        throw CommandFailure(
            command: ["MHSaveSyncMac"],
            status: 2,
            stderr: "未配置服务器地址：请设置 MH_SAVE_SYNC_SERVER_URL，或运行 --set-server-url <url>。\n"
        )
    }
    return serverURL
}

func saveRootOrThrow(_ context: MacSyncContext) throws -> String {
    guard let path = context.saveRootPath, !path.isEmpty else {
        throw CommandFailure(
            command: ["MHSaveSyncMac"],
            status: 2,
            stderr: "未配置 Mac Nemessix 存档目录：请在菜单栏选择目录，或运行 --set-save-root <path>。\n"
        )
    }
    return path
}

func recoverySecretHexOrThrow(_ context: MacSyncContext) throws -> String {
    guard let path = context.recoverySecretFile, !path.isEmpty else {
        throw CommandFailure(
            command: ["MHSaveSyncMac"],
            status: 2,
            stderr: "未配置恢复密钥文件：请在菜单栏选择 ~/Documents/Secrets 下的文件，或运行 --set-recovery-secret-file <path>。\n"
        )
    }
    let data = try Data(contentsOf: URL(fileURLWithPath: path))
    let text = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    let range = NSRange(location: 0, length: text.utf16.count)
    let regex = try NSRegularExpression(pattern: "^[0-9a-fA-F]{64}$")
    guard regex.firstMatch(in: text, options: [], range: range) != nil else {
        throw CommandFailure(
            command: ["MHSaveSyncMac"],
            status: 2,
            stderr: "恢复密钥文件格式不正确：需要 64 位 hex；不会在日志或提示中显示密钥内容。\n"
        )
    }
    return text
}


@discardableResult
func runMHSave(_ arguments: [String]) throws -> String {
    let executable = mhSaveCLIPath()
    let process = Process()
    process.executableURL = URL(fileURLWithPath: executable)
    process.arguments = arguments
    let stdout = Pipe()
    let stderr = Pipe()
    process.standardOutput = stdout
    process.standardError = stderr
    try process.run()
    process.waitUntilExit()
    let out = String(data: stdout.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
    let err = String(data: stderr.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
    if process.terminationStatus != 0 {
        throw CommandFailure(command: [executable] + arguments, status: process.terminationStatus, stderr: err)
    }
    return out
}

func printServerUpload(_ args: [String], context: MacSyncContext) throws {
    let serverURL = try serverURLOrThrow(context)
    let root = try requireOption("--root", in: args)
    let secret = try requireOption("--secret-hex", in: args)
    var cliArgs = [
        "server-upload",
        "--server-url", serverURL,
        "--root", root,
        "--secret-hex", secret,
        "--device-id", optionValue("--device-id", in: args) ?? "macos-nemessix",
    ]
    if let baseHead = optionValue("--base-head", in: args) {
        cliArgs += ["--base-head", baseHead]
    }
    if let logicalSave = optionValue("--logical-save-id", in: args) {
        cliArgs += ["--logical-save-id", logicalSave]
    }
    print(try runMHSave(cliArgs), terminator: "")
}

func printServerStatus(_ args: [String], context: MacSyncContext) throws {
    let serverURL = try serverURLOrThrow(context)
    let secret = try requireOption("--secret-hex", in: args)
    var cliArgs = ["server-status", "--server-url", serverURL, "--secret-hex", secret]
    if let logicalSave = optionValue("--logical-save-id", in: args) {
        cliArgs += ["--logical-save-id", logicalSave]
    }
    print(try runMHSave(cliArgs), terminator: "")
}

func printServerRestore(_ args: [String], context: MacSyncContext) throws {
    let serverURL = try serverURLOrThrow(context)
    let target = try requireOption("--target", in: args)
    let secret = try requireOption("--secret-hex", in: args)
    var cliArgs = [
        "server-restore",
        "--server-url", serverURL,
        "--target", target,
        "--secret-hex", secret,
        "--emulator-state", optionValue("--emulator-state", in: args) ?? "stopped",
    ]
    if let snapshotID = optionValue("--snapshot-id", in: args) {
        cliArgs += ["--snapshot-id", snapshotID]
    }
    if let logicalSave = optionValue("--logical-save-id", in: args) {
        cliArgs += ["--logical-save-id", logicalSave]
    }
    print(try runMHSave(cliArgs), terminator: "")
}

func configuredServerUpload(context: MacSyncContext, reason: String = "manual") throws -> String {
    let serverURL = try serverURLOrThrow(context)
    let root = try saveRootOrThrow(context)
    let secret = try recoverySecretHexOrThrow(context)
    return try runMHSave([
        "server-upload",
        "--server-url", serverURL,
        "--root", root,
        "--secret-hex", secret,
        "--device-id", "macos-nemessix",
        "--logical-save-id", mh3gNemessixLogicalSaveID,
    ]) + "\n触发来源：\(reason)。本地原始存档未移动；上传前由共享引擎创建稳定快照。\n"
}

func configuredServerStatus(context: MacSyncContext) throws -> String {
    let serverURL = try serverURLOrThrow(context)
    let secret = try recoverySecretHexOrThrow(context)
    return try runMHSave([
        "server-status",
        "--server-url", serverURL,
        "--secret-hex", secret,
        "--logical-save-id", mh3gNemessixLogicalSaveID,
    ])
}

func configuredServerRestore(context: MacSyncContext) throws -> String {
    if isNemessixRunning() {
        throw CommandFailure(
            command: ["MHSaveSyncMac"],
            status: 2,
            stderr: "Nemessix 仍在运行：不会把云端存档覆盖到正在运行的模拟器目录。请先退出 MH3G/Nemessix。\n"
        )
    }
    let serverURL = try serverURLOrThrow(context)
    let target = try saveRootOrThrow(context)
    let secret = try recoverySecretHexOrThrow(context)
    return try runMHSave([
        "server-restore",
        "--server-url", serverURL,
        "--target", target,
        "--secret-hex", secret,
        "--logical-save-id", mh3gNemessixLogicalSaveID,
        "--emulator-state", "stopped",
    ])
}

func isNemessixRunning() -> Bool {
    NSWorkspace.shared.runningApplications.contains { app in
        app.bundleIdentifier == "io.github.vincentadamnemessisx.nemessix" ||
            app.localizedName?.lowercased().contains("nemessix") == true
    }
}


@MainActor
final class MenuController: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem?
    private var serverMenuItem: NSMenuItem?
    private var saveRootMenuItem: NSMenuItem?
    private var secretMenuItem: NSMenuItem?
    private var autoUploadMenuItem: NSMenuItem?
    private var syncStateMenuItem: NSMenuItem?
    private var nextActionMenuItem: NSMenuItem?
    private var processTimer: Timer?
    private var wasNemessixRunning = false
    private var context: MacSyncContext

    init(context: MacSyncContext) {
        self.context = context
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.button?.title = "MH 云存档"
        item.button?.toolTip = "MH 云存档同步 · macOS Alpha"
        let menu = NSMenu()
        let serverItem = NSMenuItem(title: "服务器：\(context.serverLabel)", action: nil, keyEquivalent: "")
        menu.addItem(serverItem)
        serverMenuItem = serverItem
        let saveRootItem = NSMenuItem(title: "Mac 存档目录：\(context.saveRootLabel)", action: nil, keyEquivalent: "")
        menu.addItem(saveRootItem)
        saveRootMenuItem = saveRootItem
        let secretItem = NSMenuItem(title: "恢复密钥：\(context.recoverySecretFile == nil ? "未配置" : "已配置文件")", action: nil, keyEquivalent: "")
        menu.addItem(secretItem)
        secretMenuItem = secretItem
        let autoItem = NSMenuItem(title: "自动同步：\(context.autoUploadOnExit ? "退出后自动上传" : "关闭")", action: nil, keyEquivalent: "")
        menu.addItem(autoItem)
        autoUploadMenuItem = autoItem
        let stateItem = NSMenuItem(title: "状态：等待操作", action: nil, keyEquivalent: "")
        menu.addItem(stateItem)
        syncStateMenuItem = stateItem
        let nextItem = NSMenuItem(title: "下一步：\(nextActionText(context))", action: #selector(showQuickGuide), keyEquivalent: "")
        menu.addItem(nextItem)
        nextActionMenuItem = nextItem
        menu.addItem(NSMenuItem.separator())
        menu.addItem(NSMenuItem(title: "设置服务器地址…", action: #selector(promptServerURL), keyEquivalent: "s"))
        menu.addItem(NSMenuItem(title: "选择 Mac Nemessix 存档目录…", action: #selector(promptSaveRoot), keyEquivalent: "r"))
        menu.addItem(NSMenuItem(title: "选择恢复密钥文件…", action: #selector(promptRecoverySecretFile), keyEquivalent: "k"))
        menu.addItem(NSMenuItem(title: "自动同步：退出 Nemessix 后上传", action: #selector(toggleAutoUploadOnExit), keyEquivalent: "a"))
        menu.addItem(NSMenuItem.separator())
        menu.addItem(NSMenuItem(title: "启动前检查", action: #selector(showPrelaunch), keyEquivalent: "p"))
        menu.addItem(NSMenuItem(title: "立即上传 Mac 存档到服务器", action: #selector(uploadNow), keyEquivalent: "u"))
        menu.addItem(NSMenuItem(title: "我已退出 MH3G：立即对账上传", action: #selector(reconcileAfterExit), keyEquivalent: "e"))
        menu.addItem(NSMenuItem(title: "查看云端状态", action: #selector(showServerStatus), keyEquivalent: "h"))
        menu.addItem(NSMenuItem(title: "云端覆盖本地（先备份，需停止 Nemessix）", action: #selector(restoreCloudToLocal), keyEquivalent: "d"))
        menu.addItem(NSMenuItem.separator())
        menu.addItem(NSMenuItem(title: "新手引导：办公室 Mac ↔ 回家 Android", action: #selector(showQuickGuide), keyEquivalent: "g"))
        menu.addItem(NSMenuItem(title: "继续本地并打开 Nemessix", action: #selector(continueLocalAndOpenNemessix), keyEquivalent: "o"))
        menu.addItem(NSMenuItem(title: "查看冲突处理", action: #selector(showConflict), keyEquivalent: "c"))
        menu.addItem(NSMenuItem(title: "云端不可用策略", action: #selector(showCloudUnavailable), keyEquivalent: "y"))
        menu.addItem(NSMenuItem.separator())
        menu.addItem(NSMenuItem(title: "退出", action: #selector(quit), keyEquivalent: "q"))
        menu.items.forEach { $0.target = self }
        item.menu = menu
        statusItem = item
        refreshMenuLabels()
        startProcessExitMonitor()
        showStartupGuideIfNeeded()
    }

    private func showStartupGuideIfNeeded() {
        guard !context.onboardingComplete else { return }
        DispatchQueue.main.async { [weak self] in
            self?.showQuickGuide()
        }
    }

    @objc private func showQuickGuide() {
        refreshContext()
        showAlert(
            title: "MH 云存档已在菜单栏运行",
            message: quickGuideText(context)
        )
    }

    @objc private func promptServerURL() {
        let alert = NSAlert()
        alert.messageText = "设置同步服务器地址"
        alert.informativeText = "Mac 和 Android 必须填写同一个服务器地址。服务器只保存端到端加密快照，不保存恢复密钥或明文存档。"
        alert.addButton(withTitle: "保存")
        alert.addButton(withTitle: "取消")
        let input = NSTextField(frame: NSRect(x: 0, y: 0, width: 420, height: 24))
        input.placeholderString = "例如 http://8.130.112.207:39082"
        input.stringValue = context.serverURL ?? ""
        alert.accessoryView = input
        guard alert.runModal() == .alertFirstButtonReturn else {
            return
        }
        do {
            let saved = try persistServerURL(input.stringValue)
            context = loadContext()
            refreshMenuLabels()
            showAlert(
                title: "服务器地址已保存",
                message: """
                当前同步到：\(saved)
                Android App 里也填写同一个地址，两个客户端才会同步到同一套云端快照。

                \(onboardingChecklistText(context))
                """
            )
        } catch {
            showAlert(title: "服务器地址无效", message: "\(error)")
        }
    }

    private func refreshContext() {
        context = loadContext()
        refreshMenuLabels()
    }

    private func refreshMenuLabels() {
        serverMenuItem?.title = "服务器：\(context.serverLabel)"
        saveRootMenuItem?.title = "Mac 存档目录：\(context.saveRootLabel)"
        secretMenuItem?.title = "恢复密钥：\(context.recoverySecretFile == nil ? "未配置" : "已配置文件")"
        autoUploadMenuItem?.title = "自动同步：\(context.autoUploadOnExit ? "退出后自动上传" : "关闭")"
        nextActionMenuItem?.title = "下一步：\(nextActionText(context))"
    }

    private func setSyncState(_ message: String) {
        syncStateMenuItem?.title = "状态：\(message)"
    }

    private func performSyncAction(title: String, state: String, action: () throws -> String) {
        refreshContext()
        setSyncState(state)
        do {
            let output = try action()
            setSyncState("完成：\(state)")
            showAlert(title: title, message: output)
        } catch {
            setSyncState("失败：\(state)")
            showAlert(title: "\(title)失败", message: "\(error)\n\n不会破坏本地原始存档；请按提示补齐配置或稍后重试。")
        }
    }

    private func startProcessExitMonitor() {
        wasNemessixRunning = isNemessixRunning()
        processTimer?.invalidate()
        processTimer = Timer.scheduledTimer(withTimeInterval: 15, repeats: true) { [weak self] _ in
            Task { @MainActor in
                self?.pollNemessixProcess()
            }
        }
    }

    private func pollNemessixProcess() {
        let running = isNemessixRunning()
        if wasNemessixRunning && !running && context.autoUploadOnExit {
            performSyncAction(title: "Nemessix 已退出，开始自动上传", state: "退出后自动上传") {
                try configuredServerUpload(context: self.context, reason: "Nemessix 退出后自动同步")
            }
        }
        wasNemessixRunning = running
    }

    @objc private func promptSaveRoot() {
        let panel = NSOpenPanel()
        panel.title = "选择 Mac Nemessix 存档目录"
        panel.message = "请选择 MH3G 的 data/00000001 存档目录或该逻辑存档根目录。不会立刻上传。"
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        if let path = context.saveRootPath {
            panel.directoryURL = URL(fileURLWithPath: path, isDirectory: true)
        }
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            let saved = try persistSaveRootPath(url.path)
            refreshContext()
            showAlert(
                title: "Mac 存档目录已保存",
                message: """
                目录：\(saved)
                不会立刻上传；手动上传和退出后自动上传都会先做稳定快照校验。

                \(onboardingChecklistText(context))
                """
            )
        } catch {
            showAlert(title: "Mac 存档目录无效", message: "\(error)")
        }
    }

    @objc private func promptRecoverySecretFile() {
        let panel = NSOpenPanel()
        panel.title = "选择恢复密钥文件"
        panel.message = "请选择 ~/Documents/Secrets 下的 64 位 hex 恢复密钥文件。配置只保存路径，不保存密钥内容。"
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.directoryURL = documentsSecretsDirectory()
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            let saved = try persistRecoverySecretFile(url.path)
            refreshContext()
            showAlert(
                title: "恢复密钥文件已保存",
                message: """
                文件：\(saved)
                密钥内容没有写入配置、日志或提示。

                \(onboardingChecklistText(context))
                """
            )
        } catch {
            showAlert(title: "恢复密钥文件无效", message: "\(error)")
        }
    }

    @objc private func toggleAutoUploadOnExit() {
        do {
            let enabled = try persistAutoUploadOnExit(!context.autoUploadOnExit)
            refreshContext()
            showAlert(
                title: enabled ? "自动同步已开启" : "自动同步已关闭",
                message: enabled
                    ? "菜单栏 App 运行时，会每 15 秒检查 Nemessix 是否从运行变为退出；退出后才上传稳定快照。不会因为文件变化直接上传。"
                    : "已关闭退出后自动上传；仍可手动点击「立即上传 Mac 存档到服务器」。"
            )
        } catch {
            showAlert(title: "自动同步设置失败", message: "\(error)")
        }
    }

    @objc private func uploadNow() {
        performSyncAction(title: "立即上传 Mac 存档到服务器", state: "手动上传") {
            try configuredServerUpload(context: self.context, reason: "菜单栏手动同步")
        }
    }

    @objc private func reconcileAfterExit() {
        if isNemessixRunning() {
            showAlert(title: "Nemessix 仍在运行", message: "请先退出 MH3G/Nemessix。运行中不会上传正在写入的中间态，也不会从云端覆盖本地。")
            return
        }
        performSyncAction(title: "退出后对账上传", state: "退出后对账") {
            try configuredServerUpload(context: self.context, reason: "用户确认已退出 MH3G")
        }
    }

    @objc private func showServerStatus() {
        performSyncAction(title: "云端状态", state: "查看云端") {
            try configuredServerStatus(context: self.context)
        }
    }

    @objc private func restoreCloudToLocal() {
        if isNemessixRunning() {
            showAlert(title: "不能恢复：Nemessix 仍在运行", message: "请先退出 MH3G/Nemessix。运行中绝不把云端内容覆盖到模拟器存档目录。")
            return
        }
        let confirm = NSAlert()
        confirm.messageText = "确认用云端版本覆盖 Mac 本地存档？"
        confirm.informativeText = "执行前会走共享恢复流程：下载云端版本、校验、先备份当前本地存档，再恢复到配置的 Mac Nemessix 存档目录。若不确定，请先点查看云端状态。"
        confirm.addButton(withTitle: "确认恢复")
        confirm.addButton(withTitle: "取消")
        guard confirm.runModal() == .alertFirstButtonReturn else { return }
        performSyncAction(title: "云端覆盖本地", state: "云端恢复") {
            try configuredServerRestore(context: self.context)
        }
    }

    @objc private func showPrelaunch() {
        showAlert(
            title: "启动 MH3G 前检查",
            message: prelaunchCheckText(context)
        )
    }

    @objc private func showConflict() {
        showAlert(
            title: "冲突处理说明",
            message: """
            这是说明页，不会执行覆盖或上传。真正发生冲突时会列出本地与云端的设备、时间、上一版、大小和校验摘要。
            可选：云端覆盖本地 / 本地替换云端 / 暂不处理。两边历史都会保留。
            """
        )
    }

    @objc private func showCloudUnavailable() {
        showAlert(
            title: "云端不可用",
            message: "可以继续使用本地存档；本地快照和上传队列保留，远端恢复后再补传。不会覆盖本地原始存档。若要继续，请点「继续本地并打开 Nemessix」。"
        )
    }

    @objc private func continueLocalAndOpenNemessix() {
        let opened = openNemessixApp()
        showAlert(
            title: "继续使用本地存档",
            message: opened
                ? "已尝试打开 Nemessix。当前不会从云端覆盖本地；退出 MH3G 后再对账补传，若云端也修改过会进入冲突分支。"
                : "没有在 /Applications 找到 Nemessix.app。你仍可手动打开；当前不会从云端覆盖本地，退出后再对账补传。"
        )
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }

    private func showAlert(title: String, message: String) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = message
        alert.addButton(withTitle: "知道了")
        alert.runModal()
    }
}

@MainActor
func runMenuBarApp(context: MacSyncContext) {
    let delegate = MenuController(context: context)
    NSApplication.shared.delegate = delegate
    NSApplication.shared.run()
}

let args = Array(CommandLine.arguments.dropFirst())
do {
    let context = loadContext()
    let launchedFromAppBundle =
        args.isEmpty && Bundle.main.bundlePath.hasSuffix(".app")
    if args.contains("--set-server-url") {
        try saveServerURL(try requireOption("--set-server-url", in: args))
    } else if args.contains("--set-save-root") {
        try saveRootPath(try requireOption("--set-save-root", in: args))
    } else if args.contains("--set-recovery-secret-file") {
        try saveRecoverySecretFile(try requireOption("--set-recovery-secret-file", in: args))
    } else if args.contains("--auto-upload-on-exit") {
        try saveAutoUploadOnExit(try requireOption("--auto-upload-on-exit", in: args))
    } else if args.contains("--app") || launchedFromAppBundle {
        runMenuBarApp(context: context)
    } else if args.contains("--server-upload") {
        try printServerUpload(args, context: context)
    } else if args.contains("--server-status") {
        try printServerStatus(args, context: context)
    } else if args.contains("--server-restore") {
        try printServerRestore(args, context: context)
    } else if args.contains("--prelaunch-check") {
        printPrelaunchCheck(context)
    } else if args.contains("--conflict-demo") {
        printConflictDemo()
    } else if args.contains("--cloud-unavailable") {
        printCloudUnavailable()
    } else if args.contains("--continue-local") {
        printContinueLocal()
    } else if args.contains("--configured-upload") {
        print(try configuredServerUpload(context: context, reason: "CLI configured upload"), terminator: "")
    } else if args.contains("--configured-status") {
        print(try configuredServerStatus(context: context), terminator: "")
    } else if args.contains("--configured-restore") {
        print(try configuredServerRestore(context: context), terminator: "")
    } else if args.contains("--help") {
        print("用法：MHSaveSyncMac [--status] [--set-server-url <url>] [--set-save-root <path>] [--set-recovery-secret-file <path>] [--auto-upload-on-exit on|off] [--prelaunch-check] [--configured-upload] [--configured-status] [--configured-restore] [--continue-local] [--conflict-demo] [--cloud-unavailable] [--server-upload --root <path> --secret-hex <hex>] [--server-status --secret-hex <hex>] [--server-restore --target <path> --secret-hex <hex> --emulator-state stopped|running] [--app]\n运行 ./scripts/install-macos-app.sh 可安装 /Applications/MH Save Sync.app；双击后进入菜单栏模式，屏幕右上角会出现「MH 云存档」。菜单顶部会显示「下一步」，配置后也会继续提示还缺什么；菜单内可点：设置服务器地址…、选择 Mac Nemessix 存档目录…、选择恢复密钥文件…、启动前检查、立即上传 Mac 存档到服务器、我已退出 MH3G：立即对账上传、查看云端状态、云端覆盖本地（先备份，需停止 Nemessix）、自动同步：退出 Nemessix 后上传，并读取 ~/Library/Application Support/MH Save Sync/config.json。")
    } else {
        printStatus(context)
    }
} catch {
    FileHandle.standardError.write(Data("\(error)\n".utf8))
    exit(1)
}
