import Foundation
import Testing
@testable import MacPresentation

@Test func conflictUploadHidesRawJSONAndTechnicalIdentifiers() {
    let raw = #"{"server_url":"http://example.invalid","account_handle":"secret-account","logical_save_id":"logical-save","snapshot_id":"f058d19a515d51fb","cloud_head":"4d59cc8328a85e35","outcome":"conflict","file_count":2,"total_bytes":47616,"message_zh":"very long"}"#
    let result = presentSyncResult(raw, kind: .upload)
    #expect(result.contains("检测到冲突"))
    #expect(result.contains("云端版本没有被覆盖"))
    #expect(result.contains("f058d19a…"))
    #expect(!result.contains("server_url"))
    #expect(!result.contains("account_handle"))
    #expect(!result.contains("logical_save_id"))
    #expect(!result.contains("{"))
}

@Test func conflictExplainsTheSessionBaseInsteadOfLookingRandom() {
    let raw = #"{"snapshot_id":"f058d19a515d51fb","cloud_head":"4d59cc8328a85e35","outcome":"conflict","file_count":2,"total_bytes":47616}"#
    let known = presentSyncResult(raw, kind: .upload, sessionBaseHead: "1111222233334444")
    #expect(known.contains("本次游玩前的云端版本：11112222…"))
    #expect(known.contains("云端后来变成：4d59cc83…"))
    #expect(known.contains("因此没有自动覆盖"))

    let unknown = presentSyncResult(raw, kind: .upload, sessionBaseHead: nil)
    #expect(unknown.contains("没有可信的游玩前基线"))
    #expect(unknown.contains("先用云端恢复本地，再开始游戏"))
}

@Test func extractsHeadOnlyFromSuccessfulHeadEstablishingResults() {
    #expect(headEstablishedBySyncResult(#"{"outcome":"fast-forward","snapshot_id":"abc123"}"#) == "abc123")
    #expect(headEstablishedBySyncResult(#"{"outcome":"up-to-date","snapshot_id":"def456"}"#) == "def456")
    #expect(headEstablishedBySyncResult(#"{"outcome":"first-snapshot","snapshot_id":"ghi789"}"#) == "ghi789")
    #expect(headEstablishedBySyncResult(#"{"outcome":"conflict","snapshot_id":"branch","cloud_head":"head"}"#) == nil)
    #expect(headEstablishedByRestoreResult(#"{"snapshot_id":"restored123"}"#) == "restored123")
    #expect(headFromStatusResult(#"{"cloud_head":"observed123"}"#) == "observed123")
}

@Test func sessionLedgerKeepsTheHeadObservedBeforePlay() throws {
    var ledger = SaveSessionLedger()
    ledger.recordEstablishedHead(logicalSaveID: "mh3g", head: "before-play")
    ledger.beginSession(logicalSaveID: "mh3g", observedCloudHead: "before-play")
    #expect(ledger.baseHeadForUpload(logicalSaveID: "mh3g") == "before-play")

    // A later cloud observation must not be substituted at exit.
    ledger.observeStatus(logicalSaveID: "mh3g", cloudHead: "changed-elsewhere")
    #expect(ledger.baseHeadForUpload(logicalSaveID: "mh3g") == "before-play")

    let roundTrip = try JSONDecoder().decode(
        SaveSessionLedger.self,
        from: JSONEncoder().encode(ledger)
    )
    #expect(roundTrip.baseHeadForUpload(logicalSaveID: "mh3g") == "before-play")
}

@Test func observingRemoteHeadNeverClaimsStaleLocalSaveDescendsFromIt() {
    var ledger = SaveSessionLedger()
    ledger.beginSession(logicalSaveID: "mh3g", observedCloudHead: "phone-updated-cloud")
    #expect(ledger.baseHeadForUpload(logicalSaveID: "mh3g") == nil)

    ledger.recordEstablishedHead(logicalSaveID: "mh3g", head: "previously-restored")
    ledger.beginSession(logicalSaveID: "mh3g", observedCloudHead: "phone-updated-again")
    #expect(ledger.baseHeadForUpload(logicalSaveID: "mh3g") == "previously-restored")
}

@Test func missingLaunchObservationStaysUnknownAndSuccessfulSyncEstablishesHead() {
    var ledger = SaveSessionLedger()
    ledger.beginSession(logicalSaveID: "mh3g", observedCloudHead: nil)
    ledger.observeStatus(logicalSaveID: "mh3g", cloudHead: "latest-at-exit")
    #expect(ledger.baseHeadForUpload(logicalSaveID: "mh3g") == nil)

    ledger.recordEstablishedHead(logicalSaveID: "mh3g", head: "fast-forwarded")
    #expect(ledger.baseHeadForUpload(logicalSaveID: "mh3g") == "fast-forwarded")
}

@Test func statusUsesCompactChineseSummary() {
    let raw = #"{"cloud_head":"4d59cc8328a85e35","history_count":4,"conflict_count":2,"conflict_diffs":[]}"#
    let result = presentSyncResult(raw, kind: .status)
    #expect(result.contains("云端版本：4d59cc83…"))
    #expect(result.contains("历史版本：4 个"))
    #expect(result.contains("冲突：2 个分支待处理"))
    #expect(!result.contains("conflict_diffs"))
}

@Test func restoreDoesNotExposeLocalPaths() {
    let raw = #"{"snapshot_id":"0123456789abcdef","restored":"/Users/person/private-save","backup":"/Users/person/private-backup","file_count":2,"total_bytes":1024}"#
    let result = presentSyncResult(raw, kind: .restore)
    #expect(result.contains("云端版本已恢复到 Mac"))
    #expect(result.contains("本地存档已备份"))
    #expect(!result.contains("/Users/"))
}

@Test func nonJSONFallbackShowsOnlyFirstUsefulLine() {
    let result = presentSyncResult("完成\n内部诊断\n更多内容", kind: .upload)
    #expect(result == "完成")
}

@Test func jsonFollowedByDiagnosticTextStillGetsSummarized() {
    let raw = #"{"outcome":"fast-forward","snapshot_id":"abcdef0123456789","file_count":2,"total_bytes":2048}"#
        + "\n触发来源：菜单栏手动同步。"
    let result = presentSyncResult(raw, kind: .upload)
    #expect(result.contains("云端已更新为本地版本"))
    #expect(!result.contains("outcome"))
    #expect(!result.contains("触发来源"))
}

@Test func primaryMenuCopyMakesEverySyncDirectionDiscoverable() {
    #expect(MenuCopy.syncNow == "同步存档…")
    #expect(MenuCopy.uploadLocal == "上传本地存档")
    #expect(MenuCopy.restoreCloud == "用云端恢复本地…")
    #expect(MenuCopy.conflicts == "处理冲突…")
    #expect(MenuCopy.cloudStatus == "云端状态")
}

@Test func onboardingPromptIsShortAndActionable() {
    let text = onboardingPrompt(missingServer: true, missingSaveRoot: true, missingSecret: true)
    #expect(text == "还差 3 项设置。先填写服务器地址。")
    #expect(!text.contains("同步路线"))
    #expect(!text.contains("第一步"))
}

@Test func onboardingPromptAdvancesToNextMissingSetting() {
    #expect(onboardingPrompt(missingServer: false, missingSaveRoot: true, missingSecret: true) ==
        "还差 2 项设置。请选择 Nemessix 存档目录。")
    #expect(onboardingPrompt(missingServer: false, missingSaveRoot: false, missingSecret: true) ==
        "还差 1 项设置。请选择或生成恢复密钥。")
    #expect(onboardingPrompt(missingServer: false, missingSaveRoot: false, missingSecret: false) ==
        "设置完成，可以同步。")
}
