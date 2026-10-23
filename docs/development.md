# 开发与构建

[返回首页](../README.md)

## 环境

Windows 10 / 11 x64，Node.js 22+，Rust MSVC stable，Visual Studio 2022 桌面 C++ Build Tools、Windows SDK 和 WebView2。界面测试需要 Microsoft Edge。

```powershell
npm ci
npm run prepare:tools
npm run desktop
```

当前终端找不到 Cargo 时，可重新打开终端，或执行 `$env:Path = "$env:USERPROFILE/.cargo/bin;" + $env:Path`。

工具从 yt-dlp 官方发布与 Gyan FFmpeg 构建站获取，核对发布方 SHA-256。版本、许可与校验结果在 `src-tauri/licenses`，二进制在 `src-tauri/binaries`，不提交 Git。已有工具跳过下载；更新后应重新核对许可和媒体测试。

### macOS

需要 macOS 13+、Node.js 22+、Rust stable 和 Xcode Command Line Tools。运行同样的 `npm ci`、`npm run prepare:tools`、`npm run desktop`。准备脚本会按当前架构下载固定版本工具，校验归档 SHA-256，保留许可并生成 ICNS 图标。工具和许可分别位于 `src-tauri/binaries/macos` 与 `src-tauri/licenses/macos`，与 Windows 文件分开且不提交 Git。

macOS 使用官方 yt-dlp 通用程序，以及 Martin Riedl 提供的原生静态 FFmpeg / FFprobe；版本与归档校验值见 `scripts/macos-tools.json`。更新工具时需要同时核对许可、原生架构、动态库依赖和六种格式测试。

## 检查

```powershell
npm run build
npm test
cargo test --manifest-path src-tauri/Cargo.toml --jobs 2 --tests
```

界面测试自行启停服务，在 Edge 无头运行，使用模拟 IPC，不向 B站施压。截图在 `test-results`。Rust 使用本地 HTTP 故障服务器与真实 FFmpeg / FFprobe，不需要账号登录。

macOS 界面检查使用 `npx playwright install webkit`，随后运行 `BVD_UI_BROWSER=webkit npm test`。同一套引擎和媒体测试在两个 Mac 架构运行，额外校验钥匙串跨 Entry 实例持久化。Windows 上可通过 `$env:BVD_UI_PLATFORM = "macos"; npm test` 检查 Mac 布局分支，但这不等于原生 WebKit 验证。

## 打包

```powershell
$env:CARGO_BUILD_JOBS = "2"
npm run package
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-portable.ps1
```

NSIS 在 `src-tauri/target/release/bundle/nsis`；整理后的安装包、便携 ZIP 和 SHA-256 在 `release`。便携包使用独立暂存目录。

打包显式启用 `custom-protocol` 嵌入前端。`tauri dev` 依赖 Vite，不能作为独立应用发布。Windows 主程序与工具进程不创建控制台。

### macOS DMG

在对应架构的 Mac 上执行：

```sh
npm run package -- --bundles app,dmg
# 检查构建出的 DMG，第二个参数根据本机改为 arm64 或 x86_64
node scripts/check-macos-bundle.mjs src-tauri/target/release/bundle/dmg/文件名.dmg arm64
```

平台配置使用 Tauri 自动合并的 `tauri.macos.conf.json` / `tauri.windows.conf.json`。macOS 版本使用原生左侧窗口按钮和白色覆盖式顶栏；认证启用 `apple-native`，调用工具时始终使用包内绝对路径。

[macOS DMG 工作流](https://github.com/BeamusWayne/bilibili-video-downloader/actions/workflows/macos.yml) 使用 `macos-15` 与 `macos-15-intel` 原生构建。打包后挂载磁盘映像，将 App 复制到包含空格的目录，再弹出磁盘映像，检查签名结构、架构、系统动态库引用、包内工具校验值、六种输出格式和原生进程启动。通过后在 Artifacts 提供 DMG、`.sha256` 和验证报告。

当前使用 ad-hoc 签名（`signingIdentity: "-"`），不配置 Apple 证书或账号，也不宣称已经公证。以后接入 Developer ID / 公证时需单独配置凭据，不能把证书私钥或 Apple 账号信息写入源码。

## 真实视频检查

应用扫码后运行：

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --example live-check -- "https://www.bilibili.com/video/BV1SUnB61E8K/" probe
```

模式支持 `catalog`、`probe`、`account`、`resume`、`mp3`，输出在 `.cache/live-check/output`。不要把 Cookie 放进命令行、源码、Issue 或截图。

## 仓库内容

提交源码、文档、锁文件、图标和许可；排除依赖、构建、缓存、登录态、下载和工具 EXE。README 演示截图单独放入 `docs/images`，注明演示数据。测试记录见 [TESTING.md](../TESTING.md)。
