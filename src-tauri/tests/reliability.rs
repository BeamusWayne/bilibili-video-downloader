use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::get,
    Router,
};
use bvd::{
    download::{Downloader, Gate, Progress, CHUNK},
    model::*,
    provider::{normalize_cookie, normalize_input},
    store::Store,
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio_util::sync::CancellationToken;
#[derive(Default)]
struct ServerState {
    version: AtomicUsize,
    mode: AtomicUsize,
    probes: AtomicUsize,
    requests: Mutex<HashMap<u64, usize>>,
}
const SIZE: u64 = CHUNK * 2 + 197;
fn data(start: u64, end: u64, version: usize) -> Vec<u8> {
    (start..=end)
        .map(|i| ((i * 7 + version as u64) % 251) as u8)
        .collect()
}
async fn serve(State(state): State<Arc<ServerState>>, headers: HeaderMap) -> Response {
    let mode = state.mode.load(Ordering::Relaxed);
    let version = state.version.load(Ordering::Relaxed);
    if mode == 4 {
        return Response::builder().status(403).body(Body::empty()).unwrap();
    }
    let range = headers
        .get("range")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("bytes="))
        .and_then(|s| s.split_once('-'))
        .map(|(a, b)| (a.parse::<u64>().unwrap(), b.parse::<u64>().unwrap()));
    let (start, end) = if mode == 1 {
        (0, SIZE - 1)
    } else {
        range.unwrap_or((0, SIZE - 1))
    };
    let probe = start == 0 && end == 0;
    if probe && state.probes.fetch_add(1, Ordering::Relaxed) == 0 && (mode == 6 || mode == 7) {
        return Response::builder()
            .status(if mode == 6 { 503 } else { 429 })
            .header("retry-after", "2")
            .body(Body::empty())
            .unwrap();
    }
    if !probe {
        *state.requests.lock().unwrap().entry(start).or_default() += 1;
    }
    let mut response = Response::builder()
        .status(if range.is_some() && mode != 1 {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header("content-length", end - start + 1);
    if mode != 5 {
        response = response.header("etag", format!("\"resource-{version}\""));
    }
    if range.is_some() && mode != 1 {
        response = response.header(
            "content-range",
            format!(
                "bytes {}-{end}/{SIZE}",
                if mode == 2 && !probe {
                    start + 1
                } else {
                    start
                }
            ),
        );
    }
    let fail = mode == 3 && !probe && state.requests.lock().unwrap().get(&start) == Some(&1);
    let stream = futures_util::stream::unfold((start, false), move |(pos, failed)| async move {
        if failed || pos > end {
            return None;
        }
        if fail && pos > start {
            return Some((
                Err(std::io::Error::other("simulated disconnect")),
                (pos, true),
            ));
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
        let last = (pos + 65535).min(end);
        Some((
            Ok::<_, std::io::Error>(Bytes::from(data(pos, last, version))),
            (last + 1, false),
        ))
    });
    response.body(Body::from_stream(stream)).unwrap()
}
async fn setup() -> (Stream, Arc<ServerState>, tokio::task::JoinHandle<()>) {
    let state = Arc::new(ServerState::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/file", get(serve))
        .with_state(state.clone());
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (
        Stream {
            id: "video-v1".into(),
            url: format!("http://{address}/file"),
            headers: HashMap::new(),
            kind: "video".into(),
            size: SIZE,
        },
        state,
        handle,
    )
}

#[tokio::test]
async fn probe_recovers_from_server_failure_and_rate_limit() {
    for mode in [6, 7] {
        let (s, state, server) = setup().await;
        state.mode.store(mode, Ordering::Relaxed);
        let dir = tempfile::tempdir().unwrap();
        let dl = Downloader::new(4).unwrap();
        let out = dl
            .fetch(
                &s,
                dir.path(),
                4,
                2,
                Arc::new(Progress::default()),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(tokio::fs::read(out).await.unwrap(), data(0, SIZE - 1, 0));
        assert_eq!(state.probes.load(Ordering::Relaxed), 2);
        if mode == 7 {
            assert!(dl.gate.adaptive.load(Ordering::Relaxed) <= 2);
        }
        server.abort();
    }
}

#[tokio::test]
async fn resume_reuses_verified_chunks_and_detects_corruption() {
    let (s, state, server) = setup().await;
    let dir = tempfile::tempdir().unwrap();
    let dl = Downloader::new(3).unwrap();
    let token = CancellationToken::new();
    let p = Arc::new(Progress::default());
    let stopper = token.clone();
    let progress = p.clone();
    tokio::spawn(async move {
        while progress.done.load(Ordering::Relaxed) < CHUNK + 65536 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        stopper.cancel();
    });
    assert!(dl.fetch(&s, dir.path(), 1, 1, p, &token).await.is_err());
    assert!(dir.path().join("0.json").exists());
    let out = dl
        .fetch(
            &s,
            dir.path(),
            2,
            1,
            Arc::new(Progress::default()),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(tokio::fs::read(&out).await.unwrap(), data(0, SIZE - 1, 0));
    assert_eq!(state.requests.lock().unwrap().get(&0), Some(&1));
    tokio::fs::write(dir.path().join("0.part"), vec![1; CHUNK as usize])
        .await
        .unwrap();
    dl.fetch(
        &s,
        dir.path(),
        2,
        1,
        Arc::new(Progress::default()),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(state.requests.lock().unwrap().get(&0), Some(&2));
    server.abort();
}
#[tokio::test]
async fn changed_resource_is_not_spliced() {
    let (s, state, server) = setup().await;
    let dir = tempfile::tempdir().unwrap();
    let dl = Downloader::new(4).unwrap();
    dl.fetch(
        &s,
        dir.path(),
        3,
        1,
        Arc::new(Progress::default()),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    state.version.store(1, Ordering::Relaxed);
    let out = dl
        .fetch(
            &s,
            dir.path(),
            3,
            1,
            Arc::new(Progress::default()),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(tokio::fs::read(out).await.unwrap(), data(0, SIZE - 1, 1));
    assert_eq!(state.requests.lock().unwrap().get(&0), Some(&2));
    server.abort();
}
#[tokio::test]
async fn non_range_server_uses_single_stream() {
    let (s, state, server) = setup().await;
    state.mode.store(1, Ordering::Relaxed);
    let dir = tempfile::tempdir().unwrap();
    let out = Downloader::new(8)
        .unwrap()
        .fetch(
            &s,
            dir.path(),
            8,
            0,
            Arc::new(Progress::default()),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(tokio::fs::read(out).await.unwrap(), data(0, SIZE - 1, 0));
    server.abort();
}
#[tokio::test]
async fn invalid_range_is_rejected() {
    let (s, state, server) = setup().await;
    state.mode.store(2, Ordering::Relaxed);
    let dir = tempfile::tempdir().unwrap();
    assert!(Downloader::new(3)
        .unwrap()
        .fetch(
            &s,
            dir.path(),
            3,
            3,
            Arc::new(Progress::default()),
            &CancellationToken::new()
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("范围"));
    assert!(!dir.path().join("complete.media").exists());
    server.abort();
}
#[tokio::test]
async fn disconnected_segments_retry_without_corruption() {
    let (s, state, server) = setup().await;
    state.mode.store(3, Ordering::Relaxed);
    let dir = tempfile::tempdir().unwrap();
    let out = Downloader::new(3)
        .unwrap()
        .fetch(
            &s,
            dir.path(),
            3,
            2,
            Arc::new(Progress::default()),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(tokio::fs::read(out).await.unwrap(), data(0, SIZE - 1, 0));
    assert_eq!(state.requests.lock().unwrap().get(&0), Some(&2));
    assert_eq!(state.requests.lock().unwrap().get(&CHUNK), Some(&2));
    assert_eq!(state.requests.lock().unwrap().get(&(CHUNK * 2)), Some(&1));
    server.abort();
}
#[tokio::test]
async fn forbidden_is_returned_for_url_refresh() {
    let (s, state, server) = setup().await;
    state.mode.store(4, Ordering::Relaxed);
    let dir = tempfile::tempdir().unwrap();
    assert!(Downloader::new(3)
        .unwrap()
        .fetch(
            &s,
            dir.path(),
            3,
            5,
            Arc::new(Progress::default()),
            &CancellationToken::new()
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("403"));
    server.abort();
}
#[tokio::test]
async fn missing_validators_force_safe_restart() {
    let (s, state, server) = setup().await;
    state.mode.store(5, Ordering::Relaxed);
    let dir = tempfile::tempdir().unwrap();
    let dl = Downloader::new(3).unwrap();
    for _ in 0..2 {
        dl.fetch(
            &s,
            dir.path(),
            3,
            0,
            Arc::new(Progress::default()),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    }
    assert_eq!(state.requests.lock().unwrap().get(&0), Some(&2));
    server.abort();
}
#[tokio::test]
async fn five_hundred_workers_obey_global_connection_limit() {
    let gate = Gate::new(7);
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let mut workers = vec![];
    for _ in 0..500 {
        let g = gate.clone();
        let a = active.clone();
        let p = peak.clone();
        workers.push(tokio::spawn(async move {
            let _permit = g.acquire(&CancellationToken::new()).await.unwrap();
            let count = a.fetch_add(1, Ordering::SeqCst) + 1;
            p.fetch_max(count, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(1)).await;
            a.fetch_sub(1, Ordering::SeqCst);
        }));
    }
    for w in workers {
        w.await.unwrap();
    }
    assert!(peak.load(Ordering::SeqCst) <= 7);
    assert!(peak.load(Ordering::SeqCst) > 1);
}
#[test]
fn five_hundred_tasks_survive_database_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.db");
    {
        let db = Store::open(&path).unwrap();
        for i in 0..500 {
            db.save_task(&Task::new(
                MediaItem {
                    id: i.to_string(),
                    url: format!("https://www.bilibili.com/video/BV{i}"),
                    title: format!("视频 {i}"),
                    album_title: String::new(),
                    uploader: "用户".into(),
                    thumbnail: String::new(),
                    duration: 1.,
                    index: i,
                },
                Options::default(),
            ))
            .unwrap();
        }
    }
    let reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.tasks().unwrap().len(), 500);
}
#[test]
fn paths_and_input_are_constrained() {
    for name in ["CON", "NUL.txt", "../../a", "a:b?c", "LPT1", ".."] {
        let safe = safe_name(name);
        assert!(!safe.contains(['/', '\\', ':']));
        assert_ne!(safe, "..");
        assert_ne!(safe, "CON");
    }
    assert!(normalize_input("https://www.bilibili.com.evil.example/video/BV1").is_err());
    assert!(normalize_input("file:///C:/secret").is_err());
    assert!(normalize_input("BV1SUnB61E8K")
        .unwrap()
        .starts_with("https://www.bilibili.com/"));
    assert!(normalize_cookie("SESSDATA=abc\r\nInjected: x").is_err());
    assert_eq!(
        normalize_cookie("Cookie: SESSDATA=abc; bili_jct=def").unwrap(),
        "SESSDATA=abc; bili_jct=def"
    );
}

#[tokio::test]
async fn isolated_process_download_worker() {
    let Ok(url) = std::env::var("BVD_TEST_URL") else {
        return;
    };
    let dir = std::path::PathBuf::from(std::env::var("BVD_TEST_DIR").unwrap());
    let stream = Stream {
        id: "video-v1".into(),
        url,
        headers: HashMap::new(),
        kind: "video".into(),
        size: SIZE,
    };
    Downloader::new(1)
        .unwrap()
        .fetch(
            &stream,
            &dir,
            1,
            0,
            Arc::new(Progress::default()),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
}
#[tokio::test]
async fn force_killed_process_recovers_confirmed_chunks() {
    let (s, state, server) = setup().await;
    let dir = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "isolated_process_download_worker", "--nocapture"])
        .env("BVD_TEST_URL", &s.url)
        .env("BVD_TEST_DIR", dir.path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = command.spawn().unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        while !dir.path().join("0.json").exists() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    child.kill().await.unwrap();
    let _ = child.wait().await;
    let out = Downloader::new(3)
        .unwrap()
        .fetch(
            &s,
            dir.path(),
            2,
            0,
            Arc::new(Progress::default()),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(tokio::fs::read(out).await.unwrap(), data(0, SIZE - 1, 0));
    assert_eq!(state.requests.lock().unwrap().get(&0), Some(&1));
    server.abort();
}
