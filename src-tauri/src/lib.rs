pub mod download;
pub mod engine;
pub mod media;
pub mod model;
pub mod process;
pub mod provider;
pub mod store;

use engine::Engine;
use model::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tauri::{Manager, State};
use tauri_plugin_opener::OpenerExt;
type AppState<'a> = State<'a, Arc<Engine>>;
fn error(e: impl std::fmt::Display) -> String {
    process::redact(&e.to_string())
}
#[tauri::command]
fn snapshot(state: AppState<'_>) -> engine::Snapshot {
    state.snapshot()
}
#[tauri::command]
fn save_settings(settings: Settings, state: AppState<'_>) -> Result<(), String> {
    state.save_settings(settings).map_err(error)
}
#[tauri::command]
fn enqueue(items: Vec<MediaItem>, options: Options, state: AppState<'_>) -> Result<usize, String> {
    state.enqueue(items, options).map_err(error)
}
#[tauri::command]
fn task_action(
    id: Option<String>,
    ids: Option<Vec<String>>,
    action: String,
    state: AppState<'_>,
) -> Result<(), String> {
    if ids.as_ref().map(|v| v.len() > 5000).unwrap_or(false) {
        return Err("单次最多操作 5000 项".into());
    }
    if id.as_deref() == Some("*") || ids.is_some() {
        let selected = ids.map(|v| v.into_iter().collect::<std::collections::HashSet<_>>());
        let ids: Vec<_> = state
            .tasks
            .lock()
            .unwrap()
            .iter()
            .filter(|t| {
                if selected
                    .as_ref()
                    .map(|s| !s.contains(&t.id))
                    .unwrap_or(false)
                {
                    return false;
                }
                if action == "pause" {
                    [
                        "queued",
                        "resolving",
                        "downloading",
                        "retrying",
                        "converting",
                    ]
                    .contains(&t.status.as_str())
                } else if action == "retry" {
                    t.status == "error"
                } else {
                    t.status == "paused"
                }
            })
            .map(|t| t.id.clone())
            .collect();
        for id in ids {
            state.action(&id, &action).map_err(error)?;
        }
        Ok(())
    } else {
        state
            .action(&id.ok_or_else(|| "请选择任务".to_owned())?, &action)
            .map_err(error)
    }
}
#[tauri::command]
fn parse_inputs(input: String, state: AppState<'_>) -> Result<(), String> {
    state.inner().parse_inputs(input).map_err(error)
}
#[tauri::command]
fn cancel_parse(state: AppState<'_>) {
    state.parse_cancel.lock().unwrap().cancel();
}
#[tauri::command]
fn clear_parse(state: AppState<'_>) -> Result<(), String> {
    let mut p = state.parse.lock().unwrap();
    if p.running {
        return Err("请先停止解析".into());
    }
    *p = ParseState::default();
    state.store.save_parse(&p).map_err(error)
}
#[tauri::command]
async fn account(state: AppState<'_>) -> Result<Account, String> {
    state.provider.account().await.map_err(error)
}
#[tauri::command]
fn cached_account(state: AppState<'_>) -> Option<Account> {
    state.provider.cached_account()
}
#[tauri::command]
async fn import_cookie(cookie: String, state: AppState<'_>) -> Result<Account, String> {
    state.provider.import_cookie(&cookie).await.map_err(error)
}
#[tauri::command]
fn logout(state: AppState<'_>) -> Result<(), String> {
    state.provider.logout().map_err(error)
}
#[tauri::command]
async fn qr_login(state: AppState<'_>) -> Result<serde_json::Value, String> {
    state.provider.qr().await.map_err(error)
}
#[tauri::command]
async fn poll_qr(key: String, state: AppState<'_>) -> Result<serde_json::Value, String> {
    state.provider.poll_qr(&key).await.map_err(error)
}
#[tauri::command]
async fn read_text(path: String) -> Result<String, String> {
    let p = Path::new(&path);
    if !["txt", "json", "cookies"].contains(&p.extension().and_then(|s| s.to_str()).unwrap_or("")) {
        return Err("仅支持 TXT、JSON 和 cookies 文件".into());
    }
    let meta = tokio::fs::metadata(p).await.map_err(error)?;
    if meta.len() > 2 * 1024 * 1024 {
        return Err("导入文件不能超过 2 MB".into());
    }
    tokio::fs::read_to_string(p).await.map_err(error)
}
#[tauri::command]
fn open_output(
    id: String,
    folder: bool,
    state: AppState<'_>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let tasks = state.tasks.lock().unwrap();
    let task = tasks.iter().find(|t| t.id == id).ok_or("任务不存在")?;
    let p = PathBuf::from(&task.output);
    let path = if folder {
        p.parent().ok_or("路径无效")?
    } else {
        p.as_path()
    };
    if !path.exists() {
        return Err("文件尚未生成或已被移动".into());
    }
    app.opener()
        .open_path(path.to_string_lossy().as_ref(), None::<&str>)
        .map_err(error)
}
#[tauri::command]
async fn export_logs(path: String, state: AppState<'_>) -> Result<(), String> {
    let snapshot = state.snapshot();
    let bytes = serde_json::to_vec_pretty(&snapshot).map_err(error)?;
    tokio::fs::write(path, bytes).await.map_err(error)
}
#[tauri::command]
async fn clear_cache(id: String, state: AppState<'_>) -> Result<(), String> {
    if state.active.lock().unwrap().contains_key(&id) {
        return Err("请先暂停任务".into());
    }
    if !state
        .tasks
        .lock()
        .unwrap()
        .iter()
        .any(|t| t.id == id && t.status == "completed")
    {
        return Err("仅可清理已完成任务的缓存".into());
    }
    let path = state.data.join("cache").join(&id);
    if path.exists() {
        tokio::fs::remove_dir_all(path).await.map_err(error)?;
    }
    Ok(())
}
#[tauri::command]
fn reprocess(id: String, options: Options, state: AppState<'_>) -> Result<(), String> {
    options.validate().map_err(error)?;
    if state.active.lock().unwrap().contains_key(&id) {
        return Err("请先暂停任务".into());
    }
    let mut tasks = state.tasks.lock().unwrap();
    let t = tasks.iter_mut().find(|t| t.id == id).ok_or("任务不存在")?;
    if options.quality != t.options.quality || options.audio() != t.options.audio() {
        let _ = std::fs::remove_file(state.data.join("cache").join(&id).join("downloaded.json"));
    }
    t.options = options;
    t.status = "queued".into();
    t.error.clear();
    let base = output_base(t);
    t.output = base
        .with_file_name(format!(
            "{} [{}]-{}.{}",
            base.file_name().unwrap_or_default().to_string_lossy(),
            &id[..8],
            now(),
            t.options.format
        ))
        .to_string_lossy()
        .into_owned();
    state.store.save_task(t).map_err(error)
}
#[tauri::command]
async fn preview_frame(
    id: String,
    second: f64,
    watermark: Option<Watermark>,
    clip: bool,
    state: AppState<'_>,
) -> Result<serde_json::Value, String> {
    let task = state
        .tasks
        .lock()
        .unwrap()
        .iter()
        .find(|t| t.id == id)
        .cloned()
        .ok_or("任务不存在")?;
    let source = state.data.join("cache").join(&id).join("0/complete.media");
    let source = if source.exists() {
        source
    } else {
        PathBuf::from(&task.output)
    };
    if !source.exists() {
        return Err("下载完成后可预览水印处理".into());
    }
    if task.options.audio() {
        return Err("纯音频任务没有画面".into());
    }
    let token = tokio_util::sync::CancellationToken::new();
    let info = media::inspect(&state.provider.tools, &source, &token)
        .await
        .map_err(error)?;
    let video = info["streams"]
        .as_array()
        .and_then(|a| a.iter().find(|v| v["codec_type"] == "video"))
        .ok_or("没有视频轨道")?;
    let width = video["width"].as_u64().unwrap_or(0) as u32;
    let height = video["height"].as_u64().unwrap_or(0) as u32;
    let preview_dir = state.data.join("previews");
    tokio::fs::create_dir_all(&preview_dir)
        .await
        .map_err(error)?;
    let dest = preview_dir.join(format!(
        "{}.{}",
        uuid::Uuid::new_v4(),
        if clip { "mp4" } else { "png" }
    ));
    let mut args = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-v".into(),
        "error".into(),
        "-ss".into(),
        second.max(0.).to_string(),
        "-i".into(),
        source.to_string_lossy().into_owned(),
    ];
    if let Some(mut w) = watermark {
        w.start = 0.;
        w.end = 0.;
        if let Some(f) = media::filter(&w, width, height).map_err(error)? {
            args.extend(["-vf".into(), f]);
        }
    }
    if clip {
        args.extend([
            "-t".into(),
            "5".into(),
            "-an".into(),
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            "ultrafast".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
        ]);
    } else {
        args.extend(["-frames:v".into(), "1".into()]);
    }
    args.push(dest.to_string_lossy().into_owned());
    process::capture(
        &state.provider.tools.join("ffmpeg.exe"),
        &args,
        &token,
        Duration::from_secs(120),
    )
    .await
    .map_err(error)?;
    Ok(serde_json::json!({"path":dest,"width":width,"height":height}))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data = app.path().app_data_dir()?;
            let tools = if cfg!(debug_assertions) {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries")
            } else {
                app.path().resource_dir()?.join("tools")
            };
            let engine = Engine::new(data, tools)?;
            engine.downloader.rate.kib.store(
                engine.settings.lock().unwrap().rate_limit_kib,
                std::sync::atomic::Ordering::Relaxed,
            );
            app.manage(engine.clone());
            engine.start();
            let show =
                tauri::menu::MenuItem::with_id(app, "show", "打开下载器", true, None::<&str>)?;
            let quit = tauri::menu::MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(app, &[&show, &quit])?;
            tauri::tray::TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("BilibiliVideoDownloader")
                .menu(&menu)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => {
                        app.state::<Arc<Engine>>().stop();
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let engine = window.state::<Arc<Engine>>();
                if engine.settings.lock().unwrap().tray {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    engine.stop();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            save_settings,
            enqueue,
            task_action,
            parse_inputs,
            cancel_parse,
            clear_parse,
            account,
            cached_account,
            import_cookie,
            logout,
            qr_login,
            poll_qr,
            read_text,
            open_output,
            export_logs,
            clear_cache,
            reprocess,
            preview_frame
        ])
        .run(tauri::generate_context!())
        .expect("启动下载器失败");
}
