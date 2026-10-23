#![cfg(target_os = "macos")]

#[test]
fn long_unicode_titles_can_be_written_to_the_file_system() {
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
    let task = Task::new(item, options);
    let output = std::path::Path::new(&task.output);
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    std::fs::write(output, b"test output").unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"test output");
}
