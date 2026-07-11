import Foundation

public enum SyncResultKind {
    case upload
    case status
    case restore
}

public func shortSnapshotID(_ value: Any?) -> String? {
    guard let value = value as? String, !value.isEmpty else { return nil }
    return String(value.prefix(8))
}

public func presentSyncResult(_ raw: String, kind: SyncResultKind) -> String {
    let jsonText: String
    if let start = raw.firstIndex(of: "{"), let end = raw.lastIndex(of: "}"), start <= end {
        jsonText = String(raw[start...end])
    } else {
        jsonText = raw
    }
    guard let data = jsonText.data(using: .utf8),
          let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    else {
        let firstUsefulLine = raw.split(separator: "\n")
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first { !$0.isEmpty }
        return firstUsefulLine ?? "操作已完成。"
    }

    switch kind {
    case .upload:
        return uploadSummary(json)
    case .status:
        return statusSummary(json)
    case .restore:
        return restoreSummary(json)
    }
}

private func uploadSummary(_ json: [String: Any]) -> String {
    let outcome = json["outcome"] as? String
    let files = json["file_count"] as? Int ?? 0
    let bytes = json["total_bytes"] as? Int ?? 0
    let snapshot = shortSnapshotID(json["snapshot_id"]) ?? "未知"
    if outcome == "conflict" {
        let cloud = shortSnapshotID(json["cloud_head"]) ?? "未知"
        return """
        检测到冲突，云端版本没有被覆盖。

        本地版本：\(snapshot)…（已安全保存为分支）
        云端版本：\(cloud)…（仍是当前版本）
        内容：\(files) 个文件 · \(ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file))

        下一步：打开“冲突与差异”，确认后再选择保留哪一边。
        """
    }
    let title: String
    switch outcome {
    case "first-snapshot": title = "首次云端备份完成"
    case "fast-forward": title = "云端已更新为本地版本"
    case "up-to-date": title = "本地与云端已经一致"
    default: title = "上传完成"
    }
    return """
    \(title)

    版本：\(snapshot)…
    内容：\(files) 个文件 · \(ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file))
    本地原始存档未移动。
    """
}

private func statusSummary(_ json: [String: Any]) -> String {
    let head = shortSnapshotID(json["cloud_head"])
    let history = json["history_count"] as? Int ?? 0
    let conflicts = json["conflict_count"] as? Int ?? 0
    let conflictLine = conflicts == 0
        ? "冲突：无"
        : "冲突：\(conflicts) 个分支待处理（不会自动覆盖）"
    return """
    云端版本：\(head.map { "\($0)…" } ?? "尚无备份")
    历史版本：\(history) 个
    \(conflictLine)

    \(conflicts == 0 ? "当前没有需要处理的冲突。" : "请打开“冲突与差异”查看文件级变化。")
    """
}

private func restoreSummary(_ json: [String: Any]) -> String {
    let snapshot = shortSnapshotID(json["snapshot_id"]) ?? "未知"
    let files = json["file_count"] as? Int ?? 0
    let bytes = json["total_bytes"] as? Int ?? 0
    return """
    云端版本已恢复到 Mac

    版本：\(snapshot)…
    内容：\(files) 个文件 · \(ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file))
    恢复前的本地存档已备份，可用于回滚。
    """
}
