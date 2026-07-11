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
