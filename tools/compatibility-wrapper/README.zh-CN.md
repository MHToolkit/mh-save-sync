# Windows 兼容补丁入口源码

此目录保存用于生成更新版 Windows 压缩包中 `mh3g-save-convert.exe` 的兼容包装层源码。

## 用途

原发布包中的 WinUI 程序会调用 `tools/mh3g-save-convert.exe`。由于新版代码仓库当时没有可直接下载的 Windows 构建产物，本包装层会：

1. 在临时副本中补齐随从面具熟练度、名片竞技场记录和 CEC 竞技场记录的遗漏转换；
2. 调用原版转换器核心 `mh3g-save-convert-core.exe` 完成其余转换；
3. 保持原界面与调用参数兼容。

## 文件

- `gen_wrapper.py`：从仓库中的 `meow_transform_table.rs` 读取转换表并生成 `wrapper.c`。
- `wrapper.c`：Windows x64 无 CRT 包装程序源码。
- `validate_patch.py`：验证补丁覆盖范围、避免重复转换，并检查生成的 PE 文件。
- `kernel32.def`、`shell32.def`：链接所需的 Windows 导入定义。
- `build-linux.sh`：在带有 LLVM/Clang 的 Linux 环境中交叉编译。
- `build-windows.ps1`：在带有 LLVM/Clang 的 Windows 环境中编译。

## Linux 构建

```bash
cd tools/compatibility-wrapper
./build-linux.sh
```

输出文件：`dist/mh3g-save-convert.exe`。

## Windows 构建

在已安装 Python 3、Clang 和 LLD 的 PowerShell 中运行：

```powershell
cd tools/compatibility-wrapper
./build-windows.ps1
```

## 与成品包组合

将原发布包的转换器改名为：

```text
mh3g-save-convert-core.exe
```

再把本目录构建出的文件命名为：

```text
mh3g-save-convert.exe
```

两者必须放在同一目录。包装层不包含也不分发原转换器核心。
