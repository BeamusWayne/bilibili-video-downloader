use crate::{
    model::*,
    process::{capture, command},
};
use anyhow::{ensure, Result};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio_util::sync::CancellationToken;

pub fn filter(w: &Watermark, width: u32, height: u32) -> Result<Option<String>> {
    if w.mode.is_empty() || w.mode == "none" {
        return Ok(None);
    }
    ensure!(w.width > 0 && w.height > 0, "请选择水印区域");
    if width > 0 && height > 0 {
        ensure!(
            w.x.saturating_add(w.width) <= width && w.y.saturating_add(w.height) <= height,
            "处理区域超出画面"
        );
    }
    let time = if w.end > w.start {
        format!(",enable='between(t,{},{})'", w.start, w.end)
    } else if w.start > 0. {
        format!(",enable='gte(t,{})'", w.start)
    } else {
        String::new()
    };
    let filter=match w.mode.as_str(){"delogo"=>{ensure!(w.x>0&&w.y>0,"修补区域需离画面边缘至少 1 像素");format!("delogo=x={}:y={}:w={}:h={}{}",w.x,w.y,w.width,w.height,time.replacen(",enable",":enable",1))},"blur"=>format!("split[base][region];[region]crop={}:{}:{}:{},gblur=sigma=10[blur];[base][blur]overlay={}:{}{}",w.width,w.height,w.x,w.y,w.x,w.y,time.replacen(",enable",":enable",1)),"crop"=>format!("crop={}:{}:{}:{}",w.width,w.height,w.x,w.y),_=>anyhow::bail!("未知处理方式")};
    Ok(Some(filter))
}
pub async fn inspect(tools: &Path, path: &Path, c: &CancellationToken) -> Result<Value> {
    let args = vec![
        "-v".into(),
        "error".into(),
        "-show_format".into(),
        "-show_streams".into(),
        "-of".into(),
        "json".into(),
        path.to_string_lossy().into_owned(),
    ];
    Ok(serde_json::from_slice(
        &capture(
            &tools.join("ffprobe.exe"),
            &args,
            c,
            Duration::from_secs(45),
        )
        .await?,
    )?)
}
pub async fn verify(tools: &Path, path: &Path, audio: bool, c: &CancellationToken) -> Result<()> {
    let info = inspect(tools, path, c).await?;
    let streams = info["streams"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("媒体检查失败"))?;
    ensure!(
        streams
            .iter()
            .any(|s| s["codec_type"] == if audio { "audio" } else { "video" }),
        "媒体检查失败：缺少预期音视频轨道"
    );
    let duration = info["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.);
    ensure!(duration > 0., "媒体检查失败：时长无效");
    Ok(())
}
pub async fn convert(
    tools: &Path,
    task: &Task,
    resolved: &Resolved,
    inputs: &[PathBuf],
    output: &Path,
    cancel: &CancellationToken,
    on_progress: Arc<dyn Fn(f64) + Send + Sync>,
) -> Result<()> {
    let parent = output.parent().unwrap();
    tokio::fs::create_dir_all(parent).await?;
    let temp = parent.join(format!(".{}.processing.{}", task.id, task.options.format));
    let mut args = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-loglevel".into(),
        "error".into(),
        "-progress".into(),
        "pipe:1".into(),
    ];
    let video: Vec<_> = inputs
        .iter()
        .zip(&resolved.streams)
        .filter(|(_, s)| s.kind == "video")
        .map(|(p, _)| p)
        .collect();
    let audio: Vec<_> = inputs
        .iter()
        .zip(&resolved.streams)
        .filter(|(_, s)| s.kind == "audio")
        .map(|(p, _)| p)
        .collect();
    let mut used = 0;
    for group in [&video, &audio] {
        if group.is_empty() {
            continue;
        }
        if group.len() > 1 {
            let list = parent.join(format!(".{}-{used}.concat.txt", task.id));
            let content = group
                .iter()
                .map(|p| {
                    format!(
                        "file '{}'\n",
                        p.to_string_lossy()
                            .replace('\\', "/")
                            .replace('\'', "'\\''")
                    )
                })
                .collect::<String>();
            tokio::fs::write(&list, content).await?;
            args.extend([
                "-f".into(),
                "concat".into(),
                "-safe".into(),
                "0".into(),
                "-i".into(),
                list.to_string_lossy().into_owned(),
            ]);
        } else {
            args.extend(["-i".into(), group[0].to_string_lossy().into_owned()]);
        }
        used += 1;
    }
    ensure!(used > 0, "没有可处理的源文件");
    let opt = &task.options;
    if opt.audio() {
        args.extend([
            "-map".into(),
            format!(
                "{}:a:0",
                if !video.is_empty() && !audio.is_empty() {
                    1
                } else {
                    0
                }
            ),
            "-vn".into(),
        ]);
        let codec = match opt.format.as_str() {
            "mp3" => "libmp3lame",
            "m4a" => "aac",
            "flac" => "flac",
            _ => "pcm_s16le",
        };
        args.extend(["-c:a".into(), codec.into()]);
        if ["mp3", "m4a"].contains(&opt.format.as_str()) {
            args.extend(["-b:a".into(), format!("{}k", opt.bitrate)]);
        }
    } else {
        args.extend([
            "-map".into(),
            "0:v:0".into(),
            "-map".into(),
            format!("{}:a:0?", if !audio.is_empty() { 1 } else { 0 }),
        ]);
        let effect = filter(&opt.watermark, resolved.width, resolved.height)?;
        if opt.compatibility || effect.is_some() {
            args.extend([
                "-c:v".into(),
                "libx264".into(),
                "-preset".into(),
                "medium".into(),
                "-crf".into(),
                "20".into(),
                "-pix_fmt".into(),
                "yuv420p".into(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                "192k".into(),
            ]);
            if let Some(effect) = effect {
                args.extend(["-vf".into(), effect]);
            }
        } else {
            args.extend(["-c".into(), "copy".into()]);
        }
        if opt.format == "mp4" {
            args.extend(["-movflags".into(), "+faststart".into()]);
        }
    }
    args.extend([
        "-metadata".into(),
        format!("title={}", task.item.title),
        temp.to_string_lossy().into_owned(),
    ]);
    let mut child = command(&tools.join("ffmpeg.exe"))
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let stderr = child.stderr.take().unwrap();
    let err_task = tokio::spawn(async move {
        let mut text = String::new();
        let _ = BufReader::new(stderr)
            .take(64 * 1024)
            .read_to_string(&mut text)
            .await;
        text
    });
    loop {
        tokio::select! {_=cancel.cancelled()=>{let _=child.kill().await;let _=child.wait().await;err_task.abort();anyhow::bail!("已暂停");},line=lines.next_line()=>{match line?{Some(l)=>{if let Some(v)=l.strip_prefix("out_time_us=").and_then(|s|s.parse::<f64>().ok()){if resolved.duration>0.{on_progress((v/1_000_000./resolved.duration*100.).clamp(0.,99.));}}},None=>break}}}
    }
    let status = child.wait().await?;
    let error = err_task.await.unwrap_or_default();
    ensure!(
        status.success(),
        "转换失败：{}",
        crate::process::redact(&error)
    );
    verify(tools, &temp, opt.audio(), cancel).await?;
    // Final paths are reserved per task. A completed file is never overwritten.
    if output.exists() {
        verify(tools, output, opt.audio(), cancel).await?;
        tokio::fs::remove_file(&temp).await?;
    } else {
        tokio::fs::rename(&temp, output).await?;
    }
    for i in 0..used {
        let _ = tokio::fs::remove_file(parent.join(format!(".{}-{i}.concat.txt", task.id))).await;
    }
    Ok(())
}
pub fn srt_from_bili(data: &[u8]) -> Result<String> {
    let v: Value = serde_json::from_slice(data)?;
    let body = v["body"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("无效字幕"))?;
    let ts = |t: f64| {
        let ms = (t * 1000.).round() as u64;
        format!(
            "{:02}:{:02}:{:02},{:03}",
            ms / 3600000,
            (ms / 60000) % 60,
            (ms / 1000) % 60,
            ms % 1000
        )
    };
    Ok(body
        .iter()
        .enumerate()
        .map(|(i, s)| {
            format!(
                "{}\n{} --> {}\n{}\n\n",
                i + 1,
                ts(s["from"].as_f64().unwrap_or(0.)),
                ts(s["to"].as_f64().unwrap_or(0.)),
                s["content"].as_str().unwrap_or("")
            )
        })
        .collect())
}
