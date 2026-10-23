use bvd::{engine::Engine, library::*, model::*, platform};
fn item(part: u32) -> MediaItem {
    MediaItem {
        id: format!("BV1SUnB61E8K_{part}"),
        url: format!("https://www.bilibili.com/video/BV1SUnB61E8K?p={part}"),
        title: "测试".into(),
        album_title: "".into(),
        uploader: "UP".into(),
        thumbnail: "".into(),
        duration: 20.,
        index: part,
    }
}
#[test]
fn handoff_strips_tracking_and_rejects_non_vod_or_injected_parameters() {
    assert_eq!(handoff("bilidownloader://download?url=https%3A%2F%2Fwww.bilibili.com%2Fvideo%2FBV1SUnB61E8K%2F%3Fp%3D2%26vd_source%3Dtest").unwrap(), "https://www.bilibili.com/video/BV1SUnB61E8K?p=2");
    for uri in [
        "bilidownloader://download?url=https://live.bilibili.com/23612045",
        "bilidownloader://download?url=file:///C:/secret",
        "bilidownloader://download?url=https://www.bilibili.com/video/BV1SUnB61E8K&cookie=secret",
        "bilidownloader://download?url=https://www.bilibili.com.evil/video/BV1SUnB61E8K",
        "bilidownloader://download?url=https://u:p@www.bilibili.com/video/BV1SUnB61E8K",
        "bilidownloader://download?url=https://www.bilibili.com/video/BV1SUnB61E8K?p=0",
    ] {
        assert!(handoff(uri).is_err(), "{uri}");
    }
}
#[test]
fn downloaded_history_survives_queue_removal_and_allows_missing_file_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let engine = Engine::new(data.clone(), platform::development_tools()).unwrap();
    let options = Options {
        directory: dir.path().to_string_lossy().into_owned(),
        ..Options::default()
    };
    assert_eq!(
        engine
            .enqueue(vec![item(1), item(2)], options.clone())
            .unwrap(),
        2
    );
    let t = engine.tasks.lock().unwrap()[0].clone();
    std::fs::create_dir_all(std::path::Path::new(&t.output).parent().unwrap()).unwrap();
    std::fs::write(&t.output, b"saved").unwrap();
    engine.store.record_download(&t).unwrap();
    engine.action(&t.id, "remove").unwrap();
    drop(engine);
    let engine = Engine::new(data, platform::development_tools()).unwrap();
    let mut moved = options.clone();
    moved.directory = dir.path().join("elsewhere").to_string_lossy().into_owned();
    assert_eq!(engine.enqueue(vec![item(1)], moved.clone()).unwrap(), 0);
    assert_eq!(
        engine
            .duplicates(&[item(1)], &moved)
            .unwrap()
            .get(&item(1).url)
            .map(String::as_str),
        Some("已下载")
    );
    std::fs::remove_file(t.output).unwrap();
    assert_eq!(engine.enqueue(vec![item(1)], moved).unwrap(), 1);
    let mut audio = options;
    audio.format = "mp3".into();
    assert_eq!(engine.enqueue(vec![item(2)], audio).unwrap(), 1);
}
#[test]
fn enqueue_is_atomic_and_batch_deduplication_is_linear() {
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::new(root.path().join("data"), platform::development_tools()).unwrap();
    let options = Options {
        directory: root.path().to_string_lossy().into_owned(),
        ..Options::default()
    };
    let mut bad = item(9);
    bad.url = "https://example.com/invalid".into();
    assert!(engine.enqueue(vec![item(1), bad], options.clone()).is_err());
    assert!(engine.store.tasks().unwrap().is_empty());
    let items: Vec<_> = (1..=500).flat_map(|i| [item(i), item(i)]).collect();
    assert_eq!(engine.enqueue(items, options.clone()).unwrap(), 500);
    assert_eq!(
        engine
            .enqueue((1..=500).map(item).collect(), options)
            .unwrap(),
        0
    );
}
#[test]
fn presets_round_trip_and_space_guard_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    let store = bvd::store::Store::open(&root.path().join("test.db")).unwrap();
    let s = Settings {
        directory: root.path().to_string_lossy().into_owned(),
        ..Settings::default()
    };
    let mut p = Presets::initial(&s);
    p.default_id = "audio".into();
    p.validate().unwrap();
    store.save_config("presets", &p).unwrap();
    let loaded: Presets = store.config("presets").unwrap();
    assert_eq!(loaded.default_id, "audio");
    assert_eq!(loaded.items[1].options.format, "mp3");
    assert!(check_space(root.path(), u64::MAX, 1).is_err());
    assert!(check_space(root.path(), 0, 0).is_ok());
    p.default_id = "missing".into();
    assert!(p.validate().is_err());
}

#[tokio::test]
async fn low_disk_space_preserves_resume_files_without_network_requests() {
    use std::sync::{atomic::Ordering, Arc};
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("0.part"), b"verified data").unwrap();
    let downloader = bvd::download::Downloader::new(3).unwrap();
    downloader.reserve.store(u64::MAX, Ordering::Relaxed);
    let s = Stream {
        id: "test".into(),
        url: "http://127.0.0.1:1/unreachable".into(),
        headers: Default::default(),
        kind: "video".into(),
        size: 1,
    };
    let result = downloader
        .fetch(
            &s,
            root.path(),
            3,
            0,
            Arc::new(bvd::download::Progress::default()),
            &tokio_util::sync::CancellationToken::new(),
        )
        .await;
    assert!(result.unwrap_err().to_string().contains("磁盘空间不足"));
    assert_eq!(
        std::fs::read(root.path().join("0.part")).unwrap(),
        b"verified data"
    );
}

#[test]
fn parser_selection_rejects_paths_and_keeps_bundled_fallback() {
    let root = tempfile::tempdir().unwrap();
    let bundled = root.path().join("tools");
    let data = root.path().join("data");
    std::fs::create_dir_all(data.join("parser")).unwrap();
    std::fs::write(data.join("parser/current.json"), b"\"../../untrusted\"").unwrap();
    assert_eq!(
        bvd::maintenance::parser_path(&bundled, &data),
        platform::tool(&bundled, "yt-dlp")
    );
    let hash = "a".repeat(64);
    let target = data
        .join("parser")
        .join(format!("{hash}{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&target, b"fixture").unwrap();
    std::fs::write(
        data.join("parser/current.json"),
        serde_json::to_vec(&hash).unwrap(),
    )
    .unwrap();
    assert_eq!(bvd::maintenance::parser_path(&bundled, &data), target);
}
