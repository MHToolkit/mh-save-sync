# MH3G Save Converter 原生多端 UI 设计

日期：2026-07-26  
状态：已确认，待实施计划与代码评审

## 1. 目标

为已发布的 `mh3g-save-convert` 构建独立的原生桌面工作台，使用户能够以可视、可审计、可回滚的方式完成日版 MH3G 3DS 到日版 MH3G HD Wii U/Cemu 的本地存档迁移。

该产品属于 `MH Save Sync` 仓库，但不是云同步客户端的一个隐藏页面：它是一个独立、前台、本地离线的 `MH3G Save Converter` 工具。唯一的转换事实源仍是 Rust CLI/库；UI 不解释、不重写、不复制任何存档格式或转换规则。

首发优先级为 macOS 和 Windows；Android 在桌面工作流稳定及设备连通后实施。

## 2. 已确认决策

| 主题 | 决策 |
| --- | --- |
| macOS | 独立 SwiftUI 常规窗口 App，不复用现有 AppKit 菜单栏云同步程序。 |
| Windows | 独立 C# / .NET 8 / WinUI 3 App，不使用 Vue、WebView、Tauri 或 Electron。 |
| Android | 独立 Kotlin / Compose / Android Studio 工程，后置实施。 |
| 转换调用 | App 打包同平台 `mh3g-save-convert` 二进制，以严格 argv 数组启动并消费 JSON；不得通过 shell 字符串执行。 |
| UI 主结构 | 四阶段安全工作台：输入与检查、组件选择、Dry Run、写入与回滚。 |
| UI 语言 | 默认跟随系统语言；首发始终支持简体中文和 English，并允许用户在设置中覆盖。 |
| 图标 | 自有「3DS -> HD」双存档单向迁移图标，不复用 MH Save Sync 云同步标识。 |
| 视觉 | 游戏场景主导的低对比度顶部视觉层；操作与数据在清晰的原生材质内容层上。 |
| CEC | 独立的“实验性 CEC”工具，默认折叠、默认关闭，并有额外确认。 |
| ExtData | 第一版必须支持完整、安全的组事务；不能把现有 staging-only 行为伪装成已安装。 |
| MCP | 0.1 不提供 MCP server；保留 JSON CLI 和 AI/CLI 使用指南，待安全门稳定后再评估。 |

## 3. 非目标

- 不改变日版 `0x2B` profile、现有转换表、保存格式或已验证的 Rust 行为。
- 不支持反向转换、其他地区、其他游戏、云端自动转换或递归“转换整个目录”。
- 不直接读取 ZIP、7z、RAR、QQ/浏览器压缩预览。
- 不在 UI 中绕过模拟器运行门禁、事务、备份、manifest 或回滚。
- 不将实验性 CEC 表述为普通名片/离线伙伴迁移的必需条件。
- 不将 GameHub/CrossOver 的结果表述为原生 Windows 11 最终兼容性结论。

## 4. 工程结构

```text
crates/mh3g-save-convert/
  # 唯一转换、检查、事务、回滚与结构化报告内核

apps/mh3g-save-converter-macos/
  Package.swift
  Sources/
    ConverterPresentation/
    MH3GSaveConverterMac/
  Tests/
  Resources/AppIcon/

apps/mh3g-save-converter-windows/
  MH3GSaveConverter.sln
  src/MH3GSaveConverter/       # WinUI 3
  tests/MH3GSaveConverter.Tests/
  assets/

apps/mh3g-save-converter-android/  # 后置
  app/
```

现有 `apps/macos` 是 `LSUIElement=true` 的 AppKit 菜单栏云同步程序，保留其原职责，不承担转换器窗口。新 macOS App 使用 `WindowGroup`，正常显示于 Dock、Cmd-Tab、窗口菜单与辅助功能树。

Windows App 是原生 WinUI 3，不引入 Vue 或 Node frontend 工具链。两个桌面 App 都分别打包其平台二进制：

```text
MH3G Save Converter.app/Contents/MacOS/
  MH3GSaveConverterMac
  mh3g-save-convert

MH3G Save Converter Windows install root/
  MH3GSaveConverter.exe
  mh3g-save-convert.exe
```

App 内的命令客户端必须异步启动子进程，同时 drain stdout/stderr，再解码 JSON；不得在 UI 主线程上 `waitUntilExit()`，也不得仅靠退出码推断成功。

## 5. Rust 安全前置能力

UI 实施前先补齐以下 CLI/库能力，并以 Rust 测试证明。

### 5.1 Windows 进程门禁

现有运行门禁只在 macOS 可用。新增 Windows `ProcessProbe`，用 Windows 原生进程枚举检测至少：

```text
Cemu.exe
Cemu_release.exe
Nemessix.exe
Azahar.exe
```

写入、批量 ExtData 安装、CEC 写入和任何 rollback 均使用该后端门禁。不能可靠建立进程状态时必须 fail closed；WinUI 的“我已退出模拟器”提示或复选框不能取代后端验证。

### 5.2 ExtData 完整组事务

当前 `convert-extras` 仅能向新 staging 目录写入八个组件。第一版 UI 需要的 Rust 扩展必须：

1. 继续将转换与安装分离：先生成新 staging 集。
2. 将 `card1/card2/card3/cardbox` 视为一个不可拆分的“公会名片与离线伙伴”组。
3. 将 `quest1/quest2/quest3/quest4` 视为一个不可拆分的“任务”组。
4. 校验全部已选 staging 文件及目标路径后，先快照全部已选目标。
5. 使用同目录临时文件和受控 manifest 安装完整组；任一替换失败时恢复每一个已变更目标。
6. 在 manifest 中记录每个选中目标的 before/after SHA-256、备份或“原本不存在”状态、路径及组归属。
7. 只接受完整组选择；禁止单独覆盖某个 `card#` 或 `quest#`。
8. 回滚必须恢复完整 manifest 绑定集合，而不是只恢复第一个失败或最后一个文件。

新命令/库 API 的精确名称在实施计划中确定，但必须输出稳定 JSON，并沿用现有 manifest/backup 命名与 fail-closed 原则。

### 5.3 结构化报告

UI 只能根据 Rust 的 JSON 报告决定状态。每个读取、dry-run、写入或回滚报告至少包含：

- operation 与明确的源/目标路径；
- profile、slot、大小、SHA-256；
- 将写入与明确不会写入的文件集合；
- backup、manifest、history 路径；
- 当前事务阶段与可回滚状态；
- 失败的操作阶段、完整 OS 错误与 stderr；
- CEC 的 `experimental` 状态。

## 6. 工作台体验

首屏直接进入转换工作台，不设置营销 landing page。

```text
迁移流程
  输入与检查
  组件选择
  Dry Run
  写入与回滚

工具
  转换历史
  实验性 CEC
  设置
```

### 6.1 输入与检查

- 选择一个明确的源 `user1`、`user2` 或 `user3`，以及同名 Cemu 目标文件。
- 可选选择 `system`、准确 ExtData `.../00000481/user` 目录与准确 CEC `.../CEC/00048100` 目录。
- 显示 profile、slot、文件大小、SHA-256、完整路径和只读/可写状态。
- UI 可便利地帮助用户选择目录，但绝不递归猜测文件、自动覆盖整个 MLC 或隐式加入未选择组件。

### 6.2 组件选择

核心角色必选。可选组件为：

| UI 组件 | UI 可选粒度 | Rust 实际边界 |
| --- | --- | --- |
| 共享 system | 一个完整 system | 一个明确 system 文件 |
| 公会名片与离线伙伴 | 一整个开关 | card1 + card2 + card3 + cardbox |
| 任务 | 一整个开关 | quest1 + quest2 + quest3 + quest4 |
| 实验性 CEC | 高级页单独开关 | cec 与独立 CEC manifest |

每个数据组有自己可识别的插图、内容解释和读写边界；插图是理解文件范围的方式，不是无功能的装饰卡。

### 6.3 Dry Run

- 先执行 Rust 后端模拟器停止检查。
- 将源、staging、目标、backup 与 manifest 以文件流图和明确表格显示。
- 显示所有将写入与不会写入的文件。
- 写入授权绑定到当前输入 SHA-256、目标 SHA-256、组件集合和 dry-run 报告；任意一个变化都会立即使授权失效。

### 6.4 写入与回滚

- Write 仅在有效 dry-run、输入指纹未变、后端进程门禁通过时可用。
- 原生确认框列出文件数、目标目录、备份数、manifest 路径与实验性 CEC 状态。
- 写入开始后 UI 进入受控执行状态，显示 Rust 报告的真实阶段；不可通过关闭动画或中途取消破坏事务。
- 成功页显示实际备份与 manifest，并提供 manifest-bound “回滚此操作”。
- 错误页显示 operation、完整路径、OS 错误与 stderr；失败绝不显示成成功。

### 6.5 实验性 CEC

CEC 单独位于“实验性 CEC”页面：先只读检查 `InBox___`/`OutBox__`，默认不选写入；开启后必须确认实验属性，dry-run 后才可以写入。现有名片与离线伙伴迁移不依赖 CEC。

## 7. Apple Design 与视觉系统

设计使用 Apple 的目的、可控、责任、熟悉、灵活、简洁、工艺与愉悦原则，而不是跨平台复制 macOS 控件。

### 7.1 材质、层级与画面

- macOS 使用 SwiftUI 系统语义色、`Material`、`NavigationSplitView`、原生 toolbar、sheet、confirmation dialog。
- Windows 使用 WinUI 3 的 Mica/Acrylic、系统主题、`NavigationView`、原生 ContentDialog 与文件选择器。
- Android 后续用 Compose/Material 3 保持同一信息层级和状态语义。
- 顶部使用与当前阶段关联的低对比度 MH3G 场景视觉；正文置于清晰的内容材质，路径、哈希、备份和错误不可被背景干扰。
- 不在 translucent surface 上再堆一层 translucent surface；侧栏、主内容与 inspector 各自清晰分层。

### 7.2 图片与图标

- 采用独立的「3DS -> HD」双存档单向迁移图标，深青为主、琥珀仅表示迁移动作，状态颜色使用系统语义色。
- 导出 macOS `.icns`、Windows `.ico` 与 Android adaptive icon；图标通过可复现的本地绘制/导出管线产生。
- 每阶段提供有语义的图形：源/目标路线、组件组、dry-run 文件流、备份/manifest、CEC 邮箱。
- 若使用外部 MH3G 场景截图或官方宣传图，必须记录来源、访问日期、分发许可与替换策略；无法证明可随开源发布时，使用自有生成的低对比度场景插图替代。

### 7.3 动态与反馈

- 按下操作立即给出原生 pressed/focus 反馈；导航、组件展开、状态出现从触发点自然过渡。
- 可交互的位移使用平台原生、可中断的短 spring/缓动，不在动画期间锁住输入。
- UI 状态只由真实子进程事件驱动；不得制作与 Rust 事务无关的虚假进度。
- sheet、popover、详情面板沿触发方向进入与返回；重要写入操作使用清晰确认而不是装饰性动效。
- `reduce motion` 时以短 opacity cross-fade 代替位移/弹簧；同时尊重减少透明度、高对比度、深浅色、动态字体与键盘/屏幕阅读器导航。

### 7.4 本地化

- 默认以系统 locale 选择中文或 English。
- Settings 可手动覆盖语言，并即时刷新可安全刷新的文案。
- 所有危险操作、错误阶段、文件路径和命令契约必须有双语资源；原始路径、hash、stderr 不被翻译或截断。

## 8. 验证、发布与 Android 后置

### 8.1 验证顺序

1. Rust：Windows probe mock、未知/失败 probe fail-closed、ExtData 全组成功、所有中间失败点的整组恢复、manifest 不一致拒绝与 JSON 契约测试。
2. macOS：fake command executor 的 UI 状态机测试；真实 `.app` 构建、签名检查、内置 CLI `--help/--version`、合成 fixture inspect/dry-run/write/rollback 运行验证。先完成真实 macOS 窗口验收。
3. Windows：WinUI 单元/UI 状态机测试、x64 build/installer/hash、内置 EXE smoke。GameHub/CrossOver 仅作辅助启动探索；原生 Windows 11 实机是最终 GUI 验收。
4. Android：设备连通后再实施 SAF、生成输出目录或 ZIP 的 Android 路径，进行 APK 和实机 UI 验收。

UI 合成 fixture 测试不会启动 Cemu 或读取/写入真实 MLC。任何真实存档 UI 验收仅能对用户明确选择的隔离目标执行，并保留哈希、backup 与 manifest。

### 8.2 发布

- macOS：独立 `.app` / ZIP，包含内置 arm64 CLI、图标、双语使用说明、校验文件。
- Windows：独立 x64 安装包或可完整解压的包，包含 WinUI App、内置静态 MSVC CLI、校验文件与双语说明。
- Android：后续独立 APK/AAB，不宣称可以直接写入桌面 Cemu MLC。
- UI 版本与随包 CLI 版本必须在 About/Diagnostics 中可见，并被构建脚本记录。

## 9. 测试产物清理

清理只在新的 UI/核心验证结束后执行，先生成可审计 inventory：路径、大小、SHA-256、生成来源、是否含 3DS 源数据及删除理由。

永远保留：

- 3DS 原始 `user#`、`system`、ExtData、CEC 及其用户备份；
- 发布的 release 包、校验文件、受控 rollback backup/manifest；
- 无法确定归属或是否含原始源数据的目录。

只删除能证明为旧隔离 Cemu MLC、旧转换目标、临时 staging、dry-run 输出、临时截图缓存的产物。删除前后生成清单；不以“路径名像测试目录”为唯一依据。

## 10. MCP / AI 接口

0.1 不新增 MCP。当前 CLI 已有 JSON、dry-run、write、rollback 及受控 manifest，足够供自动化与 AI 调用。补充 AI/CLI 使用指南，明确：

- 只接受的文件/目录形状；
- 写入前必须停止模拟器并跑 dry-run；
- source 永不改写；
- manifest-bound rollback 的使用方法；
- CEC 的实验性状态。

只有在 Windows 门禁、ExtData 批量事务、UI/CLI 报告契约都稳定后，才评估本地 stdio MCP；它必须复用相同后端门禁，不能新增绕开 UI/CLI 的写入路径。

## 11. 实施顺序与完成标准

1. 将本设计所需的 native UI 决策写入新的 ADR，并修订 `docs/research/MH3G_CONVERTER_UI_OPTIONS.md` 中过时的 Tauri 推荐。
2. 实现并验证 Rust Windows process probe 与 ExtData batch transaction。
3. 建立独立 macOS SwiftUI App、图标资源与打包/运行脚本；完成 macOS 实际窗口与合成文件流程验证。
4. 建立独立 Windows WinUI 3 App、打包与 CI；完成辅助 Wine 运行检查和原生 Windows 11 GUI 验收。
5. 建立 Android Compose 工程并在连接设备后验收。
6. 生成清理 inventory，保留原始源数据与 release 资产，删除明确的旧测试产物。

完成时必须同时满足：

- UI 没有在无 dry-run、输入变化、目标变化或模拟器运行时开放 Write；
- ExtData 以完整组安装与完整组回滚；
- Windows 写入/回滚后端强制运行门禁；
- macOS 真实窗口、Windows 原生 GUI、Android 实机（其阶段实施后）分别有证据；
- 所有发布包内 CLI 版本、UI 版本、哈希和用户文档相互一致；
- 清理没有删除任何原始 3DS 输入、已发布资产或不确定归属数据。
