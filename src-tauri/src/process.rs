use anyhow::{bail, Result};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command};
use tokio_util::sync::CancellationToken;

#[cfg(unix)]
struct ProcessGroup(libc::pid_t);
#[cfg(unix)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        // This group is created for our child, so it never includes the app.
        // One-file extractors may spawn children that outlive their launcher.
        unsafe {
            libc::kill(-self.0, libc::SIGKILL);
        }
    }
}

pub fn command(path: &Path) -> Command {
    let mut c = Command::new(path);
    c.kill_on_drop(true);
    #[cfg(windows)]
    c.creation_flags(0x08000000);
    #[cfg(unix)]
    c.as_std_mut().process_group(0);
    c
}
pub async fn capture(
    path: &Path,
    args: &[String],
    cancel: &CancellationToken,
    timeout: Duration,
) -> Result<Vec<u8>> {
    let child = command(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    #[cfg(unix)]
    let _group = child.id().map(|pid| ProcessGroup(pid as libc::pid_t));
    let result = tokio::select! {_ = cancel.cancelled()=>bail!("已暂停"),r=tokio::time::timeout(timeout,child.wait_with_output())=>r.map_err(|_|anyhow::anyhow!("子进程超时"))??};
    if !result.status.success() {
        let message = String::from_utf8_lossy(&result.stderr);
        bail!("{}", redact(&message));
    }
    Ok(result.stdout)
}
pub fn redact(s: &str) -> String {
    let url = regex::Regex::new(r#"https?://[^\s"<>]+"#).unwrap();
    let cleaned = url.replace_all(s, "[链接已隐藏]");
    let secret = regex::Regex::new(
        r"(?i)(SESSDATA|bili_jct|Cookie|access_token|refresh_token)[=: ]+[^\s;]+",
    )
    .unwrap();
    secret
        .replace_all(&cleaned, "$1=[已隐藏]")
        .chars()
        .take(1200)
        .collect()
}
pub async fn atomic_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    atomic_bytes(path, &bytes).await
}
pub async fn atomic_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap();
    tokio::fs::create_dir_all(parent).await?;
    let temp = tempfile::NamedTempFile::new_in(parent)?;
    let mut file = tokio::fs::File::from_std(temp.reopen()?);
    file.write_all(bytes).await?;
    file.sync_all().await?;
    drop(file);
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
