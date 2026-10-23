use crate::{model::*, process::capture};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::{
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

pub struct Provider {
    pub tools: PathBuf,
    pub cookie: Mutex<String>,
    pub client: reqwest::Client,
    pub cache: PathBuf,
    extraction_gate: Arc<crate::download::Gate>,
}
fn credential() -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(
        "BilibiliVideoDownloader",
        "bilibili-cookie",
    )?)
}
impl Provider {
    pub fn new(tools: PathBuf, cache: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&cache)?;
        for e in std::fs::read_dir(&cache)? {
            let e = e?;
            if e.file_name().to_string_lossy().starts_with("cookie-") {
                let _ = std::fs::remove_file(e.path());
            }
        }
        let cookie = credential()
            .ok()
            .and_then(|e| e.get_password().ok())
            .unwrap_or_default();
        Ok(Self{tools,cookie:Mutex::new(cookie),client:reqwest::Client::builder().timeout(Duration::from_secs(25)).user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/130.0.0.0 Safari/537.36").build()?,cache,extraction_gate:crate::download::Gate::new(1)})
    }
    fn cookie_file(&self) -> Result<Option<tempfile::NamedTempFile>> {
        let cookie = self.cookie.lock().unwrap().clone();
        if cookie.is_empty() {
            return Ok(None);
        }
        let mut file = tempfile::Builder::new()
            .prefix("cookie-")
            .suffix(".txt")
            .tempfile_in(&self.cache)?;
        writeln!(file, "# Netscape HTTP Cookie File")?;
        for pair in cookie.split(';') {
            if let Some((k, v)) = pair.trim().split_once('=') {
                if !k.contains(['\t', '\n', '\r']) && !v.contains(['\t', '\n', '\r']) {
                    writeln!(file, ".bilibili.com\tTRUE\t/\tTRUE\t0\t{k}\t{v}")?;
                }
            }
        }
        file.flush()?;
        Ok(Some(file))
    }
    async fn extract(
        &self,
        url: &str,
        args: &[String],
        cancel: &CancellationToken,
    ) -> Result<Value> {
        let jar = self.cookie_file()?;
        let mut argv = vec![
            "--ignore-config".into(),
            "--no-warnings".into(),
            "--no-progress".into(),
            "--skip-download".into(),
            "--dump-single-json".into(),
            "--socket-timeout".into(),
            "20".into(),
            "--retries".into(),
            "3".into(),
            "--extractor-retries".into(),
            "3".into(),
            "--sleep-requests".into(),
            "0.4".into(),
            "--encoding".into(),
            "utf-8".into(),
        ];
        if let Some(jar) = &jar {
            argv.extend([
                "--cookies".into(),
                jar.path().to_string_lossy().into_owned(),
            ]);
        }
        argv.extend_from_slice(args);
        argv.extend(["--".into(), normalize_input(url)?]);
        let _permit = self.extraction_gate.acquire(cancel).await?;
        for attempt in 0..3 {
            let result = capture(
                &crate::platform::tool(&self.tools, "yt-dlp"),
                &argv,
                cancel,
                Duration::from_secs(900),
            )
            .await;
            if let Err(e) = &result {
                let message = e.to_string();
                if message.contains("412") || message.contains("429") || message.contains("-352") {
                    self.extraction_gate.throttle(60);
                    anyhow::bail!(
                    "B站暂时限制请求，后续解析已冷却 60 秒。请稍后重试；如未登录，可先登录账号。{}",
                    message
                );
                }
                let transient = [
                    "IncompleteRead",
                    "timed out",
                    "Timeout",
                    "Connection reset",
                    "Remote end closed",
                    "HTTP Error 5",
                    "Unable to download",
                    "Error reading response",
                ]
                .iter()
                .any(|s| message.contains(s));
                if attempt < 2 && transient && !cancel.is_cancelled() {
                    tokio::select! { _=cancel.cancelled()=>anyhow::bail!("已暂停"), _=tokio::time::sleep(Duration::from_secs(2u64.pow(attempt+1)))=>{} }
                    continue;
                }
            }
            let bytes = result?;
            return Ok(serde_json::from_slice(&bytes)?);
        }
        unreachable!()
    }
    pub async fn enumerate(&self, url: &str, cancel: &CancellationToken) -> Result<Vec<MediaItem>> {
        let normalized = normalize_input(url)?;
        let parsed = url::Url::parse(&normalized)?;
        if parsed.path().starts_with("/video/") {
            if let Ok(items) = self.video_items(&parsed, cancel).await {
                return Ok(items);
            }
            ensure!(!cancel.is_cancelled(), "已暂停");
        }
        let value = self
            .extract(
                url,
                &[
                    "--flat-playlist".into(),
                    "--yes-playlist".into(),
                    "--ignore-errors".into(),
                    "--playlist-end".into(),
                    "5000".into(),
                ],
                cancel,
            )
            .await?;
        let mut items = vec![];
        flatten(&value, url, &mut items);
        ensure!(
            !items.is_empty(),
            "没有找到可下载的内容，可能需要登录或更新解析器"
        );
        Ok(items)
    }
    async fn video_items(
        &self,
        url: &url::Url,
        cancel: &CancellationToken,
    ) -> Result<Vec<MediaItem>> {
        let code = url
            .path()
            .trim_start_matches("/video/")
            .trim_end_matches('/');
        let key = if code.starts_with("BV") {
            "bvid"
        } else {
            "aid"
        };
        let value = code.strip_prefix("av").unwrap_or(code);
        let cookie = self.cookie.lock().unwrap().clone();
        let _permit = self.extraction_gate.acquire(cancel).await?;
        let request = self
            .client
            .get("https://api.bilibili.com/x/web-interface/view")
            .query(&[(key, value)])
            .header("Referer", "https://www.bilibili.com/")
            .header("Cookie", cookie);
        let response = tokio::select! { _=cancel.cancelled()=>anyhow::bail!("已暂停"), response=request.send()=>response? };
        let v: Value = response.error_for_status()?.json().await?;
        ensure!(v["code"].as_i64() == Some(0), "视频元数据暂不可用");
        let d = &v["data"];
        let bvid = d["bvid"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 BV 号"))?;
        let selected = url
            .query_pairs()
            .find(|(k, _)| k == "p")
            .and_then(|(_, v)| v.parse::<u64>().ok());
        let pages = d["pages"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("缺少分 P 列表"))?;
        let items: Vec<_> = pages
            .iter()
            .filter(|p| selected.is_none() || selected == p["page"].as_u64())
            .map(|p| {
                let index = p["page"].as_u64().unwrap_or(1) as u32;
                MediaItem {
                    id: format!("{bvid}_{}", p["cid"].as_u64().unwrap_or(0)),
                    url: format!("https://www.bilibili.com/video/{bvid}?p={index}"),
                    title: p["part"]
                        .as_str()
                        .or(d["title"].as_str())
                        .unwrap_or(bvid)
                        .into(),
                    album_title: d["title"].as_str().unwrap_or(bvid).into(),
                    uploader: d["owner"]["name"].as_str().unwrap_or("").into(),
                    thumbnail: d["pic"]
                        .as_str()
                        .unwrap_or("")
                        .replacen("http://", "https://", 1),
                    duration: p["duration"].as_f64().unwrap_or(0.),
                    index,
                }
            })
            .collect();
        ensure!(!items.is_empty(), "未找到指定分 P");
        Ok(items)
    }
    pub async fn resolve(
        &self,
        item: &MediaItem,
        opt: &Options,
        cancel: &CancellationToken,
    ) -> Result<Resolved> {
        let height = if opt.quality > 0 {
            format!("[height<={}]", opt.quality)
        } else {
            String::new()
        };
        let format = if opt.audio() {
            if opt.format == "m4a" {
                "ba[ext=m4a]/ba/b".into()
            } else {
                "ba/b".into()
            }
        } else {
            format!("bv{height}+ba/b{height}")
        };
        let mut extra = vec!["--no-playlist".into(), "--format".into(), format];
        if opt.subtitles || opt.danmaku {
            extra.extend(["--write-subs".into(), "--sub-langs".into(), "all".into()]);
        }
        let mut v = self.extract(&item.url, &extra, cancel).await?;
        if v["entries"].is_array() {
            v = v["entries"]
                .as_array()
                .and_then(|a| a.first())
                .cloned()
                .unwrap_or(Value::Null);
        }
        let formats = v["requested_formats"]
            .as_array()
            .cloned()
            .unwrap_or_else(|| vec![v.clone()]);
        let mut streams = vec![];
        for f in formats {
            let kind = if f["vcodec"].as_str() == Some("none") {
                "audio"
            } else {
                "video"
            };
            let mut headers = std::collections::HashMap::new();
            for h in [&v["http_headers"], &f["http_headers"]] {
                if let Some(map) = h.as_object() {
                    for (k, val) in map {
                        if let Some(s) = val.as_str() {
                            if !["cookie", "authorization"]
                                .contains(&k.to_ascii_lowercase().as_str())
                            {
                                headers.insert(k.clone(), s.into());
                            }
                        }
                    }
                }
            }
            let id = f["format_id"].as_str().unwrap_or("source");
            if let Some(parts) = f["fragments"].as_array() {
                for (index, part) in parts.iter().enumerate() {
                    if let Some(url) = part["url"].as_str() {
                        streams.push(Stream {
                            id: format!("{}-{id}-{index}", item.id),
                            url: url.into(),
                            headers: headers.clone(),
                            kind: kind.into(),
                            size: 0,
                        });
                    }
                }
            } else if let Some(url) = f["url"].as_str() {
                streams.push(Stream {
                    id: format!("{}-{id}", item.id),
                    url: url.into(),
                    headers,
                    kind: kind.into(),
                    size: f["filesize"].as_u64().unwrap_or(0),
                });
            }
        }
        ensure!(!streams.is_empty(), "未取得可下载媒体流");
        ensure!(streams.len() < 1000, "此媒体分段过多，暂不支持");
        Ok(Resolved {
            streams,
            duration: v["duration"].as_f64().unwrap_or(item.duration),
            title: v["title"].as_str().unwrap_or(&item.title).into(),
            width: v["width"].as_u64().unwrap_or(0) as u32,
            height: v["height"].as_u64().unwrap_or(0) as u32,
            subtitles: v["subtitles"].clone(),
            thumbnail: v["thumbnail"].as_str().unwrap_or(&item.thumbnail).into(),
            cid: v["subtitles"]["danmaku"][0]["url"]
                .as_str()
                .and_then(|s| s.rsplit('/').next())
                .and_then(|s| s.strip_suffix(".xml"))
                .map(str::to_owned)
                .or_else(|| {
                    v["id"]
                        .as_str()
                        .and_then(|s| s.split_once('_').map(|(_, c)| c.to_owned()))
                        .filter(|s| s.chars().all(|c| c.is_ascii_digit()))
                }),
        })
    }
    pub async fn account_with(&self, cookie: &str) -> Result<Account> {
        let v: Value = self
            .client
            .get("https://api.bilibili.com/x/web-interface/nav")
            .header("Cookie", cookie)
            .header("Referer", "https://www.bilibili.com/")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let d = &v["data"];
        let mut account = Account {
            logged_in: d["isLogin"].as_bool().unwrap_or(false),
            name: d["uname"].as_str().unwrap_or("").into(),
            uid: d["mid"].as_u64().unwrap_or(0),
            vip: d["vipStatus"].as_u64().unwrap_or(0) > 0,
            avatar: String::new(),
            cached: false,
        };
        if account.logged_in {
            let profile = self.cache.parent().unwrap_or(&self.cache).join("profile");
            let avatar = profile.join(format!("{}.image", account.uid));
            if avatar.exists() {
                account.avatar = avatar.to_string_lossy().into_owned();
            }
            let fresh = std::fs::metadata(&avatar)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.elapsed().ok())
                .map(|d| d.as_secs() < 86400)
                .unwrap_or(false);
            if !fresh {
                if let Some(url) = d["face"].as_str() {
                    let url = url.replacen("http://", "https://", 1);
                    if let Ok(bytes) = self.auxiliary(&url).await {
                        if bytes.len() < 2 * 1024 * 1024
                            && (bytes.starts_with(b"\x89PNG")
                                || bytes.starts_with(b"\xff\xd8\xff")
                                || bytes.starts_with(b"RIFF"))
                        {
                            if crate::process::atomic_bytes(&avatar, &bytes).await.is_ok() {
                                account.avatar = avatar.to_string_lossy().into_owned();
                            }
                        }
                    }
                }
            }
            let _ = crate::process::atomic_json(&profile.join("account.json"), &account).await;
        }
        Ok(account)
    }
    pub fn cached_account(&self) -> Option<Account> {
        if self.cookie.lock().unwrap().is_empty() {
            return None;
        }
        let path = self
            .cache
            .parent()
            .unwrap_or(&self.cache)
            .join("profile/account.json");
        let mut account: Account = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        account.cached = true;
        Some(account)
    }
    pub async fn account(&self) -> Result<Account> {
        let cookie = self.cookie.lock().unwrap().clone();
        match self.account_with(&cookie).await {
            Ok(account) => Ok(account),
            Err(e) => self.cached_account().ok_or(e),
        }
    }
    pub async fn import_cookie(&self, input: &str) -> Result<Account> {
        let cookie = normalize_cookie(input)?;
        let account = self.account_with(&cookie).await?;
        ensure!(account.logged_in, "Cookie 无效或已过期");
        credential()?.set_password(&cookie)?;
        *self.cookie.lock().unwrap() = cookie;
        Ok(account)
    }
    pub fn logout(&self) -> Result<()> {
        let profile = self.cache.parent().unwrap_or(&self.cache).join("profile");
        let uid = self.cached_account().map(|a| a.uid);
        if let Ok(e) = credential() {
            match e.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => return Err(e.into()),
            }
        }
        self.cookie.lock().unwrap().clear();
        let _ = std::fs::remove_file(profile.join("account.json"));
        if let Some(uid) = uid {
            let _ = std::fs::remove_file(profile.join(format!("{uid}.image")));
        }
        Ok(())
    }
    pub async fn qr(&self) -> Result<Value> {
        let v: Value = self
            .client
            .get("https://passport.bilibili.com/x/passport-login/web/qrcode/generate")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        ensure!(v["code"].as_i64() == Some(0), "无法生成二维码");
        Ok(v["data"].clone())
    }
    pub async fn poll_qr(&self, key: &str) -> Result<Value> {
        ensure!(key.len() < 256, "无效二维码");
        let r = self
            .client
            .get("https://passport.bilibili.com/x/passport-login/web/qrcode/poll")
            .query(&[("qrcode_key", key)])
            .send()
            .await?
            .error_for_status()?;
        let cookies: Vec<String> = r
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|h| h.to_str().ok())
            .filter_map(|s| s.split(';').next())
            .map(str::to_owned)
            .collect();
        let v: Value = r.json().await?;
        let code = v["data"]["code"].as_i64().unwrap_or(-1);
        if code == 0 {
            let cookie = cookies.join("; ");
            let account = self.import_cookie(&cookie).await?;
            return Ok(json!({"code":0,"account":account}));
        }
        Ok(json!({"code":code}))
    }
    pub async fn auxiliary(&self, url: &str) -> Result<Vec<u8>> {
        let u = url::Url::parse(url)?;
        let host = u.host_str().unwrap_or("");
        ensure!(
            u.scheme() == "https"
                && (host.ends_with(".hdslb.com")
                    || host.ends_with(".bilibili.com")
                    || host == "bilibili.com"),
            "不允许的附件地址"
        );
        let r = self
            .client
            .get(url)
            .header("Referer", "https://www.bilibili.com/")
            .send()
            .await?
            .error_for_status()?;
        ensure!(
            r.content_length().unwrap_or(0) < 32 * 1024 * 1024,
            "附件过大"
        );
        let data = r.bytes().await?;
        ensure!(data.len() < 32 * 1024 * 1024, "附件过大");
        Ok(data.to_vec())
    }
}
pub fn normalize_input(s: &str) -> Result<String> {
    let s = s.trim().trim_matches(|c: char| "，。；\"'<>".contains(c));
    let code = regex::Regex::new(r"(?i)^(BV[a-zA-Z0-9]+|av[0-9]+)$")?;
    if code.is_match(s) {
        return Ok(format!("https://www.bilibili.com/video/{s}"));
    }
    let u = url::Url::parse(s).map_err(|_| anyhow::anyhow!("请输入 B站链接或 BV/AV 号"))?;
    let h = u.host_str().unwrap_or("");
    ensure!(
        ["http", "https"].contains(&u.scheme())
            && (h == "bilibili.com" || h.ends_with(".bilibili.com") || h == "b23.tv"),
        "仅支持 B站链接"
    );
    ensure!(
        u.username().is_empty() && u.password().is_none(),
        "无效链接"
    );
    Ok(s.into())
}
fn flatten(v: &Value, fallback: &str, out: &mut Vec<MediaItem>) {
    if let Some(entries) = v["entries"].as_array() {
        for e in entries {
            let mut entry = e.clone();
            if entry.is_object() {
                for name in ["uploader", "thumbnail"] {
                    if entry[name].as_str().unwrap_or("").is_empty() {
                        entry[name] = v[name].clone();
                    }
                }
                if entry["playlist_title"].is_null() {
                    entry["playlist_title"] = v["title"].clone();
                }
            }
            flatten(&entry, fallback, out);
        }
        return;
    }
    if v.is_null() {
        return;
    }
    let raw = v["webpage_url"]
        .as_str()
        .or(v["url"].as_str())
        .unwrap_or(fallback);
    let url = normalize_input(raw).unwrap_or_else(|_| fallback.into());
    let id = v["id"].as_str().filter(|s| !s.is_empty()).unwrap_or(&url);
    let title = v["title"].as_str().filter(|s| !s.is_empty()).unwrap_or(id);
    let thumbnail = v["thumbnail"]
        .as_str()
        .or_else(|| {
            v["thumbnails"]
                .as_array()
                .and_then(|a| a.last())
                .and_then(|t| t["url"].as_str())
        })
        .unwrap_or("");
    out.push(MediaItem {
        id: id.into(),
        url: url.clone(),
        title: title.into(),
        album_title: v["playlist_title"].as_str().unwrap_or("").into(),
        uploader: v["uploader"].as_str().unwrap_or("").into(),
        thumbnail: thumbnail.into(),
        duration: v["duration"].as_f64().unwrap_or(0.),
        index: v["playlist_index"].as_u64().unwrap_or(out.len() as u64 + 1) as u32,
    });
}
pub fn normalize_cookie(s: &str) -> Result<String> {
    let s = s.trim();
    ensure!(s.len() < 32_000, "Cookie 内容过长");
    let result = if s.starts_with('[') {
        let values: Value = serde_json::from_str(s)?;
        values
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| {
                v["domain"]
                    .as_str()
                    .map(|d| d == "bilibili.com" || d.ends_with(".bilibili.com"))
                    .unwrap_or(true)
            })
            .filter_map(|v| Some(format!("{}={}", v["name"].as_str()?, v["value"].as_str()?)))
            .collect::<Vec<_>>()
            .join("; ")
    } else if s.contains('\t') {
        s.lines()
            .filter(|l| !l.starts_with('#') || l.starts_with("#HttpOnly_"))
            .filter_map(|l| {
                let cols: Vec<_> = l.split('\t').collect();
                if cols.len() == 7 && cols[0].ends_with("bilibili.com") {
                    Some(format!("{}={}", cols[5], cols[6]))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("; ")
    } else {
        s.strip_prefix("Cookie:").unwrap_or(s).trim().to_owned()
    };
    ensure!(
        !result.contains(['\r', '\n', '\t'])
            && result.split(';').any(|p| p.trim().starts_with("SESSDATA=")),
        "未找到 SESSDATA，请导入完整 B站 Cookie"
    );
    Ok(result)
}
