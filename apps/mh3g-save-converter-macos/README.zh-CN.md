# macOS 版 MH3G 存档转换器

[English](README.md)

这是 `mh3g-save-convert` Rust CLI 的原生 SwiftUI 前台。它只用 argv 调用随包
sidecar 并展示 JSON 报告，不在 Swift 中重复实现存档转换。

## 两种处理方式

- **全新转换**：原始 3DS `user#` 转成同名 Cemu `user#`。
- **修复已转换存档**：把原始 3DS `user#` 与玩家继续游玩后的当前 Cemu
  `user#` 作为两个独立只读输入，并把结果写到独立选择的同槽位输出；只修复仍
  保持 0.0.3 至 0.0.6 旧转换结果的字段。

修复模式按数据域拆分：核心槽位、公会名片、任务、共享 `system`、实验性 CEC
各自执行 Dry Run、写入授权、记录 manifest 和回滚。某个可选域失败或配置不完整，
不会阻塞或撤销另一个已经独立授权的域。

每个域都明确显示原始 3DS、只读当前 Wii U/Cemu、输出三种控件，路径值不会在控件
之间自动级联。核心可选准确 `user#` 文件或其直接父目录；`system` 与 CEC 使用
准确文件。公会名片和任务在物理上属于同一个目录，因此共用一组 ExtData
原始/当前/输出目录控件，但操作和 manifest 仍完全分开。输出必须已初始化并包含
完整四个 `card*` 或完整四个 `quest*`；任务修复逐字节保留当前 Wii U 数据。
历史检测为歧义时，必须明确选择 0.0.3-0.0.6 并重新 Dry Run；来自同一次旧转换的
核心、名片和任务会复用这个显式版本。

核心选择器接受准确 `user1`、`user2`、`user3` 文件或其直接父目录。它不会递归
扫描 SD 卡/MLC，不接受 ZIP、7z、RAR。目录只解析成当前选中槽位的同名直接子文件。

每次写入都绑定本域紧邻 Dry Run 的 SHA-256。核心使用
`.mh3g-compatibility-repair-<UUID>.json` 与 `rollback-repair`；名片/任务使用
`rollback-extras`；system 使用 `rollback`；CEC 使用 `rollback-cec`。执行写入或
回滚前必须退出 Nemessix、Azahar 和 Cemu。

可选的管家画廊/动画修复不会只凭 3DS `system` 整文件覆盖共享 `system`。必须
分别选择 3DS 源、当前已初始化 Cemu 权威文件和独立输出；转换器只合并已知
画廊/动画标记，其他当前 Cemu 字节（包括其他角色槽位共享数据）全部保留。

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
