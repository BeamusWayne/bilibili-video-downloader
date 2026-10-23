use crate::{model::*, store::Store};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub options: Options,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Presets {
    pub items: Vec<Preset>,
    pub default_id: String,
}
impl Presets {
    pub fn initial(s: &Settings) -> Self {
        let mut daily = Options::from_settings(s);
        daily.format = "mp4".into();
        daily.quality = if s.quality == 0 { 1080 } else { s.quality };
        let mut audio = daily.clone();
        audio.format = "mp3".into();
        audio.quality = 0;
        let mut original = daily.clone();
        original.format = "mkv".into();
        original.quality = 0;
        Self {
            items: vec![
                Preset {
                    id: "daily".into(),
                    name: "日常视频".into(),
                    options: daily,
                },
                Preset {
                    id: "audio".into(),
                    name: "只保存声音".into(),
                    options: audio,
                },
                Preset {
                    id: "original".into(),
                    name: "保留高画质".into(),
                    options: original,
                },
            ],
            default_id: "daily".into(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.items.is_empty() && self.items.len() <= 32,
            "请保留 1–32 个预设"
        );
        let mut ids = std::collections::HashSet::new();
        for p in &self.items {
            ensure!(!p.id.is_empty() && ids.insert(&p.id), "预设标识重复");
            ensure!(
                !p.name.trim().is_empty() && p.name.chars().count() <= 32,
                "预设名称为 1–32 字"
            );
            p.options.validate()?;
        }
        ensure!(ids.contains(&self.default_id), "默认预设不存在");
        Ok(())
    }
}

impl Options {
    pub fn from_settings(s: &Settings) -> Self {
        Self {
            directory: s.directory.clone(),
            template: s.template.clone(),
            format: s.format.clone(),
            quality: s.quality,
            bitrate: s.bitrate,
            cover: s.cover,
            subtitles: s.subtitles,
            danmaku: s.danmaku,
            ..Self::default()
        }
    }
}

// Ignore tracking parameters and output location; retain the part/episode identity.
pub fn identity(item: &MediaItem) -> String {
    if let Ok(u) = url::Url::parse(&item.url) {
        let part = u
            .query_pairs()
            .find(|(k, _)| k == "p")
            .map(|(_, v)| v.into_owned())
            .unwrap_or_else(|| "1".into());
        return format!("{}:{}", u.path().trim_end_matches('/'), part);
    }
    item.id.clone()
}
pub fn history_key(item: &MediaItem, options: &Options) -> String {
    let mut o = options.clone();
    if o.audio() {
        o.quality = 0;
        o.compatibility = false;
        o.watermark = Watermark::default();
    }
    if !["mp3", "m4a"].contains(&o.format.as_str()) {
        o.bitrate = 0;
    }
    o.directory.clear();
    o.template.clear();
    let value = format!("{}:{}", identity(item), serde_json::to_string(&o).unwrap());
    hex::encode(Sha256::digest(value.as_bytes()))
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub key: String,
    pub title: String,
    pub url: String,
    pub output: String,
    pub completed: u64,
    pub format: String,
}
impl Store {
    pub fn record_download(&self, t: &Task) -> Result<()> {
        let h = HistoryEntry {
            key: history_key(&t.item, &t.options),
            title: t.item.title.clone(),
            url: t.item.url.clone(),
            output: t.output.clone(),
            completed: now(),
            format: t.options.format.clone(),
        };
        self.save_history(&h)
    }
}
pub fn existing_parent(path: &Path) -> &Path {
    let mut p = path;
    while !p.exists() {
        if let Some(parent) = p.parent() {
            p = parent
        } else {
            break;
        }
    }
    p
}
pub fn available(path: &Path) -> Result<u64> {
    Ok(fs2::available_space(existing_parent(path))?)
}
pub fn check_space(path: &Path, needed: u64, reserve: u64) -> Result<()> {
    ensure!(
        available(path)? >= needed.saturating_add(reserve),
        "磁盘空间不足，任务已暂停。请清理缓存或释放磁盘空间后继续（{}）",
        path.display()
    );
    Ok(())
}

/// Public VOD links only. No arbitrary flags, local paths, credentials, or live rooms.
pub fn handoff(raw: &str) -> Result<String> {
    ensure!(raw.len() <= 32_768, "链接过长");
    let uri = url::Url::parse(raw)?;
    ensure!(
        uri.scheme() == "bilidownloader"
            && uri.host_str() == Some("download")
            && (uri.path().is_empty() || uri.path() == "/"),
        "无效的联动链接"
    );
    let mut urls = vec![];
    for (key, value) in uri.query_pairs() {
        ensure!(key == "url", "未知联动参数");
        let normalized = crate::provider::normalize_input(&value)?;
        let u = url::Url::parse(&normalized)?;
        ensure!(
            matches!(u.host_str(), Some("www.bilibili.com" | "bilibili.com"))
                && u.path().starts_with("/video/"),
            "请使用已发布视频的 BV/AV 链接，直播间不能作为回放下载"
        );
        let code = u
            .path()
            .trim_matches('/')
            .strip_prefix("video/")
            .unwrap_or("");
        ensure!(
            regex::Regex::new(r"^(BV[a-zA-Z0-9]{10}|av[0-9]+)$")?.is_match(code),
            "视频标识无效"
        );
        let mut clean = format!("https://www.bilibili.com/video/{code}");
        if let Some((_, part)) = u.query_pairs().find(|(k, _)| k == "p") {
            let n: u32 = part.parse()?;
            ensure!(n > 0 && n <= 5000, "分 P 无效");
            clean.push_str(&format!("?p={n}"));
        }
        if !urls.contains(&clean) {
            urls.push(clean);
        }
        ensure!(urls.len() <= 100, "联动单次最多传入 100 条视频链接");
    }
    ensure!(!urls.is_empty(), "联动链接没有视频");
    Ok(urls.join("\n"))
}
