use crate::{model::Stream, process::atomic_json};
use anyhow::{anyhow, bail, ensure, Result};
use futures_util::{stream, StreamExt, TryStreamExt};
use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

pub const CHUNK: u64 = 4 * 1024 * 1024;
#[derive(Default)]
pub struct Gate {
    pub maximum: AtomicUsize,
    used: AtomicUsize,
    cool_until: AtomicU64,
    pub adaptive: AtomicUsize,
}
pub struct Permit(Arc<Gate>);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.used.fetch_sub(1, Ordering::SeqCst);
    }
}
impl Gate {
    pub fn new(max: usize) -> Arc<Self> {
        Arc::new(Self {
            maximum: AtomicUsize::new(max),
            adaptive: AtomicUsize::new(max),
            ..Default::default()
        })
    }
    pub async fn acquire(self: &Arc<Self>, cancel: &CancellationToken) -> Result<Permit> {
        loop {
            let now = crate::model::now();
            let until = self.cool_until.load(Ordering::Relaxed);
            if now > until + 60 {
                self.adaptive
                    .store(self.maximum.load(Ordering::Relaxed), Ordering::Relaxed);
            }
            let limit = self
                .maximum
                .load(Ordering::Relaxed)
                .min(self.adaptive.load(Ordering::Relaxed))
                .max(1);
            let used = self.used.load(Ordering::SeqCst);
            if now >= until
                && used < limit
                && self
                    .used
                    .compare_exchange(used, used + 1, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
            {
                return Ok(Permit(self.clone()));
            }
            tokio::select! {_=cancel.cancelled()=>bail!("已暂停"),_=tokio::time::sleep(Duration::from_millis(75))=>{}}
        }
    }
    pub fn throttle(&self, seconds: u64) {
        self.adaptive.store(
            (self.adaptive.load(Ordering::Relaxed) / 2).max(1),
            Ordering::Relaxed,
        );
        self.cool_until.fetch_max(
            crate::model::now() + seconds.clamp(2, 300),
            Ordering::Relaxed,
        );
    }
}
pub struct Rate {
    pub kib: AtomicU64,
    next: Mutex<Instant>,
}
impl Rate {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            kib: AtomicU64::new(0),
            next: Mutex::new(Instant::now()),
        })
    }
    async fn wait(&self, bytes: usize, c: &CancellationToken) -> Result<()> {
        let rate = self.kib.load(Ordering::Relaxed);
        if rate == 0 {
            return Ok(());
        }
        let wait = {
            let mut next = self.next.lock().unwrap();
            let start = (*next).max(Instant::now());
            *next = start + Duration::from_secs_f64(bytes as f64 / (rate as f64 * 1024.));
            start
        };
        tokio::select! {_=c.cancelled()=>bail!("已暂停"),_=tokio::time::sleep_until(wait.into())=>Ok(())}
    }
}
#[derive(Clone)]
pub struct Downloader {
    pub client: Client,
    pub gate: Arc<Gate>,
    pub rate: Arc<Rate>,
}
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
struct Identity {
    id: String,
    total: u64,
    etag: Option<String>,
    modified: Option<String>,
    ranged: bool,
}
#[derive(Serialize, Deserialize)]
struct Checkpoint {
    size: u64,
    sha256: String,
}
#[derive(Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
}
impl Downloader {
    pub fn new(max: usize) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(12))
                .redirect(reqwest::redirect::Policy::limited(5))
                .user_agent("Mozilla/5.0 BilibiliVideoDownloader/0.1")
                .build()?,
            gate: Gate::new(max),
            rate: Rate::new(),
        })
    }
    fn request(&self, s: &Stream) -> Result<reqwest::RequestBuilder> {
        let url = url::Url::parse(&s.url)?;
        ensure!(
            ["https", "http"].contains(&url.scheme()),
            "不支持的下载协议"
        );
        let mut r = self
            .client
            .get(&s.url)
            .header("Accept-Encoding", "identity");
        for (k, v) in &s.headers {
            if ![
                "cookie",
                "authorization",
                "host",
                "range",
                "accept-encoding",
            ]
            .contains(&k.to_ascii_lowercase().as_str())
            {
                r = r.header(k, v);
            }
        }
        Ok(r)
    }
    async fn response(
        &self,
        r: reqwest::RequestBuilder,
        c: &CancellationToken,
    ) -> Result<Response> {
        let resp = tokio::select! {_=c.cancelled()=>bail!("已暂停"),r=tokio::time::timeout(Duration::from_secs(30),r.send())=>r.map_err(|_|anyhow!("连接超时"))??};
        if resp.status() == StatusCode::TOO_MANY_REQUESTS {
            let wait = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(15);
            self.gate.throttle(wait);
            bail!("HTTP 429：服务器限流，正在降低并发");
        }
        if resp.status() == StatusCode::FORBIDDEN || resp.status() == StatusCode::UNAUTHORIZED {
            bail!("HTTP {}：下载地址需要刷新", resp.status().as_u16());
        }
        ensure!(
            resp.status().is_success(),
            "HTTP {}",
            resp.status().as_u16()
        );
        Ok(resp)
    }
    async fn probe(&self, s: &Stream, c: &CancellationToken) -> Result<Identity> {
        let _permit = self.gate.acquire(c).await?;
        let r = self
            .response(self.request(s)?.header("Range", "bytes=0-0"), c)
            .await?;
        let ranged = r.status() == StatusCode::PARTIAL_CONTENT;
        let total = if ranged {
            let (start, end, total) = parse_range(
                r.headers()
                    .get("content-range")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or(""),
            )?;
            ensure!(start == 0 && end == 0, "服务器返回了错误的探测范围");
            total
        } else {
            r.content_length().unwrap_or(0)
        };
        let h = |name: &str| {
            r.headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        let etag = h("etag").filter(|v| !v.starts_with("W/"));
        Ok(Identity {
            id: s.id.clone(),
            total,
            etag,
            modified: h("last-modified"),
            ranged,
        })
    }
    pub async fn fetch(
        &self,
        s: &Stream,
        dir: &Path,
        connections: usize,
        retries: usize,
        p: Arc<Progress>,
        cancel: &CancellationToken,
    ) -> Result<PathBuf> {
        tokio::fs::create_dir_all(dir).await?;
        let mut attempt = 0;
        let identity = loop {
            match self.probe(s, cancel).await {
                Ok(identity) => break identity,
                Err(e) => {
                    let message = e.to_string();
                    if cancel.is_cancelled()
                        || attempt >= retries
                        || ["401", "403", "404", "范围", "Content-Range"]
                            .iter()
                            .any(|s| message.contains(s))
                    {
                        return Err(e);
                    }
                    let delay = Duration::from_millis((1u64 << attempt.min(5)) * 500);
                    attempt += 1;
                    tokio::select! { _=cancel.cancelled()=>bail!("已暂停"), _=tokio::time::sleep(delay)=>{} }
                }
            }
        };
        let old = tokio::fs::read(dir.join("identity.json"))
            .await
            .ok()
            .and_then(|v| serde_json::from_slice::<Identity>(&v).ok());
        let verified = old.as_ref() == Some(&identity)
            && (identity.etag.is_some() || identity.modified.is_some());
        let count = if identity.ranged && identity.total > 0 {
            identity.total.div_ceil(CHUNK)
        } else {
            1
        };
        // A new remote representation must never inherit old chunks, even at the same URL.
        if !verified {
            let mut entries = tokio::fs::read_dir(dir).await?;
            while let Some(e) = entries.next_entry().await? {
                if e.file_type().await?.is_file() {
                    tokio::fs::remove_file(e.path()).await?;
                }
            }
        }
        atomic_json(&dir.join("identity.json"), &identity).await?;
        p.total.fetch_add(identity.total, Ordering::Relaxed);
        stream::iter(0..count).map(|index|{let id=identity.clone();let p=p.clone();async move {
   let start=index*CHUNK;let end=if id.total>0{if id.ranged{((index+1)*CHUNK).min(id.total)-1}else{id.total-1}}else{0};let size=if id.total>0{end-start+1}else{0};let file=dir.join(format!("{index}.part"));let checkpoint=dir.join(format!("{index}.json"));
   if verified&&valid_chunk(&file,&checkpoint,size).await? {p.done.fetch_add(size,Ordering::Relaxed);return Ok::<(),anyhow::Error>(());}
   for attempt in 0..=retries {
    let mut received=0u64;
    let result=self.chunk(s,&id,start,end,&file,cancel,&p,&mut received).await;
    match result {Ok(hash)=>{atomic_json(&checkpoint,&Checkpoint{size:received,sha256:hash}).await?;return Ok(());},Err(e)=>{p.done.fetch_sub(received,Ordering::Relaxed);let text=e.to_string();if cancel.is_cancelled()||attempt==retries||text.contains("403")||text.contains("401")||text.contains("404")||text.contains("范围")||text.contains("资源变化"){return Err(e);}let delay=(1u64<<attempt.min(5))*500+(crate::model::now()%7)*61;tokio::select!{_=cancel.cancelled()=>bail!("已暂停"),_=tokio::time::sleep(Duration::from_millis(delay))=>{}}}}
   }bail!("重试失败")
  }}).buffer_unordered(connections.clamp(1,8)).try_collect::<Vec<_>>().await?;
        let output = dir.join("complete.media");
        let mut merged = tokio::fs::File::create(&output).await?;
        for i in 0..count {
            ensure!(!cancel.is_cancelled(), "已暂停");
            let mut part = tokio::fs::File::open(dir.join(format!("{i}.part"))).await?;
            tokio::io::copy(&mut part, &mut merged).await?;
        }
        merged.sync_all().await?;
        if identity.total > 0 {
            ensure!(
                merged.metadata().await?.len() == identity.total,
                "完整性检查失败：文件长度不匹配"
            );
        }
        Ok(output)
    }
    async fn chunk(
        &self,
        s: &Stream,
        id: &Identity,
        start: u64,
        end: u64,
        path: &Path,
        c: &CancellationToken,
        p: &Progress,
        received: &mut u64,
    ) -> Result<String> {
        let _permit = self.gate.acquire(c).await?;
        let mut request = self.request(s)?;
        if id.ranged {
            request = request.header("Range", format!("bytes={start}-{end}"));
            if let Some(tag) = id.etag.as_ref().or(id.modified.as_ref()) {
                request = request.header("If-Range", tag);
            }
        }
        let response = self.response(request, c).await?;
        if id.ranged {
            ensure!(
                response.status() == StatusCode::PARTIAL_CONTENT,
                "服务器忽略分段范围，请重试以重新探测资源"
            );
            let range = parse_range(
                response
                    .headers()
                    .get("content-range")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or(""),
            )?;
            ensure!(range == (start, end, id.total), "服务器分段范围不一致");
        }
        if let Some(expected) = &id.etag {
            if let Some(actual) = response.headers().get("etag") {
                ensure!(actual.to_str()? == expected, "远端资源变化");
            }
        }
        let mut stream = response.bytes_stream();
        let mut file = tokio::fs::File::create(path).await?;
        let mut hash = Sha256::new();
        loop {
            let next = tokio::select! {_=c.cancelled()=>bail!("已暂停"),v=tokio::time::timeout(Duration::from_secs(30),stream.next())=>v.map_err(|_|anyhow!("下载读取超时"))?};
            let Some(bytes) = next else { break };
            let bytes = bytes?;
            if id.total > 0 {
                ensure!(
                    *received + bytes.len() as u64 <= end - start + 1,
                    "服务器分段长度超出范围"
                );
            }
            self.rate.wait(bytes.len(), c).await?;
            file.write_all(&bytes).await?;
            hash.update(&bytes);
            *received += bytes.len() as u64;
            p.done.fetch_add(bytes.len() as u64, Ordering::Relaxed);
        }
        if id.total > 0 {
            ensure!(*received == end - start + 1, "下载提前结束，正在重试");
        }
        ensure!(*received > 0, "服务器返回空文件");
        file.sync_all().await?;
        Ok(hex::encode(hash.finalize()))
    }
}
pub fn parse_range(s: &str) -> Result<(u64, u64, u64)> {
    let rest = s
        .strip_prefix("bytes ")
        .ok_or_else(|| anyhow!("无效 Content-Range 范围"))?;
    let (range, total) = rest
        .split_once('/')
        .ok_or_else(|| anyhow!("无效下载范围"))?;
    let (start, end) = range
        .split_once('-')
        .ok_or_else(|| anyhow!("无效下载范围"))?;
    let (start, end, total) = (
        start.parse::<u64>()?,
        end.parse::<u64>()?,
        total.parse::<u64>()?,
    );
    ensure!(start <= end && end < total, "无效下载范围");
    Ok((start, end, total))
}
async fn valid_chunk(path: &Path, checkpoint: &Path, size: u64) -> Result<bool> {
    let Ok(bytes) = tokio::fs::read(checkpoint).await else {
        return Ok(false);
    };
    let Ok(meta) = serde_json::from_slice::<Checkpoint>(&bytes) else {
        return Ok(false);
    };
    let Ok(mut file) = tokio::fs::File::open(path).await else {
        return Ok(false);
    };
    if meta.size != size || file.metadata().await?.len() != size {
        return Ok(false);
    }
    let mut hash = Sha256::new();
    let mut buf = vec![0; 65536];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(hex::encode(hash.finalize()) == meta.sha256)
}

pub async fn file_digest(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hash = Sha256::new();
    let mut buf = vec![0; 1024 * 1024];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(hex::encode(hash.finalize()))
}
