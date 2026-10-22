use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub directory: String,
    pub tasks: usize,
    pub connections: usize,
    pub global_connections: usize,
    pub conversions: usize,
    pub retries: usize,
    pub resume_on_start: bool,
    pub tray: bool,
    pub rate_limit_kib: u64,
    pub format: String,
    pub quality: u32,
    pub bitrate: u32,
    pub cover: bool,
    pub subtitles: bool,
    pub danmaku: bool,
    pub template: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            directory: std::env::var("USERPROFILE")
                .map(|p| {
                    PathBuf::from(p)
                        .join("Downloads")
                        .join("Bilibili")
                        .to_string_lossy()
                        .into_owned()
                })
                .unwrap_or_else(|_| "downloads".into()),
            tasks: 3,
            connections: 4,
            global_connections: 12,
            conversions: 1,
            retries: 5,
            resume_on_start: true,
            tray: true,
            rate_limit_kib: 0,
            format: "mp4".into(),
            quality: 0,
            bitrate: 320,
            cover: true,
            subtitles: true,
            danmaku: false,
            template: "{UP主}/{标题}/{P序号}-{分P标题}".into(),
        }
    }
}
impl Settings {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (1..=16).contains(&self.tasks)
                && (1..=8).contains(&self.connections)
                && (1..=64).contains(&self.global_connections)
                && (1..=4).contains(&self.conversions),
            "并发设置超出范围"
        );
        anyhow::ensure!(self.retries <= 10, "最多重试 10 次");
        anyhow::ensure!(
            PathBuf::from(&self.directory).is_absolute(),
            "请选择绝对保存路径"
        );
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub id: String,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub album_title: String,
    #[serde(default)]
    pub uploader: String,
    #[serde(default)]
    pub thumbnail: String,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub index: u32,
}
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Watermark {
    pub mode: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub start: f64,
    pub end: f64,
    pub keep_original: bool,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
    pub resource: String,
    pub format: String,
    pub quality: u32,
    pub bitrate: u32,
    pub compatibility: bool,
    pub cover: bool,
    pub subtitles: bool,
    pub danmaku: bool,
    pub directory: String,
    pub template: String,
    pub watermark: Watermark,
}
impl Default for Options {
    fn default() -> Self {
        let s = Settings::default();
        Self {
            resource: "media".into(),
            format: s.format,
            quality: 0,
            bitrate: 320,
            compatibility: false,
            cover: true,
            subtitles: true,
            danmaku: false,
            directory: s.directory,
            template: s.template,
            watermark: Watermark {
                keep_original: true,
                ..Default::default()
            },
        }
    }
}
impl Options {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            ["media", "extras"].contains(&self.resource.as_str()),
            "无效资源类型"
        );
        anyhow::ensure!(
            self.resource != "extras" || self.cover || self.subtitles || self.danmaku,
            "请选择至少一种附件"
        );
        anyhow::ensure!(
            ["mp4", "mkv", "mp3", "m4a", "flac", "wav"].contains(&self.format.as_str()),
            "不支持的输出格式"
        );
        anyhow::ensure!(
            (32..=320).contains(&self.bitrate),
            "音频码率应为 32–320 kbps"
        );
        anyhow::ensure!(
            PathBuf::from(&self.directory).is_absolute(),
            "请选择保存目录"
        );
        anyhow::ensure!(
            self.directory.encode_utf16().count() < 170,
            "保存目录路径过长，请选择较短的目录"
        );
        let w = &self.watermark;
        anyhow::ensure!(
            ["", "none", "delogo", "blur", "crop"].contains(&w.mode.as_str()),
            "无效的画面处理方式"
        );
        if !w.mode.is_empty() && w.mode != "none" {
            anyhow::ensure!(
                w.width > 0
                    && w.height > 0
                    && w.start >= 0.
                    && w.end >= 0.
                    && (w.end == 0. || w.end > w.start),
                "请设置有效的处理区域和时间范围"
            );
        }
        Ok(())
    }
    pub fn audio(&self) -> bool {
        !["mp4", "mkv"].contains(&self.format.as_str())
    }
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub item: MediaItem,
    pub options: Options,
    pub status: String,
    pub stage: String,
    pub downloaded: u64,
    pub total: u64,
    pub speed: u64,
    pub error: String,
    pub warnings: Vec<String>,
    pub output: String,
    pub created: u64,
    pub updated: u64,
}
impl Task {
    pub fn new(item: MediaItem, options: Options) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            item,
            options,
            status: "queued".into(),
            stage: "等待下载".into(),
            downloaded: 0,
            total: 0,
            speed: 0,
            error: String::new(),
            warnings: vec![],
            output: String::new(),
            created: now(),
            updated: now(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ParseState {
    pub running: bool,
    pub items: Vec<MediaItem>,
    pub errors: Vec<String>,
    pub processed: usize,
    pub total: usize,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub logged_in: bool,
    pub name: String,
    pub uid: u64,
    pub vip: bool,
    #[serde(default)]
    pub avatar: String,
    #[serde(default)]
    pub cached: bool,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Stream {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    pub kind: String,
    #[serde(default)]
    pub size: u64,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Resolved {
    pub streams: Vec<Stream>,
    pub duration: f64,
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub subtitles: serde_json::Value,
    pub thumbnail: String,
    pub cid: Option<String>,
}
pub fn safe_name(s: &str) -> String {
    let mut n: String = s
        .chars()
        .map(|c| {
            if c < ' ' || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(100)
        .collect();
    n = n.trim().trim_end_matches(['.', ' ']).to_owned();
    if n.is_empty() {
        n = "untitled".into();
    }
    let stem = n.split('.').next().unwrap_or("").to_ascii_uppercase();
    if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", ".", "..",
    ]
    .contains(&stem.as_str())
    {
        n.insert(0, '_');
    }
    n
}
pub fn output_base(task: &Task) -> PathBuf {
    let item = &task.item;
    let mut template = task.options.template.clone();
    for (k, v) in [
        ("{UP主}", safe_name(&item.uploader)),
        (
            "{标题}",
            safe_name(if item.album_title.is_empty() {
                &item.title
            } else {
                &item.album_title
            }),
        ),
        ("{分P标题}", safe_name(&item.title)),
        ("{P序号}", format!("{:02}", item.index.max(1))),
        ("{BV号}", safe_name(&item.id)),
    ] {
        template = template.replace(k, &v);
    }
    let mut p = PathBuf::from(&task.options.directory);
    let components: Vec<_> = template
        .split(['/', '\\'])
        .filter(|x| !x.is_empty())
        .take(4)
        .collect();
    let budget = (210usize.saturating_sub(task.options.directory.encode_utf16().count()))
        / components.len().max(1);
    for c in components {
        let safe = safe_name(c);
        let mut count = 0;
        let shortened: String = safe
            .chars()
            .take_while(|c| {
                count += c.len_utf16();
                count <= budget.max(8)
            })
            .collect();
        p.push(safe_name(&shortened));
    }
    if p == PathBuf::from(&task.options.directory) {
        p.push(safe_name(&item.title));
    }
    p
}
