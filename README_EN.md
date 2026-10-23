<div align="center">

<img src="docs/images/logo.png" alt="BilibiliVideoDownloader" width="72" height="72">

# BilibiliVideoDownloader

### Bilibili batch video and audio downloader

**Keep the videos you love, reliably.**

![Windows](https://img.shields.io/badge/Windows-Desktop-FB7299)
![macOS](https://img.shields.io/badge/macOS-DMG%20Beta-111111?logo=apple&logoColor=white)
![Tauri + Rust](https://img.shields.io/badge/Tauri_2-Rust-242A35)
[![MIT](https://img.shields.io/badge/License-MIT-2ea44f)](LICENSE)

Batch downloads · Verified resume · Separate conversion workers · QR login

**Windows v0.1.1** · [Download for Windows x64](https://github.com/BeamusWayne/bilibili-video-downloader/releases/tag/v0.1.1) · [Windows guide](docs/windows.md)

**New interface beta v0.2.0-beta.2** · [Windows and macOS downloads](https://github.com/BeamusWayne/bilibili-video-downloader/releases/tag/v0.2.0-beta.2)

**macOS v0.2.0-beta.1** · [Apple Silicon / Intel DMG beta](https://github.com/BeamusWayne/bilibili-video-downloader/releases/tag/v0.2.0-beta.1) · [macOS installation guide](docs/macos.md)

[Features](#features) · [Desktop interface](#desktop-interface) · [Quick Start](#quick-start) · [Related projects](#related-projects) · [中文](README.md)

</div>

---

## Features

- Paste multiple links or BV / AV IDs, or import a TXT list.
- Review parsed videos and parts in a dialog before starting downloads.
- Export MP4, MKV, MP3, M4A, FLAC and WAV with bundled FFmpeg / FFprobe.
- Control download workers, per-file connections, total connections and conversions separately.
- Resume verified 4 MiB chunks; reject inconsistent remote representations.
- Sign in with a QR code or import cookies. Windows Credential Manager or macOS Keychain stores credentials; avatars are cached locally.
- Save covers, subtitles and danmaku, and manually repair, blur or crop regions.

## Desktop interface

![Queue — demonstration data](docs/images/queue.png)

Single videos, multipart videos and short links are supported. Other Bilibili sources depend on yt-dlp, platform interfaces and account access; each source type has not been verified with a live account.

## Quick Start

Windows requires Windows 10 / 11 x64 and Microsoft Edge WebView2. Get the installer or portable ZIP from [Releases](https://github.com/BeamusWayne/bilibili-video-downloader/releases/tag/v0.1.1). Keep the portable application's `tools` and `licenses` folders alongside its executable.

The macOS beta targets macOS 13+ and comes as separate Apple Silicon and Intel DMGs. Drag the app to Applications; FFmpeg, FFprobe and yt-dlp are included. This beta is ad-hoc signed and has **no Developer ID signature or Apple notarization**. First launch may be blocked; see the [installation guide](docs/macos.md) and [Apple's instructions](https://support.apple.com/102445).

Sign in, open **New download**, paste links and parse. Select entries and output options in the dialog, then start downloading. The app switches to the queue.

Defaults: 3 download workers, 4 connections per file, 12 total connections and 1 conversion worker.

## Reliability

SQLite WAL persists tasks. SHA-256 checkpoints, ETag / Last-Modified and resource sizes help validate resumed data. Retries are bounded, rate limits trigger cooldowns, and final outputs are checked with FFprobe.

Unfinished chunks restart. Sources without reliable validators restart safely. Interrupted conversions restart conversion while retaining source files. Cache cleanup is manual.

Local fault tests, a simulated 500-task UI and real MP4 / MP3 exports have passed. This is not a benchmark against other downloaders. See [verification](TESTING.md) and [architecture](docs/architecture.md).

## White workspace and batch reliability

The new interface includes named presets, a persistent download history, part-aware duplicate detection, disk-space protection, cache cleanup and a verified yt-dlp update/reset entry. Updates live in user data and never modify the signed macOS app bundle. Ctrl+Enter (Command+Enter on macOS) opens the parsing preview; downloads require confirmation.

## Companion handoff

In the new [Bili Live Companion](https://github.com/BeamusWayne/bili-live-companion), choose **下载回放 / 视频** and supply published BV/AV video links. The downloader opens one instance and presents the parsing preview. Credentials remain separate; a live room is never treated as a published replay. If the downloader is missing, copy the links or use the release download entry. See [protocol details](docs/companion.md).

## Development

```powershell
git clone https://github.com/BeamusWayne/bilibili-video-downloader.git
cd bilibili-video-downloader
npm ci
npm run prepare:tools
npm run desktop
```

Windows builds require Node.js, Rust MSVC, Visual Studio C++ Build Tools and WebView2. macOS builds require Node.js, Rust stable and Xcode Command Line Tools. Build DMGs on a Mac or use the included GitHub Actions workflow. See the [development guide](docs/development.md), [macOS guide](docs/macos.md), [Windows guide](docs/windows.md) and [changelog](CHANGELOG.md).

## Related projects

Two independent Bilibili tools by the same author, for different workflows:

| Project                                                                             | Use it for                                                                   | Get started                                                                                                       |
| ----------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| [BilibiliVideoDownloader](https://github.com/BeamusWayne/bilibili-video-downloader) | Batch video and audio downloads, verified resume and multiple output formats | [Windows x64](https://github.com/BeamusWayne/bilibili-video-downloader/releases/latest)                           |
| [Bili Live Companion](https://github.com/BeamusWayne/bili-live-companion)           | Live-drop watching, multi-account task tracking and reward claims            | [Desktop and server guide](https://github.com/BeamusWayne/bili-live-companion/blob/main/README_EN.md#quick-start) |

## License

Source: [MIT](LICENSE). Bundled tools retain their own licenses; see [third-party notices](THIRD_PARTY_NOTICES.md). Unofficial and unaffiliated with Bilibili. Region filters cannot guarantee invisible restoration. AI watermark detection and live-stream recording are not included.

---

<div align="center">
  Made with ☕ by <a href="https://github.com/BeamusWayne">Beamus Wayne</a>
</div>
