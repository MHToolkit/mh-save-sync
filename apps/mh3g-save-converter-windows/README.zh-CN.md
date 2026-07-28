# Windows 版 MH3G 存档转换器

[English](README.md)

这是 `mh3g-save-convert` Rust CLI 的原生 Windows 配套界面。工程使用
**WinUI 3 / .NET 8**，面向 Windows 10 1809+ 与 Windows 11 x64，且不包含
任何存档解析、道具映射或转换算法。

## 安全边界

- 首次启动默认跟随系统语言；可切换并持久化“跟随系统 / 简体中文 /
  English”，配置位于
  `%LOCALAPPDATA%\MHToolkit\MH3GSaveConverter\settings.json`。
- 不扫描 SD 卡、MLC、ZIP、7z、RAR 或任意存档文件夹。所有输入都必须由用户
  选择准确文件或目录。
- 核心槽位流程固定为 `inspect` -> `convert --dry-run` -> 最终 SHA-256
  复核 -> `convert --write`。写入前有确认对话框，成功后会记录 CLI 输出的
  manifest，回滚只能使用该 manifest。
- C# 仅用 `ProcessStartInfo.ArgumentList` 逐个传递 argv，且
  `UseShellExecute = false`；它只解析 CLI 的 JSON stdout，不拼接 shell 命令，
  也不重复实现转换逻辑。
- 实验性 CEC 默认关闭，并使用完全独立的检查、Dry Run、最终只读复核、写入与
  回滚链路。写入会绑定紧接着的 Dry Run 返回的聚合
  source_record_set_sha256 与 target_sha256_before，邮箱或缓存变化时会
  失败关闭。选择核心槽位不会自动打开 CEC。
- `system` 与 ExtData（`card*`、`cardbox`、`quest*`）仍属于独立 CLI
  事务。当前首版 Windows 外壳不会猜测 Cemu MLC 目录，也不会静默安装
  ExtData 组件组。

执行写入或回滚前，必须退出 Nemessix、Azahar 和 Cemu。准确源文件、目标范围
及事务边界参见根目录的
[中文 CLI 文件契约](../../docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.zh-CN.md) 与
[English CLI contract](../../docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.md)。

## 发布包布局

Windows 发布任务会把原生 WinUI 应用与 x64 Rust sidecar 一起打包。解压
后的 ZIP 固定包含：

```text
MH3GSaveConverter.exe
tools/mh3g-save-convert.exe
tools/mh3g-save-convert.exe.sha256
Run-Converter.ps1
```

任务会在不启动 GUI 的前提下检查解压后的 GUI 可执行文件与 `tools` sidecar。
`Run-Converter.ps1` 保留给显式 CLI 调用；它会先校验打包 sidecar 的校验和，再透传
参数。开发场景可以显式选择另一个 CLI 路径，或设置 `MH3G_CONVERTER_CLI`。若
sidecar 不存在或没有输出 JSON，应用会将该操作标记为失败。

## 在 Windows x64 上一键打包

不要让 IDE、Qoder 或人工命令分别构建 Rust 与 WinUI：从仓库根目录只运行这一条
**唯一权威命令**：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-mh3g-save-converter-windows.ps1
```

脚本会预检 Windows 10 1809+/Windows 11 x64、.NET 8 SDK、Rust 1.95+ 的
`cargo`/`rustup`，以及
带 `Microsoft.VisualStudio.Component.VC.Tools.x86.x64` 和 Windows SDK 的 Visual
Studio 2022 Build Tools；随后它通过 `VsDevCmd.bat` 导入 x64 MSVC 环境，不依赖启动
Qoder 的 shell。Rust 测试和 sidecar 构建固定使用
`x86_64-pc-windows-msvc`，不会错误地沿用测试员的 GNU 默认 target。

首次机器缺少依赖时，才显式允许脚本使用 `winget` 安装；这一步可能需要管理员批准：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-mh3g-save-converter-windows.ps1 -Bootstrap
```

`-Bootstrap` 只会安装缺失的 .NET 8 SDK、Rustup 和 Visual Studio 2022 C++ Build
Tools（含推荐的 Windows SDK 组件）；若电脑已有不完整的 Visual Studio/Build Tools，
它会调用 `setup.exe modify --installPath` 补齐 VC Tools/SDK，而不是把已安装实例当作
无变化的 `winget install`。

如果 WinGet 显示 `Rustlang.Rustup` 已安装、但当前用户没有可用的
`%USERPROFILE%\.cargo\bin\rustup.exe`，同一条命令会按顺序原地修复 Rustup 载荷：
普通 WinGet 安装 → `winget repair` → 强制重新运行 Rustup 安装器 → 从官方 HTTPS 下载
`rustup-init.exe` 并校验其 SHA-256 sidecar 完整性后兜底。它**不会**卸载 Rustup、删除 `.cargo` / `.rustup`、
修改持久 PATH，也不会改写用户持久默认工具链；打包进程只在自身进程内选择
`stable-x86_64-pc-windows-msvc`。安装器返回 3010/1641 时脚本会要求重启后原命令重跑。
默认命令绝不会静默安装或更改系统。两种路径均不会清空 NuGet、Cargo 或 `target` 缓存，
因此重复执行会复用已有下载。

成功后会产生：

```text
artifacts\mh3g-save-convert-windows-x64.zip
artifacts\mh3g-save-convert-windows-x64.zip.sha256
artifacts\mh3g-save-convert.exe.sha256
artifacts\mh3g-save-convert-windows-build-transcript.txt
```

脚本会依次执行 `dotnet restore`、固定 MSVC target 的 Rust 测试/发布、self-contained
WinUI `dotnet publish`、sidecar SHA-256、ZIP SHA-256 及解压后的布局/sidecar
自检。它不会启动 GUI、Cemu 或读取真实存档；没有模拟器运行时，还会只在临时目录做一
次合成 `write -> rollback` smoke。若模拟器已在运行，则不停止它，只跳过该合成写入
smoke。常规发包不要使用 `-SkipTests` 或 `-SkipTransactionSmoke`。

若仍失败，请直接提供
`artifacts\mh3g-save-convert-windows-build-transcript.txt` 中的**第一个**
`error`/`MSB`/`link.exe`/`cargo` 错误行，而不是让工具改用另一组手工构建命令。

## 非 Windows 主机上的源级检查

`scripts/verify-mh3g-save-converter-windows-source.py` 会检查项目元数据、XML
格式、argv-only 桥接、JSON 解析、核心工作流命令、CEC 隔离与双语文案。它不能
替代 Windows x64 的真实构建和运行验证。
