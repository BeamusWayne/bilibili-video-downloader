# macOS 使用说明

[返回首页](../README.md)

## 下载哪个版本

测试版目标系统为 **macOS 13 Ventura 或更新版本**。在苹果菜单的「关于本机」查看芯片：

| 本机显示         | 下载文件                                               |
| ---------------- | ------------------------------------------------------ |
| Apple M 系列芯片 | `BilibiliVideoDownloader_0.2.0-beta.1_macos-arm64.dmg` |
| Intel 处理器     | `BilibiliVideoDownloader_0.2.0-beta.1_macos-x64.dmg`   |

从 [macOS 测试版 Releases](https://github.com/BeamusWayne/bilibili-video-downloader/releases/tag/v0.2.0-beta.1) 下载。两个版本均内置 FFmpeg、FFprobe 和 yt-dlp，不需要另外安装 Homebrew、Python、Node.js 或 Rust。

## 安装与首次打开

1. 打开 DMG，将 **BilibiliVideoDownloader** 拖到 **Applications / 应用程序**。
2. 弹出磁盘映像，从「应用程序」打开下载器。
3. 当前版本使用临时签名（ad-hoc），**没有 Apple Developer ID 签名和 Apple 公证**。首次打开可能被系统拦截。
4. 确认来自本仓库且校验值一致后，可在尝试打开之后，进入「系统设置 → 隐私与安全性」，为该 App 选择「仍要打开」，再确认「打开」。具体入口见 [Apple 官方说明](https://support.apple.com/102445)。公司管理的 Mac 可能限制这一操作。

此操作只针对这个 App；无需关闭系统安全检查。若系统明确报告恶意软件或校验值不一致，停止安装并重新核对来源。

校验下载文件：在终端运行 `shasum -a 256 文件名.dmg`，与发布页同名 `.sha256` 文件比较。

## 使用

账号页扫码或导入 Cookie → 新建下载粘贴链接 → 解析弹窗勾选内容 → 开始下载。默认保存到系统「下载」文件夹中的 `Bilibili`，也可选择其他可写目录。

- 登录态保存在 **macOS 钥匙串**，账号头像和资料缓存在本机。
- 使用左上角原生窗口按钮；菜单提供复制、粘贴和退出。点击 Dock 图标可重新显示窗口。
- 开启「关闭后在托盘运行」时，关闭窗口后会继续在菜单栏运行；从菜单栏图标或 `⌘Q` 完全退出。
- 暂停、断网和应用异常结束后的恢复机制与 Windows 共用同一套引擎。Windows 和 Mac 各自保存本机任务与账号，当前不跨设备同步。

首次读取受保护目录时，macOS 可能询问访问权限，按实际保存位置允许即可。默认并发为 3 个下载任务、每文件 4 连接、共 12 连接和 1 个转码任务。

## 数据与卸载

应用资料、SQLite 队列与媒体缓存位于 `~/Library/Application Support/com.local.bilibili-video-downloader/`。下载文件保存在选择的目录。请通过应用里的「退出账号」删除钥匙串中的登录态。

卸载时退出应用，再将「应用程序」中的 App 移到废纸篓。卸载 App 不会自动删除队列、缓存或已经下载的视频；需要时自行清理相应目录。

## 验证范围

发布前通过 Apple Silicon 和 Intel 的 GitHub macOS 构建机检查：构建与测试引擎、WebKit 模拟界面、钥匙串持久化、DMG 完整性与安装复制、内置工具架构和依赖、六种格式转换以及复制后的原生应用启动。

这些检查不等同于在你的 Mac 上完成扫码和真实视频下载。目标最低系统版本是 macOS 13；构建与自动化验证使用 macOS 15。测试版仍需要在实际设备验证扫码、系统目录授权、休眠唤醒和真实大批量下载，反馈时可附上机型、系统版本和应用导出的诊断日志。
