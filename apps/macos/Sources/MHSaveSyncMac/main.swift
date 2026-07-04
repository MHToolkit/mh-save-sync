import Foundation

struct SaveSyncStatus: Codable {
    let product: String
    let platform: String
    let role: String
    let restoreWhileRunning: String
    let watcherUpload: String
}

let status = SaveSyncStatus(
    product: "MH Save Sync",
    platform: "macOS",
    role: "menu-bar-shell-spike",
    restoreWhileRunning: "blocked",
    watcherUpload: "dirty-only"
)
let data = try JSONEncoder().encode(status)
FileHandle.standardOutput.write(data)
FileHandle.standardOutput.write(Data("\n".utf8))
