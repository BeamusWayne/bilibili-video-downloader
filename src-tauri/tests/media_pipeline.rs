use bvd::{media, model::*, process::capture};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn bundled_tools_produce_all_formats_and_watermark_previews() {
    let tools = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries");
    assert!(
        tools.join("ffmpeg.exe").exists(),
        "run npm run prepare:tools"
    );
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.mp4");
    let cancel = CancellationToken::new();
    let args = vec![
        "-v",
        "error",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=320x240:rate=24",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=44100",
        "-t",
        "2",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-y",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain([source.to_string_lossy().into_owned()])
    .collect::<Vec<_>>();
    capture(
        &tools.join("ffmpeg.exe"),
        &args,
        &cancel,
        Duration::from_secs(45),
    )
    .await
    .unwrap();
    let item = MediaItem {
        id: "test".into(),
        url: "https://www.bilibili.com/video/BVtest".into(),
        title: "测试视频".into(),
        album_title: String::new(),
        uploader: "测试".into(),
        thumbnail: String::new(),
        duration: 2.,
        index: 1,
    };
    let resolved = Resolved {
        streams: vec![Stream {
            id: "test".into(),
            url: String::new(),
            headers: HashMap::new(),
            kind: "video".into(),
            size: 0,
        }],
        duration: 2.,
        title: "测试".into(),
        width: 320,
        height: 240,
        subtitles: serde_json::Value::Null,
        thumbnail: String::new(),
        cid: None,
    };
    for format in ["mp4", "mkv", "mp3", "m4a", "flac", "wav"] {
        let mut opt = Options::default();
        opt.format = format.into();
        let task = Task::new(item.clone(), opt);
        let output = dir.path().join(format!("输出.{format}"));
        media::convert(
            &tools,
            &task,
            &resolved,
            &[source.clone()],
            &output,
            &cancel,
            Arc::new(|_| {}),
        )
        .await
        .unwrap();
        media::verify(&tools, &output, task.options.audio(), &cancel)
            .await
            .unwrap();
    }
    for mode in ["delogo", "blur", "crop"] {
        let mut opt = Options::default();
        opt.watermark = Watermark {
            mode: mode.into(),
            x: 20,
            y: 20,
            width: 80,
            height: 40,
            start: 0.2,
            end: 1.5,
            keep_original: true,
        };
        let task = Task::new(item.clone(), opt);
        let out = dir.path().join(format!("{mode}.mp4"));
        media::convert(
            &tools,
            &task,
            &resolved,
            &[source.clone()],
            &out,
            &cancel,
            Arc::new(|_| {}),
        )
        .await
        .unwrap();
    }
}
