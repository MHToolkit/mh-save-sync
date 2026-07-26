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
  回滚链路。选择核心槽位不会自动打开 CEC。
- `system` 与 ExtData（`card*`、`cardbox`、`quest*`）仍属于独立 CLI
  事务。当前首版 Windows 外壳不会猜测 Cemu MLC 目录，也不会静默安装
  ExtData 组件组。

执行写入或回滚前，必须退出 Nemessix、Azahar 和 Cemu。准确源文件、目标范围
及事务边界参见根目录的
[中文 CLI 文件契约](../../docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.zh-CN.md) 与
[English CLI contract](../../docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.md)。

## Sidecar 布局

发布包必须把已签名的 x64 Rust CLI 放在应用旁的固定位置：

```text
MH3GSaveConverter.exe
tools/mh3g-save-convert.exe
```

开发场景可以显式选择另一个 CLI 路径，或设置 `MH3G_CONVERTER_CLI`。若 sidecar
不存在或没有输出 JSON，应用会将该操作标记为失败。

## 在 Windows 上构建

安装带有 **.NET desktop development** 工作负载、Windows 10/11 SDK 与 .NET 8
SDK 的 Visual Studio 2022。然后在仓库根目录执行：

```powershell
dotnet restore apps\mh3g-save-converter-windows\MH3GSaveConverter.Windows.csproj
dotnet build apps\mh3g-save-converter-windows\MH3GSaveConverter.Windows.csproj -c Release -p:Platform=x64
dotnet publish apps\mh3g-save-converter-windows\MH3GSaveConverter.Windows.csproj -c Release -r win-x64 --self-contained true -p:Platform=x64 -p:WindowsAppSDKSelfContained=true
```

在运行前，把 release 构建得到的 `mh3g-save-convert.exe` 放入输出目录的
`tools` 子目录。Windows UI 的打包/签名与 Rust CLI 的打包/签名应保持分离，
从而保留二进制来源链路。

## 非 Windows 主机上的源级检查

`scripts/verify-mh3g-save-converter-windows-source.py` 会检查项目元数据、XML
格式、argv-only 桥接、JSON 解析、核心工作流命令、CEC 隔离与双语文案。它不能
替代 Windows x64 的真实构建和运行验证。
