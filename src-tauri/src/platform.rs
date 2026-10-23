use std::path::{Path, PathBuf};

/// Resolve bundled programs without relying on the user's shell PATH.
pub fn tool(tools: &Path, name: &str) -> PathBuf {
    tools.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}

pub fn development_tools() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries");
    if cfg!(target_os = "macos") {
        root.join("macos")
    } else {
        root
    }
}

pub fn download_directory() -> PathBuf {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .unwrap_or_else(std::env::temp_dir)
        .join("Bilibili")
}
