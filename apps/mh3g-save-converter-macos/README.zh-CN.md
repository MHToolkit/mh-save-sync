# macOS 版 MH3G 存档转换器

[English](README.md)

这是 `mh3g-save-convert` Rust CLI 的原生 SwiftUI 前台。它只用 argv 调用随包
sidecar 并展示 JSON 报告，不在 Swift 中重复实现存档转换。

## 两种处理方式

- **全新转换**：原始 3DS `user#` 转成同名 Cemu `user#`。
- **修复已转换存档**：把原始 3DS `user#` 与玩家继续游玩后的当前 Cemu
  `user#` 作为两个独立只读输入，并把结果写到独立选择的同槽位输出；只修复仍
  保持 0.0.3 至 0.0.6 旧转换结果的字段。

因此修复模式会明确显示三个选择器：原始 3DS、只读当前 Wii U/Cemu 引用、修复后
输出。它还可以额外选择完整 3DS ExtData `user` 目录以修复公会名片。当前 Cemu
`card1`、`card2`、`card3`、`cardbox` 和 `quest1` 至 `quest4` 从所选当前
`user#` 的父目录解析；任务文件只校验并保留，不会被兼容修复改写。若输出目录与
当前引用目录不同，输出目录必须已有初始化的 `card1`、`card2`、`card3`、
`cardbox`。自动检测结果为歧义时，必须明确选择历史版本并重新运行 Dry Run。

核心选择器接受准确 `user1`、`user2`、`user3` 文件或其直接父目录。它不会递归
扫描 SD 卡/MLC，不接受 ZIP、7z、RAR。目录只解析成当前选中槽位的同名直接子文件。

每次写入都绑定紧邻 Dry Run 的 SHA-256。普通转换使用单文件 manifest；兼容修复
使用 `.mh3g-compatibility-repair-<UUID>.json` 并通过 `rollback-repair` 总体回滚。
执行写入或回滚前必须退出 Nemessix、Azahar 和 Cemu。

可选的管家画廊/动画迁移不会只凭 3DS `system` 新建或整文件覆盖共享
`system`。必须同时选择 3DS 源 `system` 和已存在、已初始化的 Cemu `system`；
转换器只合并已知画廊/动画标记，其他 Cemu 字节（包括其他角色槽位共享数据）
全部保留。

## 更新检查

“关于与更新”会通过 `MHToolkit/mh-save-sync` 官方 GitHub Release 页面确认最新
正式版 tag，并读取官方 Atom Release feed。该主路径不消耗共享的 GitHub 匿名
API 配额，Release API 仅作为备用。每个本地自然日首次启动最多静默尝试一次；
GitHub 无法访问时不会阻塞窗口，也不会影响任何本地转换。手动检查会展示错误并
允许重试；发现新版本时会显示版本名、发布日期、Release 说明和官方发布链接。

## 本地验证

```bash
swift test
cd ../..
bash scripts/build-mh3g-save-converter-macos-app.sh
bash scripts/mh3g-save-converter-macos-smoke.sh
```

这些命令使用测试 fixture，不会启动 Cemu 或写入真实 MLC。准确文件范围和 CLI
示例见
[中文文件契约](../../docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.zh-CN.md)。
