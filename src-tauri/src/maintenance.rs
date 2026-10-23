use anyhow::{ensure, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

pub struct Updater {
    pub gate: tokio::sync::Mutex<()>,
    pub cancel: std::sync::Mutex<CancellationToken>,
}
impl Default for Updater {
    fn default() -> Self {
        Self {
            gate: tokio::sync::Mutex::new(()),
            cancel: std::sync::Mutex::new(CancellationToken::new()),
        }
    }
}
pub fn parser_path(bundled: &Path, data: &Path) -> PathBuf {
    let manifest = data.join("parser/current.json");
    let selected = std::fs::read(manifest)
        .ok()
        .and_then(|b| serde_json::from_slice::<String>(&b).ok());
    if let Some(name) =
        selected.filter(|n| n.len() == 64 && n.chars().all(|c| c.is_ascii_hexdigit()))
    {
        let path = data
            .join("parser")
            .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        if path.is_file() {
            return path;
        }
    }
    crate::platform::tool(bundled, "yt-dlp")
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Maintenance {
    pub cache_bytes: u64,
    pub cache_free: u64,
    pub output_free: u64,
    pub parser_version: String,
    pub parser_updated: bool,
    pub updating: bool,
}
pub fn directory_size(path: &Path) -> u64 {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| match e.file_type() {
                    Ok(t) if t.is_dir() => directory_size(&e.path()),
                    Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
                    _ => 0,
                })
                .sum()
        })
        .unwrap_or(0)
}
pub async fn version(path: &Path) -> String {
    crate::process::capture(
        path,
        &["--version".into()],
        &CancellationToken::new(),
        Duration::from_secs(12),
    )
    .await
    .map(|s| String::from_utf8_lossy(&s).trim().to_string())
    .unwrap_or_else(|_| "不可用".into())
}
pub async fn update(data: &Path, bundled: &Path, updater: &Updater) -> Result<String> {
    let _guard = updater
        .gate
        .try_lock()
        .map_err(|_| anyhow::anyhow!("解析器正在更新"))?;
    let token = CancellationToken::new();
    *updater.cancel.lock().unwrap() = token.clone();
    let operation = async {
        let client = reqwest::Client::builder()
            .user_agent("BilibiliVideoDownloader")
            .timeout(Duration::from_secs(180))
            .build()?;
        let release: serde_json::Value = client
            .get("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let tag = release["tag_name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("版本信息缺失"))?;
        ensure!(
            tag.len() <= 32 && tag.chars().all(|c| c.is_ascii_digit() || c == '.'),
            "版本标识无效"
        );
        let asset = if cfg!(target_os = "windows") {
            "yt-dlp.exe"
        } else if cfg!(target_os = "macos") {
            "yt-dlp_macos"
        } else {
            anyhow::bail!("此平台暂不支持在线更新")
        };
        let base = format!("https://github.com/yt-dlp/yt-dlp/releases/download/{tag}");
        let sums = client
            .get(format!("{base}/SHA2-256SUMS"))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let expected = sums
            .lines()
            .find_map(|line| {
                let mut p = line.split_whitespace();
                let hash = p.next()?;
                let name = p.next()?.trim_start_matches('*');
                (name == asset).then(|| hash.to_owned())
            })
            .ok_or_else(|| anyhow::anyhow!("官方校验表没有对应文件"))?;
        ensure!(
            expected.len() == 64 && expected.chars().all(|c| c.is_ascii_hexdigit()),
            "官方校验值无效"
        );
        let dir = data.join("parser");
        tokio::fs::create_dir_all(&dir).await?;
        crate::library::check_space(&dir, 180 * 1024 * 1024, 128 * 1024 * 1024)?;
        let mut response = client
            .get(format!("{base}/{asset}"))
            .send()
            .await?
            .error_for_status()?;
        ensure!(
            response.content_length().unwrap_or(0) <= 180 * 1024 * 1024,
            "解析器文件过大"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                bytes.len() + chunk.len() <= 180 * 1024 * 1024,
                "解析器文件过大"
            );
            bytes.extend_from_slice(&chunk);
        }
        ensure!(
            hex::encode(Sha256::digest(&bytes)) == expected,
            "解析器校验失败，已保留原版本"
        );
        let target = dir.join(format!("{expected}{}", std::env::consts::EXE_SUFFIX));
        if !target.exists() {
            crate::process::atomic_bytes(&target, &bytes).await?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                tokio::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).await?;
            }
        }
        let actual = version(&target).await;
        ensure!(actual == tag, "解析器启动检查失败，已保留原版本");
        let _ = bundled; // The signed application bundle is never modified.
        crate::process::atomic_json(&dir.join("current.json"), &expected).await?;
        Ok(actual)
    };
    tokio::select! { _ = token.cancelled() => anyhow::bail!("更新已取消，原版本保持可用"), result = tokio::time::timeout(Duration::from_secs(240), operation) => result.map_err(|_|anyhow::anyhow!("更新超时，原版本保持可用"))? }
}
