# MH Save Sync 跨平台图标系统设计

日期：2026-07-18  
状态：已获用户批准  
设计方向：B3「加密紫」

## 1. 目标

为 MH Save Sync 建立一套可长期使用的跨平台图标系统，覆盖：

- macOS Dock、Finder 和应用包图标；
- macOS 菜栏 Template 图标；
- Android 自适应启动图标、旧版 launcher 图标和 Android 13 monochrome 图标；
- Android 通知栏纯白小图标。

图标必须在 16–1024 px 范围内保持可辨识，表达“安全保存、完整性验证和可恢复”，但不依赖特定游戏、模拟器或设备形态。

## 2. 核心视觉

### 2.1 符号

主图形由三部分组成：

1. **盾牌**：表达原始存档保护、恢复前备份和端到端加密；
2. **存档槽**：表达游戏存档，而不是通用密码管理或杀毒软件；
3. **验证勾号**：表达稳定指纹、完整性验证和已确认快照。

不加入文字、“MH”字母、云朵、设备轮廓、怪物、武器或其他游戏版权素材。

### 2.2 配色

- 深紫：`#4936B7`
- 亮紫：`#9B72F2`
- 主背景：从深紫左上向亮紫右下的轻微线性渐变
- 主图形：白色 `#FFFFFF`
- 图形内部反色：深紫 `#4936B7`

渐变只用于彩色启动图标。单色、菜单栏和通知图标不得依赖渐变。

### 2.3 形态与层级

- 母版为 1024×1024、无文字、无外部投影的矢量源；
- 盾牌占画布约 68%，保持粗线条和大留白；
- 存档槽与勾号必须在 24 px 预览中仍能区分；
- 不使用玻璃高光、拟物纹理或复杂 3D；
- 平台负责外部遮罩，母版不得把圆角方形永久烘焙进 Android 前景层。

## 3. 平台交付

### 3.1 macOS 应用图标

源文件放在 `apps/macos/Resources/AppIcon/`：

- `mh-save-sync-icon.svg`：可编辑矢量母版；
- 生成 16、32、64、128、256、512、1024 px PNG；
- 使用 `iconutil` 生成 `MHSaveSync.icns`。

`scripts/build-macos-app-bundle.sh` 必须：

- 将 `.icns` 复制到应用包 `Contents/Resources/`；
- 在 `Info.plist` 写入 `CFBundleIconFile=MHSaveSync`；
- 验证 `.icns` 存在、`Info.plist` 可解析且产物引用正确。

### 3.2 macOS 菜单栏图标

提供独立的单色 Template 图标，不缩小彩色应用图标：

- `mh-save-sync-menubar-template.pdf` 或等价 18×18 / 36×36 template 资源；
- 只保留盾牌、存档槽与勾号的必要轮廓；
- 使用 `isTemplate = true`，自动适配浅色、深色和高对比度；
- 状态文字仍保留为辅助信息，但菜单栏主要入口应显示图标。

### 3.3 Android 启动图标

资源放在标准 Android `res` 目录：

- `mipmap-anydpi-v26/ic_launcher.xml`：adaptive icon；
- `drawable/ic_launcher_foreground.xml`：白色盾牌/存档/勾号前景；
- `values/colors.xml`：紫色背景；
- `drawable/ic_launcher_monochrome.xml`：Android 13 themed icon；
- `mipmap-mdpi` 到 `mipmap-xxxhdpi`：legacy PNG fallback；
- 圆形 fallback 使用同一母版与安全区。

`AndroidManifest.xml` 写入：

- `android:icon="@mipmap/ic_launcher"`
- `android:roundIcon="@mipmap/ic_launcher_round"`

前景图形必须位于 adaptive icon 安全区内，在圆形、圆角方形、泪滴等系统遮罩下不被裁掉。

### 3.4 Android 通知图标

通知图标使用独立的 `drawable/ic_stat_save_sync.xml`：

- 透明背景；
- 图形为纯白 alpha mask；
- 不包含渐变、紫色底板或细小文字；
- ActiveSessionService 和 WorkManager 通知统一引用该资源。

## 4. 构建与验证

### 4.1 自动验证

- macOS bundle 构建检查 `CFBundleIconFile` 与 `.icns`；
- Android `lintDebug testDebugUnitTest assembleDebug`；
- `aapt dump badging` 必须显示 launcher icon；
- APK 解包检查 adaptive、monochrome、legacy 和通知资源均存在；
- 禁止在资源或构建日志中引入密钥、真实存档或用户路径内容。

### 4.2 视觉验证

- macOS：Dock、Finder、菜单栏浅色/深色模式；
- Android：桌面圆形与圆角方形遮罩、应用信息页、通知栏浅色/深色模式；
- 检查 16、20、24、32、64、128 和 1024 px 缩放；
- 验证图标没有文字糊化、边缘裁切、紫色底板泄漏到通知栏或低对比度问题。

## 5. 非目标

- 本次不改产品名称、包名、Bundle ID 或 UI 主色；
- 不制作启动动画、品牌宣传图或商店截图；
- 不为具体怪物猎人作品制作独立图标；
- 不把图标设计与存档协议、加密格式或同步逻辑耦合。

## 6. 验收标准

1. macOS 应用包和 Android APK 均显示 B3 图标；
2. 菜单栏与 Android 通知栏使用清晰的单色专用图标；
3. Android 各类 adaptive mask 不裁切核心符号；
4. 双端构建、单测、lint、资源检查通过；
5. 产物截图或 UI dump、文件哈希和构建命令被记录为可复现证据。
