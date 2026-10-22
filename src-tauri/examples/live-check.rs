use bvd::{engine::Engine, model::Options};
use std::{path::PathBuf, time::Duration};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let url = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("https://www.bilibili.com/video/BV1SUnB61E8K/");
    let mode = args.get(2).map(String::as_str).unwrap_or("probe");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dir = root.parent().unwrap().join(".cache/live-check");
    let engine = Engine::new(dir.clone(), root.join("binaries"))?;
    let account = engine.provider.account().await?;
    println!("Login: {}", account.logged_in);
    if mode == "account" {
        return Ok(());
    }
    let token = tokio_util::sync::CancellationToken::new();
    let items = engine.provider.enumerate(url, &token).await?;
    println!("Parsed {} item(s)", items.len());
    if mode == "catalog" {
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }
    let mut options = Options::default();
    options.directory = dir.join("output").to_string_lossy().into_owned();
    options.cover = false;
    options.subtitles = false;
    options.format = if mode == "mp3" { "mp3" } else { "mp4" }.into();
    options.quality = 720;
    if mode == "probe" {
        let r = engine.provider.resolve(&items[0], &options, &token).await?;
        println!(
            "{} · {} seconds · {} streams · {}x{}",
            r.title,
            r.duration,
            r.streams.len(),
            r.width,
            r.height
        );
        return Ok(());
    }
    engine.enqueue(vec![items[0].clone()], options)?;
    let retry_ids: Vec<_> = engine
        .tasks
        .lock()
        .unwrap()
        .iter()
        .filter(|t| t.status == "error" || t.status == "paused")
        .map(|t| t.id.clone())
        .collect();
    for id in retry_ids {
        engine.action(&id, "retry")?;
    }
    engine.start();
    let mut paused_once = false;
    for _ in 0..900 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let tasks = engine.tasks.lock().unwrap().clone();
        let task = tasks.last().unwrap();
        println!("{} {} / {}", task.status, task.downloaded, task.total);
        if mode == "resume" && !paused_once && task.downloaded > 8 * 1024 * 1024 {
            engine.action(&task.id, "pause")?;
            tokio::time::sleep(Duration::from_secs(2)).await;
            engine.action(&task.id, "resume")?;
            paused_once = true;
        }
        if task.status == "completed" {
            println!("Output: {}", task.output);
            engine.stop();
            return Ok(());
        }
        if task.status == "error" {
            anyhow::bail!("{}", task.error);
        }
    }
    engine.stop();
    anyhow::bail!("Test timeout")
}
