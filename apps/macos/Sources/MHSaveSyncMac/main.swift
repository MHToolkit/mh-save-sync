import AppKit
import Foundation

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

let context = MacSyncContext(
    serverURL: ProcessInfo.processInfo.environment["MH_SAVE_SYNC_SERVER_URL"],
    profile: "MH3G / macOS Nemessix",
    emulator: "Nemessix",
    saveRootHint: "~/Library/Application Support/Nemessix/sdmc/Nintendo 3DS/.../data/00000001/"
)

func printStatus(_ context: MacSyncContext) {
    print("""
    MH 云存档同步 · macOS Alpha
    同步到服务器：\(context.serverLabel)
    当前同步对象：\(context.profile)
    模拟器：\(context.emulator)
    存档目录提示：\(context.saveRootHint)
    自动化边界：FSEvents 只标记 dirty；退出/稳定窗口后才快照上传；运行中禁止云端覆盖本地。
    常用命令：--prelaunch-check / --server-upload / --server-status / --server-restore / --app
    """)
}

func printPrelaunchCheck(_ context: MacSyncContext) {
    if context.serverURL?.isEmpty ?? true {
        print("""
        启动前检查：云端未配置
        - 你可以继续使用本地 Mac 存档游玩。
        - 不会把空云端或不可用云端覆盖到本地。
        - 配置 MH_SAVE_SYNC_SERVER_URL 后，Mac/Android 会同步到同一个服务器。
        """)
        return
    }
    print("""
    启动前检查：\(context.profile)
    - 服务器：\(context.serverLabel)
    - 若云端较新：先下载到本地 CAS 缓存，展示版本信息，用户确认后才恢复。
    - 若本地/云端冲突：列出 device、时间、parent、大小/hash；用户选择「本地替换云端」或「云端覆盖本地」。
    - 若云端不可用：明确提示，可继续本地游玩，退出后排队补传。
    - 恢复前置：Nemessix 必须停止，且先快照当前本地状态。
    """)
}

func printConflictDemo() {
    print("""
    冲突示例（不会静默 last-write-wins）
    本地：Mac Nemessix · 今天 17:35 · parent=snap-a · 53 KB · hash=63ae25d28d41
    云端：Android Nemessix · 今天 21:18 · parent=snap-a · 47 KB · hash=dd93905a1a8e
    可选动作：
    1. 本地替换云端：上传 Mac 当前快照为新的 HEAD，云端旧版本保留为冲突分支。
    2. 云端覆盖本地：先下载到缓存，确认 Nemessix 已停止，备份当前本地，再 atomic replace。
    3. 暂不处理：继续本地游玩，但状态保持 conflict，不会自动覆盖任何一边。
    """)
}

func printCloudUnavailable() {
    print("""
    云端不可用策略
    - 本地原始存档永远不因远端故障被破坏。
    - 可以继续使用本地存档；后台保留待上传队列。
    - 恢复网络后执行退出对账/手动同步，按 DAG parent 判断 fast-forward 或 conflict。
    """)
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
            stderr: "未配置服务器地址：请设置 MH_SAVE_SYNC_SERVER_URL。\n"
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

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.button?.title = "MH 云存档"
        item.button?.toolTip = "MH 云存档同步 · macOS Alpha"
        let menu = NSMenu()
        menu.addItem(NSMenuItem(title: "同步到：\(context.serverLabel)", action: nil, keyEquivalent: ""))
        menu.addItem(NSMenuItem(title: "对象：\(context.profile)", action: nil, keyEquivalent: ""))
        menu.addItem(NSMenuItem.separator())
        menu.addItem(NSMenuItem(title: "启动前检查", action: #selector(showPrelaunch), keyEquivalent: "p"))
        menu.addItem(NSMenuItem(title: "查看冲突处理", action: #selector(showConflict), keyEquivalent: "c"))
        menu.addItem(NSMenuItem(title: "云端不可用策略", action: #selector(showCloudUnavailable), keyEquivalent: "u"))
        menu.addItem(NSMenuItem.separator())
        menu.addItem(NSMenuItem(title: "退出", action: #selector(quit), keyEquivalent: "q"))
        menu.items.forEach { $0.target = self }
        item.menu = menu
        statusItem = item
    }

    @objc private func showPrelaunch() {
        showAlert(
            title: "启动 MH3G 前检查",
            message: """
            服务器：\(context.serverLabel)
            若云端较新，只会先下载到缓存；恢复必须等 Nemessix 停止，并先备份本地。
            若发生冲突，列出本地/云端版本后由你选择，不做静默覆盖。
            """
        )
    }

    @objc private func showConflict() {
        showAlert(
            title: "冲突处理",
            message: """
            本地：Mac Nemessix · parent=snap-a · 53 KB · hash=63ae25d28d41
            云端：Android Nemessix · parent=snap-a · 47 KB · hash=dd93905a1a8e
            可选：云端覆盖本地 / 本地替换云端 / 暂不处理。两边历史都会保留。
            """
        )
    }

    @objc private func showCloudUnavailable() {
        showAlert(
            title: "云端不可用",
            message: "可以继续使用本地存档；本地快照和上传队列保留，远端恢复后再补传。不会覆盖本地原始存档。"
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
func runMenuBarApp() {
    let delegate = MenuController()
    NSApplication.shared.delegate = delegate
    NSApplication.shared.run()
}

let args = Array(CommandLine.arguments.dropFirst())
do {
    let launchedFromAppBundle =
        args.isEmpty && Bundle.main.bundlePath.hasSuffix(".app")
    if args.contains("--app") || launchedFromAppBundle {
        runMenuBarApp()
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
    } else if args.contains("--help") {
        print("用法：MHSaveSyncMac [--status] [--prelaunch-check] [--conflict-demo] [--cloud-unavailable] [--server-upload --root <path> --secret-hex <hex>] [--server-status --secret-hex <hex>] [--server-restore --target <path> --secret-hex <hex> --emulator-state stopped|running] [--app]\n双击 artifacts/macos/MH Save Sync.app 会自动进入菜单栏模式。")
    } else {
        printStatus(context)
    }
} catch {
    FileHandle.standardError.write(Data("\(error)\n".utf8))
    exit(1)
}
