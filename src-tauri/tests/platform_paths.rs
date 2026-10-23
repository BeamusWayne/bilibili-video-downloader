#[tokio::test]
async fn long_unicode_titles_can_be_written_to_the_file_system() {
    use bvd::model::*;
    let root = tempfile::tempdir().unwrap();
    let item = MediaItem {
        id: "BVmacos".into(),
        url: "https://www.bilibili.com/video/BVmacos".into(),
        title: "超长标题🎬".repeat(100),
        album_title: String::new(),
        uploader: "测试 UP".into(),
        thumbnail: String::new(),
        duration: 1.,
        index: 1,
    };
    let mut options = Options::default();
    options.directory = root.path().to_string_lossy().into_owned();
    options.template = "{标题}".into();
    let engine = bvd::engine::Engine::new(
        root.path().join("state"),
        bvd::platform::development_tools(),
    )
    .unwrap();
    engine.enqueue(vec![item], options).unwrap();
    let task = engine.tasks.lock().unwrap()[0].clone();
    let output = std::path::Path::new(&task.output);
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    std::fs::write(output, b"test output").unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"test output");
}

#[cfg(target_os = "macos")]
#[test]
fn native_keychain_persists_across_entry_instances() {
    let user = format!("bundle-check-{}", uuid::Uuid::new_v4());
    let entry = keyring::Entry::new("BilibiliVideoDownloader-CI", &user).unwrap();
    entry.set_password("test-value-not-a-real-cookie").unwrap();
    let second = keyring::Entry::new("BilibiliVideoDownloader-CI", &user).unwrap();
    let result = second.get_password();
    entry.delete_credential().unwrap();
    assert_eq!(result.unwrap(), "test-value-not-a-real-cookie");
    assert!(matches!(
        second.get_password(),
        Err(keyring::Error::NoEntry)
    ));
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn cancelling_capture_stops_descendants_before_releasing_the_slot() {
    use std::{path::Path, time::Duration};
    use tokio_util::sync::CancellationToken;
    let directory = tempfile::tempdir().unwrap();
    let heartbeat = directory.path().join("heartbeat");
    let cancel = CancellationToken::new();
    let worker_cancel = cancel.clone();
    let file = heartbeat.to_string_lossy().into_owned();
    let worker = tokio::spawn(async move {
        bvd::process::capture(
            Path::new("/bin/sh"),
            &[
                "-c".into(),
                "(while :; do printf x >> \"$1\"; sleep 0.05; done) & wait".into(),
                "process-group-test".into(),
                file,
            ],
            &worker_cancel,
            Duration::from_secs(10),
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        while !heartbeat.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    assert!(worker.await.unwrap().is_err());
    tokio::time::sleep(Duration::from_millis(100)).await;
    let confirmed = std::fs::metadata(&heartbeat).unwrap().len();
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(std::fs::metadata(&heartbeat).unwrap().len(), confirmed);
}
