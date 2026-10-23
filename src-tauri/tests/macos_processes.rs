#![cfg(target_os = "macos")]

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
    // A surviving grandchild keeps writing even after the immediate shell dies.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let confirmed = std::fs::metadata(&heartbeat).unwrap().len();
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(std::fs::metadata(&heartbeat).unwrap().len(), confirmed);
}
