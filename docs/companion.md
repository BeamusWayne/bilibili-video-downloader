# 掉宝助手与下载器

两款应用独立运行、独立保存凭据。助手只发送公开的视频链接，下载器始终显示解析确认窗口。

## 使用

1. 安装新版下载器并启动一次，让系统注册 `bilidownloader` 协议。Windows 支持安装包与保留工具目录的便携包；macOS 将 App 拖入「应用程序」。
2. 在新版掉宝助手侧栏点击「下载回放 / 视频」。
3. 粘贴已发布回放或相关视频的 BV/AV 号或完整视频链接，每行一个，单次最多 100 条。
4. 点击「在下载器中确认」，在解析弹窗勾选内容并开始下载。

未安装时可点击「获取下载器」或「复制视频链接」。不支持用直播间号推断回放，也不自动共享 Cookie、直播观看状态或奖励任务标识。

## 协议

`bilidownloader://download?url=<URL encoded public video URL>`

多个视频使用重复的 `url` 参数。接收方只允许 Bilibili `/video/BV…` 或 `/video/av…`，保留有效的 `p` 分 P 参数，去掉追踪参数。拒绝直播间、外站、文件协议、用户密码和非 `url` 联动参数。单次 URI 最大 32 KiB。传入链接不会直接入队。

Windows/Linux 使用 Tauri deep-link 与 single-instance 插件；macOS 由 Launch Services 分发。实现参考 [Tauri Deep Linking](https://v2.tauri.app/plugin/deep-linking/)。

## 解析器和空间

更新使用 yt-dlp 官方 GitHub Release 及同版本 `SHA2-256SUMS`，通过 HTTPS 获取并校验后，在用户数据目录安装，用版本启动检查通过后原子切换选择文件。此校验不是 Apple 公证；内置版本始终保留，可在设置恢复。临时下载失败或取消不会切换到未完成的版本。

默认磁盘预留 512 MiB，可在设置调整。缓存和输出可能位于不同磁盘，分别检查；分片合并及转码也需要空间。空间不足暂停后，请先释放相应磁盘空间，再继续任务。清理已完成缓存后，重新转码可能需要重新下载媒体源；最终输出不被清理。
