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

## 检查

```powershell
npm run build
npm test
cargo test --manifest-path src-tauri/Cargo.toml --jobs 2 --tests
```

界面测试自行启停服务，在 Edge 无头运行，使用模拟 IPC，不向 B站施压。截图在 `test-results`。Rust 使用本地 HTTP 故障服务器与真实 FFmpeg / FFprobe，不需要账号登录。

## 打包

```powershell
$env:CARGO_BUILD_JOBS = "2"
npm run package
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-portable.ps1
```

NSIS 在 `src-tauri/target/release/bundle/nsis`；整理后的安装包、便携 ZIP 和 SHA-256 在 `release`。便携包使用独立暂存目录。

打包显式启用 `custom-protocol` 嵌入前端。`tauri dev` 依赖 Vite，不能作为独立应用发布。Windows 主程序与工具进程不创建控制台。

## 真实视频检查

应用扫码后运行：

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --example live-check -- "https://www.bilibili.com/video/BV1SUnB61E8K/" probe
```

模式支持 `catalog`、`probe`、`account`、`resume`、`mp3`，输出在 `.cache/live-check/output`。不要把 Cookie 放进命令行、源码、Issue 或截图。

## 仓库内容

提交源码、文档、锁文件、图标和许可；排除依赖、构建、缓存、登录态、下载和工具 EXE。README 演示截图单独放入 `docs/images`，注明演示数据。测试记录见 [TESTING.md](../TESTING.md)。
