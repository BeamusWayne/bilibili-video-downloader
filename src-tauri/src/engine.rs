use crate::{
    download::{Downloader, Gate, Progress},
    media,
    model::*,
    process::atomic_json,
    provider::Provider,
    store::Store,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

pub struct Engine {
    pub presets: Mutex<crate::library::Presets>,
    pub incoming: Mutex<Option<String>>,
    pub updater: crate::maintenance::Updater,
    pub store: Store,
    pub tasks: Mutex<Vec<Task>>,
    pub settings: Mutex<Settings>,
    pub parse: Mutex<ParseState>,
    pub provider: Provider,
    pub downloader: Downloader,
    pub active: Mutex<HashMap<String, CancellationToken>>,
    pub parse_cancel: Mutex<CancellationToken>,
    pub converter: Arc<Gate>,
    pub resolver: Arc<Gate>,
    pub data: PathBuf,
    pub shutdown: CancellationToken,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    cooldown: u64,
    presets: crate::library::Presets,
    incoming: Option<String>,
    tasks: Vec<Task>,
    settings: Settings,
    parse: ParseState,
    connections: usize,
    version: String,
}
#[derive(Serialize, Deserialize)]
struct Receipt {
    resolved: Resolved,
    hashes: Vec<String>,
}
impl Engine {
    pub fn new(data: PathBuf, tools: PathBuf) -> Result<Arc<Self>> {
        std::fs::create_dir_all(&data)?;
        let store = Store::open(&data.join("tasks.db"))?;
        let settings = store.settings()?;
        let presets = store
            .config("presets")
            .unwrap_or_else(|| crate::library::Presets::initial(&settings));
        let mut tasks = store.tasks()?;
        for t in tasks.iter().filter(|t| t.status == "completed") {
            store.record_download(t)?;
        }
        for t in &mut tasks {
            if [
                "resolving",
                "downloading",
                "converting",
                "retrying",
                "queued",
            ]
            .contains(&t.status.as_str())
            {
                t.status = if settings.resume_on_start {
                    "queued"
                } else {
                    "paused"
                }
                .into();
                t.speed = 0;
                store.save_task(t)?;
            }
        }
        let mut parse = store.load_parse();
        parse.running = false;
        let downloader = Downloader::new(settings.global_connections)?;
        downloader
            .reserve
            .store(settings.disk_reserve_mib * 1024 * 1024, Ordering::Relaxed);
        Ok(Arc::new(Self {
            presets: Mutex::new(presets),
            incoming: Mutex::new(None),
            updater: crate::maintenance::Updater::default(),
            provider: Provider::new(tools, data.join("credentials-temp"))?,
            downloader,
            converter: Gate::new(settings.conversions),
            resolver: Gate::new(1),
            store,
            tasks: Mutex::new(tasks),
            settings: Mutex::new(settings),
            parse: Mutex::new(parse),
            active: Mutex::new(HashMap::new()),
            parse_cancel: Mutex::new(CancellationToken::new()),
            data,
            shutdown: CancellationToken::new(),
        }))
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            cooldown: self.downloader.gate.cooldown(),
            presets: self.presets.lock().unwrap().clone(),
            incoming: self.incoming.lock().unwrap().clone(),
            tasks: self.tasks.lock().unwrap().clone(),
            settings: self.settings.lock().unwrap().clone(),
            parse: self.parse.lock().unwrap().clone(),
            connections: self.downloader.gate.adaptive.load(Ordering::Relaxed),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }
    pub fn update(&self, id: &str, f: impl FnOnce(&mut Task)) -> Result<()> {
        let mut tasks = self.tasks.lock().unwrap();
        let t = tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| anyhow::anyhow!("任务不存在"))?;
        f(t);
        t.updated = now();
        self.store.save_task(t)
    }
    pub fn save_settings(&self, s: Settings) -> Result<()> {
        s.validate()?;
        let old = self.settings.lock().unwrap().directory.clone();
        if old != s.directory {
            let mut presets = self.presets.lock().unwrap();
            let mut next = presets.clone();
            for p in &mut next.items {
                if p.options.directory == old {
                    p.options.directory = s.directory.clone();
                }
            }
            self.store.save_config("presets", &next)?;
            *presets = next;
        }
        self.store.save_settings(&s)?;
        self.downloader
            .gate
            .maximum
            .store(s.global_connections, Ordering::Relaxed);
        self.downloader
            .rate
            .kib
            .store(s.rate_limit_kib, Ordering::Relaxed);
        self.converter
            .maximum
            .store(s.conversions, Ordering::Relaxed);
        self.downloader
            .reserve
            .store(s.disk_reserve_mib * 1024 * 1024, Ordering::Relaxed);
        *self.settings.lock().unwrap() = s;
        Ok(())
    }
    pub fn enqueue(&self, items: Vec<MediaItem>, options: Options) -> Result<usize> {
        self.enqueue_with(items, options, false)
    }
    pub fn duplicates(
        &self,
        items: &[MediaItem],
        options: &Options,
    ) -> Result<HashMap<String, String>> {
        let existing: HashSet<_> = self
            .store
            .history()?
            .into_iter()
            .filter(|h| Path::new(&h.output).is_file())
            .map(|h| h.key)
            .collect();
        let queued_keys: HashSet<_> = self
            .tasks
            .lock()
            .unwrap()
            .iter()
            .filter(|t| !["cancelled", "completed"].contains(&t.status.as_str()))
            .map(|t| crate::library::history_key(&t.item, &t.options))
            .collect();
        Ok(items
            .iter()
            .filter_map(|i| {
                let key = crate::library::history_key(i, options);
                let queued = queued_keys.contains(&key);
                if queued {
                    Some((i.url.clone(), "已在队列".into()))
                } else if existing.contains(&key) {
                    Some((i.url.clone(), "已下载".into()))
                } else {
                    None
                }
            })
            .collect())
    }
    pub fn enqueue_with(
        &self,
        items: Vec<MediaItem>,
        options: Options,
        force: bool,
    ) -> Result<usize> {
        options.validate()?;
        ensure!(items.len() <= 5000, "单次最多添加 5000 项");
        let known = self.duplicates(&items, &options)?;
        let mut tasks = self.tasks.lock().unwrap();
        for item in &items {
            crate::provider::normalize_input(&item.url)?;
        }
        let mut pending = vec![];
        let mut keys: HashSet<_> = tasks
            .iter()
            .filter(|t| !["completed", "cancelled"].contains(&t.status.as_str()))
            .map(|t| crate::library::history_key(&t.item, &t.options))
            .collect();
        for item in items {
            if known
                .get(&item.url)
                .map(|v| v == "已在队列" || !force)
                .unwrap_or(false)
                || !keys.insert(crate::library::history_key(&item, &options))
            {
                continue;
            }
            let mut t = Task::new(item, options.clone());
            let base = output_base(&t);
            let name = format!(
                "{} [{}].{}",
                base.file_name().unwrap_or_default().to_string_lossy(),
                &t.id[..8],
                options.format
            );
            t.output = base.with_file_name(name).to_string_lossy().into_owned();
            pending.push(t);
        }
        self.store.save_tasks(&pending)?;
        let added = pending.len();
        tasks.extend(pending);
        Ok(added)
    }
    pub fn action(&self, id: &str, action: &str) -> Result<()> {
        match action {
            "pause" | "cancel" => {
                let active = self.active.lock().unwrap();
                if let Some(c) = active.get(id) {
                    c.cancel();
                }
                self.update(id, |t| {
                    if t.status != "completed" {
                        t.status = if action == "cancel" {
                            "cancelled"
                        } else {
                            "paused"
                        }
                        .into();
                        t.speed = 0;
                    }
                })?;
            }
            "resume" | "retry" => {
                ensure!(
                    !self.active.lock().unwrap().contains_key(id),
                    "正在停止，请稍后重试"
                );
                self.update(id, |t| {
                    if t.status != "completed" {
                        t.status = "queued".into();
                        t.error.clear();
                        t.speed = 0;
                    }
                })?;
            }
            "remove" => {
                ensure!(
                    !self.active.lock().unwrap().contains_key(id),
                    "请先暂停任务"
                );
                let mut tasks = self.tasks.lock().unwrap();
                self.store.delete_task(id)?;
                tasks.retain(|t| t.id != id);
            }
            _ => anyhow::bail!("未知操作"),
        };
        Ok(())
    }
    pub fn start(self: &Arc<Self>) {
        let engine = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                if engine.shutdown.is_cancelled() {
                    break;
                }
                let max = engine.settings.lock().unwrap().tasks;
                {
                    let mut active = engine.active.lock().unwrap();
                    let tasks = engine.tasks.lock().unwrap();
                    let fetching = tasks
                        .iter()
                        .filter(|t| active.contains_key(&t.id) && t.status != "converting")
                        .count();
                    let allowance = max
                        .saturating_sub(fetching)
                        .min((max + 4).saturating_sub(active.len()));
                    let candidates: Vec<_> = tasks
                        .iter()
                        .filter(|t| t.status == "queued" && !active.contains_key(&t.id))
                        .take(allowance)
                        .cloned()
                        .collect();
                    drop(tasks);
                    for task in candidates {
                        let token = engine.shutdown.child_token();
                        active.insert(task.id.clone(), token.clone());
                        let eng = engine.clone();
                        tauri::async_runtime::spawn(async move {
                            let id = task.id.clone();
                            let result = eng.run_task(task, &token).await;
                            if let Err(e) = result {
                                let raw = crate::process::redact(&e.to_string());
                                let text = if [
                                    "os error 112",
                                    "No space left",
                                    "not enough space on the disk",
                                ]
                                .iter()
                                .any(|s| raw.contains(s))
                                {
                                    "磁盘空间不足，任务已暂停。请释放空间后继续。".to_string()
                                } else {
                                    raw
                                };
                                let _ = eng.update(&id, |t| {
                                    if token.is_cancelled() {
                                        if t.status != "cancelled" {
                                            t.status = "paused".into();
                                        }
                                    } else if text.contains("磁盘空间不足") {
                                        t.status = "paused".into();
                                        t.error = text.clone();
                                    } else {
                                        t.status = "error".into();
                                        t.error = text;
                                    }
                                    t.speed = 0;
                                });
                            }
                            let disk_full = eng
                                .tasks
                                .lock()
                                .unwrap()
                                .iter()
                                .any(|t| t.id == id && t.error.contains("磁盘空间不足"));
                            if disk_full {
                                let ids: Vec<_> = eng
                                    .tasks
                                    .lock()
                                    .unwrap()
                                    .iter()
                                    .filter(|t| {
                                        [
                                            "queued",
                                            "resolving",
                                            "downloading",
                                            "retrying",
                                            "converting",
                                        ]
                                        .contains(&t.status.as_str())
                                    })
                                    .map(|t| t.id.clone())
                                    .collect();
                                for id in ids {
                                    let _ = eng.action(&id, "pause");
                                }
                            }
                            eng.active.lock().unwrap().remove(&id);
                        });
                    }
                }
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
        });
    }
    async fn run_task(self: &Arc<Self>, task: Task, cancel: &CancellationToken) -> Result<()> {
        let reserve = self.settings.lock().unwrap().disk_reserve_mib * 1024 * 1024;
        crate::library::check_space(&self.data, 0, reserve)?;
        crate::library::check_space(Path::new(&task.options.directory), 0, reserve)?;
        if task.options.resource == "extras" {
            self.update(&task.id, |t| {
                t.status = "resolving".into();
                t.stage = "解析附件".into();
            })?;
            let r = {
                let _permit = self.resolver.acquire(cancel).await?;
                self.provider
                    .resolve(&task.item, &task.options, cancel)
                    .await?
            };
            let files = self.auxiliary(&task, &r, cancel).await;
            ensure!(!cancel.is_cancelled(), "已暂停");
            ensure!(!files.is_empty(), "没有成功保存附件，请查看任务详情");
            self.update(&task.id, |t| {
                t.status = "completed".into();
                t.stage = "已完成".into();
                t.output = files[0].to_string_lossy().into_owned();
            })?;
            let completed = self
                .tasks
                .lock()
                .unwrap()
                .iter()
                .find(|t| t.id == task.id)
                .cloned();
            if let Some(t) = completed {
                self.store.record_download(&t)?;
            }
            return Ok(());
        }
        let dir = self.data.join("cache").join(&task.id);
        tokio::fs::create_dir_all(&dir).await?;
        let settings = self.settings.lock().unwrap().clone();
        let downloaded = dir.join("downloaded.json");
        let cached = tokio::fs::read(&downloaded)
            .await
            .ok()
            .and_then(|b| serde_json::from_slice::<Receipt>(&b).ok());
        let mut resolved: Option<Resolved> = None;
        let mut inputs = vec![];
        if let Some(receipt) = cached {
            let r = receipt.resolved;
            let paths: Vec<_> = (0..r.streams.len())
                .map(|i| dir.join(i.to_string()).join("complete.media"))
                .collect();
            let mut valid = paths.len() == receipt.hashes.len();
            for (path, hash) in paths.iter().zip(&receipt.hashes) {
                if crate::download::file_digest(path).await.ok().as_ref() != Some(hash) {
                    valid = false;
                    break;
                }
            }
            if valid {
                resolved = Some(r);
                inputs = paths;
            }
        }
        if resolved.is_none() {
            for refresh in 0..3 {
                ensure!(!cancel.is_cancelled(), "已暂停");
                self.update(&task.id, |t| {
                    t.status = "resolving".into();
                    t.stage = if refresh == 0 {
                        "解析媒体"
                    } else {
                        "刷新下载地址"
                    }
                    .into();
                })?;
                let r = {
                    let _permit = self.resolver.acquire(cancel).await?;
                    self.provider
                        .resolve(&task.item, &task.options, cancel)
                        .await?
                };
                let estimate = r
                    .streams
                    .iter()
                    .map(|s| s.size)
                    .sum::<u64>()
                    .saturating_mul(2);
                let cached_bytes = crate::maintenance::directory_size(&dir);
                crate::library::check_space(&dir, estimate.saturating_sub(cached_bytes), reserve)?;
                self.update(&task.id, |t| {
                    t.status = "downloading".into();
                    t.stage = "下载中".into();
                    t.error.clear();
                })?;
                let progress = Arc::new(Progress::default());
                let monitor_cancel = cancel.child_token();
                let monitor =
                    self.monitor(task.id.clone(), progress.clone(), monitor_cancel.clone());
                let mut paths = vec![];
                let mut failure = None;
                for (index, stream) in r.streams.iter().enumerate() {
                    match self
                        .downloader
                        .fetch(
                            stream,
                            &dir.join(index.to_string()),
                            settings.connections,
                            settings.retries,
                            progress.clone(),
                            cancel,
                        )
                        .await
                    {
                        Ok(path) => paths.push(path),
                        Err(e) => {
                            failure = Some(e);
                            break;
                        }
                    }
                }
                monitor_cancel.cancel();
                let _ = monitor.await;
                self.update(&task.id, |t| {
                    t.downloaded = progress.done.load(Ordering::Relaxed);
                    t.total = progress.total.load(Ordering::Relaxed);
                    t.speed = 0;
                })?;
                if let Some(e) = failure {
                    if refresh < 2
                        && (e.to_string().contains("403") || e.to_string().contains("401"))
                    {
                        continue;
                    }
                    return Err(e);
                }
                let mut hashes = vec![];
                for path in &paths {
                    hashes.push(crate::download::file_digest(path).await?);
                }
                atomic_json(
                    &downloaded,
                    &Receipt {
                        resolved: r.clone(),
                        hashes,
                    },
                )
                .await?;
                inputs = paths;
                resolved = Some(r);
                break;
            }
        }
        let r = resolved.ok_or_else(|| anyhow::anyhow!("未能取得媒体资源"))?;
        ensure!(!cancel.is_cancelled(), "已暂停");
        self.update(&task.id, |t| {
            t.status = "converting".into();
            t.stage = "等待处理".into();
        })?;
        let _permit = self.converter.acquire(cancel).await?;
        let input_bytes = inputs
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum::<u64>();
        let estimate = if task.options.format == "wav" {
            (r.duration * 192_000.).max(input_bytes as f64) as u64
        } else {
            input_bytes.saturating_mul(2)
        };
        crate::library::check_space(Path::new(&task.options.directory), estimate, reserve)?;
        let eng = self.clone();
        let id = task.id.clone();
        let output_dir = task.options.directory.clone();
        let conversion_cancel = cancel.clone();
        let cb = Arc::new(move |percent: f64| {
            if crate::library::check_space(Path::new(&output_dir), 0, reserve).is_err() {
                let _ = eng.update(&id, |t| {
                    t.error = "磁盘空间不足，任务已暂停。请释放空间后继续。".into()
                });
                conversion_cancel.cancel();
                return;
            }
            let _ = eng.update(&id, |t| t.stage = format!("处理中 · {:.0}%", percent));
        });
        if task.options.watermark.keep_original
            && !task.options.watermark.mode.is_empty()
            && task.options.watermark.mode != "none"
            && !task.options.audio()
        {
            let mut original = task.clone();
            original.options.watermark = Watermark::default();
            original.options.compatibility = false;
            let path = Path::new(&task.output);
            let original_path = path.with_file_name(format!(
                "{}-原始.{}",
                path.file_stem().unwrap().to_string_lossy(),
                task.options.format
            ));
            if !original_path.exists() {
                media::convert(
                    &self.provider.tools,
                    &original,
                    &r,
                    &inputs,
                    &original_path,
                    cancel,
                    Arc::new(|_| {}),
                )
                .await?;
            }
        }
        media::convert(
            &self.provider.tools,
            &task,
            &r,
            &inputs,
            Path::new(&task.output),
            cancel,
            cb,
        )
        .await?;
        self.auxiliary(&task, &r, cancel).await;
        ensure!(!cancel.is_cancelled(), "已暂停");
        self.update(&task.id, |t| {
            t.status = "completed".into();
            t.stage = "已完成".into();
            t.error.clear();
            t.speed = 0;
        })?;
        self.store.record_download(&task)?;
        Ok(())
    }
    fn monitor(
        self: &Arc<Self>,
        id: String,
        p: Arc<Progress>,
        cancel: CancellationToken,
    ) -> tauri::async_runtime::JoinHandle<()> {
        let eng = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut last = 0;
            loop {
                tokio::select! {_=cancel.cancelled()=>break,_=tokio::time::sleep(Duration::from_secs(1))=>{let done=p.done.load(Ordering::Relaxed);let _=eng.update(&id,|t|{t.downloaded=done;t.total=p.total.load(Ordering::Relaxed);t.speed=done.saturating_sub(last);});last=done;}}
            }
        })
    }
    async fn auxiliary(&self, task: &Task, r: &Resolved, c: &CancellationToken) -> Vec<PathBuf> {
        let base = PathBuf::from(&task.output);
        let mut warnings = vec![];
        let mut saved = vec![];
        if let Some(parent) = base.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                let _ = self.update(&task.id, |t| t.warnings = vec![e.to_string()]);
                return saved;
            }
        }
        if task.options.cover && !r.thumbnail.is_empty() {
            match self.provider.auxiliary(&r.thumbnail).await {
                Ok(bytes) => {
                    let ext = if bytes.starts_with(b"\x89PNG") {
                        "png"
                    } else if bytes.starts_with(b"RIFF") {
                        "webp"
                    } else {
                        "jpg"
                    };
                    let path = base.with_extension(format!("cover.{ext}"));
                    match crate::process::atomic_bytes(&path, &bytes).await {
                        Ok(()) => saved.push(path),
                        Err(e) => warnings.push(e.to_string()),
                    }
                }
                Err(_) => warnings.push("封面保存失败".into()),
            }
        }
        if task.options.subtitles {
            let mut found = false;
            if let Some(langs) = r.subtitles.as_object() {
                for (lang, entries) in langs {
                    if lang == "danmaku" {
                        continue;
                    }
                    if c.is_cancelled() {
                        break;
                    }
                    if let Some(entry) = entries
                        .as_array()
                        .and_then(|a| a.iter().find(|v| v["ext"] == "srt").or_else(|| a.first()))
                    {
                        found = true;
                        let bytes = if let Some(data) = entry["data"].as_str() {
                            Ok(data.as_bytes().to_vec())
                        } else if let Some(url) = entry["url"].as_str() {
                            self.provider.auxiliary(url).await
                        } else {
                            Err(anyhow::anyhow!("字幕没有内容"))
                        };
                        match bytes {
                            Ok(bytes) => {
                                let ext = entry["ext"].as_str().unwrap_or("json");
                                let content = if ext == "json" {
                                    media::srt_from_bili(&bytes).map(|s| s.into_bytes())
                                } else {
                                    Ok(bytes)
                                };
                                match content {
                                    Ok(data) => {
                                        let path = base.with_extension(format!(
                                            "{}.{}",
                                            safe_name(lang),
                                            if ext == "json" { "srt" } else { ext }
                                        ));
                                        match crate::process::atomic_bytes(&path, &data).await {
                                            Ok(()) => saved.push(path),
                                            Err(_) => {
                                                warnings.push(format!("字幕 {lang} 保存失败"))
                                            }
                                        }
                                    }
                                    Err(_) => warnings.push(format!("字幕 {lang} 转换失败")),
                                }
                            }
                            Err(_) => warnings.push(format!("字幕 {lang} 下载失败")),
                        }
                    }
                }
            }
            if !found {
                warnings.push("此视频未提供可访问字幕".into());
            }
        }
        if task.options.danmaku {
            if let Some(cid) = &r.cid {
                match self
                    .provider
                    .auxiliary(&format!("https://comment.bilibili.com/{cid}.xml"))
                    .await
                {
                    Ok(data) => {
                        let path = base.with_extension("danmaku.xml");
                        match crate::process::atomic_bytes(&path, &data).await {
                            Ok(()) => saved.push(path),
                            Err(_) => warnings.push("弹幕保存失败".into()),
                        }
                    }
                    Err(_) => warnings.push("弹幕下载失败".into()),
                }
            } else {
                warnings.push("当前解析结果未提供弹幕标识".into());
            }
        }
        let _ = self.update(&task.id, |t| t.warnings = warnings);
        saved
    }
    pub fn parse_inputs(self: &Arc<Self>, input: String) -> Result<()> {
        ensure!(
            !self.parse.lock().unwrap().running,
            "正在解析，请等待或取消"
        );
        let urls: Vec<_> = input
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        ensure!(
            !urls.is_empty() && urls.len() <= 5000,
            "请输入 1–5000 条链接"
        );
        let token = CancellationToken::new();
        *self.parse_cancel.lock().unwrap() = token.clone();
        {
            let mut p = self.parse.lock().unwrap();
            p.running = true;
            p.total = urls.len();
            p.processed = 0;
            p.errors.clear();
        }
        let engine = self.clone();
        tauri::async_runtime::spawn(async move {
            for url in urls {
                if token.is_cancelled() {
                    break;
                }
                let result = async {
                    let _permit = engine.resolver.acquire(&token).await?;
                    engine.provider.enumerate(&url, &token).await
                }
                .await;
                let mut p = engine.parse.lock().unwrap();
                match result {
                    Ok(items) => {
                        let mut known: HashSet<_> = p.items.iter().map(|i| i.url.clone()).collect();
                        for item in items {
                            if known.insert(item.url.clone()) {
                                p.items.push(item);
                            }
                        }
                    }
                    Err(e) => {
                        if !token.is_cancelled() {
                            p.errors.push(format!(
                                "{}：{}",
                                crate::model::safe_name(&url),
                                crate::process::redact(&e.to_string())
                            ));
                        }
                    }
                }
                p.processed += 1;
                let _ = engine.store.save_parse(&p);
            }
            let mut p = engine.parse.lock().unwrap();
            p.running = false;
            let _ = engine.store.save_parse(&p);
        });
        Ok(())
    }
    pub fn stop(&self) {
        self.shutdown.cancel();
        self.parse_cancel.lock().unwrap().cancel();
        for (_, c) in self.active.lock().unwrap().iter() {
            c.cancel();
        }
    }
}
