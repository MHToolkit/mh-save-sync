import AppKit
import Foundation

struct MacConfig: Codable {
    var serverURL: String?

    enum CodingKeys: String, CodingKey {
        case serverURL = "server_url"
    }
}

struct MacSyncContext {
    let serverURL: String?
    let profile: String
    let emulator: String
    let saveRootHint: String

    var serverLabel: String {
        guard let serverURL, !serverURL.isEmpty else {
            return "未配置（请设置 MH_SAVE_SYNC_SERVER_URL）"
        }
        return serverURL
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
        return MacConfig(serverURL: nil)
    }
    return (try? JSONDecoder().decode(MacConfig.self, from: data)) ?? MacConfig(serverURL: nil)
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
    MacSyncContext(
        serverURL: configuredServerURL(),
        profile: "MH3G / macOS Nemessix",
        emulator: "Nemessix",
        saveRootHint: "~/Library/Application Support/Nemessix/sdmc/Nintendo 3DS/.../data/00000001/"
    )
}

func persistServerURL(_ raw: String) throws -> String {
    guard let serverURL = normalizedServerURL(raw) else {
        throw CommandFailure(command: ["MHSaveSyncMac", "--set-server-url"], status: 2, stderr: "服务器地址不能为空。\n")
    }
    let dir = configDirectory()
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    let data = try encoder.encode(MacConfig(serverURL: serverURL))
    try data.write(to: configFileURL(), options: [.atomic])
    return serverURL
}

func saveServerURL(_ raw: String) throws {
    let serverURL = try persistServerURL(raw)
    print("已保存服务器地址：\(serverURL)")
    print("配置文件：~/Library/Application Support/MH Save Sync/config.json")
    print("Mac 和 Android 请填写同一个服务器地址；服务器只保存端到端加密快照。")
}

func printStatus(_ context: MacSyncContext) {
    print("""
    MH 云存档同步 · macOS Alpha
    同步到服务器：\(context.serverLabel)
    当前同步对象：\(context.profile)
    模拟器：\(context.emulator)
    存档目录提示：\(context.saveRootHint)
    自动化边界：FSEvents 只标记 dirty；退出/稳定窗口后才快照上传；运行中禁止云端覆盖本地。
    本机 App：运行 ./scripts/install-macos-app.sh 后打开 /Applications/MH Save Sync.app；菜单里可直接设置服务器地址。
    常用命令：--set-server-url <url> / --prelaunch-check / --server-upload / --server-status / --server-restore / --app
    """)
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

        - 云端已有 MH3G 版本：\(snapshot.isEmpty ? "unknown" : snapshot)。不会自动打开 Nemessix；请先选择恢复云端、本地继续或保留冲突分支。
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
    2. 云端覆盖本地：先下载到缓存，确认 Nemessix 已停止，备份当前本地，再 atomic replace。
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
    if let configured = ProcessInfo.processInfo.environment["MH_SAVE_SYNC_CLI"], !configured.isEmpty {
        return configured
    }
    return FileManager.default.currentDirectoryPath + "/target/debug/mh-save"
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

@MainActor
final class MenuController: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem?
    private var serverMenuItem: NSMenuItem?
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
        let serverItem = NSMenuItem(title: "同步到：\(context.serverLabel)", action: nil, keyEquivalent: "")
        menu.addItem(serverItem)
        serverMenuItem = serverItem
        menu.addItem(NSMenuItem(title: "对象：\(context.profile)", action: nil, keyEquivalent: ""))
        menu.addItem(NSMenuItem.separator())
        menu.addItem(NSMenuItem(title: "设置服务器地址…", action: #selector(promptServerURL), keyEquivalent: "s"))
        menu.addItem(NSMenuItem(title: "启动前检查", action: #selector(showPrelaunch), keyEquivalent: "p"))
        menu.addItem(NSMenuItem(title: "继续本地并打开 Nemessix", action: #selector(continueLocalAndOpenNemessix), keyEquivalent: "o"))
        menu.addItem(NSMenuItem(title: "查看冲突处理", action: #selector(showConflict), keyEquivalent: "c"))
        menu.addItem(NSMenuItem(title: "云端不可用策略", action: #selector(showCloudUnavailable), keyEquivalent: "u"))
        menu.addItem(NSMenuItem.separator())
        menu.addItem(NSMenuItem(title: "退出", action: #selector(quit), keyEquivalent: "q"))
        menu.items.forEach { $0.target = self }
        item.menu = menu
        statusItem = item
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
            serverMenuItem?.title = "同步到：\(context.serverLabel)"
            showAlert(
                title: "服务器地址已保存",
                message: "当前同步到：\(saved)\nAndroid App 里也填写同一个地址，两个客户端才会同步到同一套云端快照。"
            )
        } catch {
            showAlert(title: "服务器地址无效", message: "\(error)")
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
    } else if args.contains("--help") {
        print("用法：MHSaveSyncMac [--status] [--set-server-url <url>] [--prelaunch-check] [--continue-local] [--conflict-demo] [--cloud-unavailable] [--server-upload --root <path> --secret-hex <hex>] [--server-status --secret-hex <hex>] [--server-restore --target <path> --secret-hex <hex> --emulator-state stopped|running] [--app]\n运行 ./scripts/install-macos-app.sh 可安装 /Applications/MH Save Sync.app；双击后进入菜单栏模式，菜单内可设置服务器地址、启动前检查、继续本地并打开 Nemessix，并读取 ~/Library/Application Support/MH Save Sync/config.json。")
    } else {
        printStatus(context)
    }
} catch {
    FileHandle.standardError.write(Data("\(error)\n".utf8))
    exit(1)
}
