#[cfg(not(test))]
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use tokio::io::AsyncReadExt;

pub(crate) const RENDER_WORKER_ARG: &str = "--lockscreen-render-worker";
#[cfg(not(test))]
const SELF_PROGRAM: &str = "/proc/self/exe";
const WORKER_TIMEOUT: Duration = Duration::from_secs(30);
const REPLY_BYTES: usize = 64;

pub(crate) fn run_render_worker() -> Result<()> {
    let mut arguments = std::env::args_os().skip(2);
    let source = PathBuf::from(arguments.next().context("Missing lockscreen worker source")?);
    let root = PathBuf::from(arguments.next().context("Missing lockscreen worker directory")?);
    ensure!(arguments.next().is_none(), "Unexpected lockscreen worker argument");
    let image = super::export::encode(&source, &root)?;
    let name = image
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .context("Invalid lockscreen worker image name")?;
    println!("{name}");
    Ok(())
}

#[cfg(not(test))]
pub(super) fn render(source: &Path, root: &Path) -> Result<PathBuf> {
    let mut command = crate::infrastructure::proc::tool_async(SELF_PROGRAM);
    command.arg(RENDER_WORKER_ARG).arg(source).arg(root);
    let bytes = crate::infrastructure::proc::runtime().block_on(execute(command))?;
    super::export::rendered_path(root, std::str::from_utf8(&bytes)?.trim())
}

async fn execute(mut command: tokio::process::Command) -> Result<Vec<u8>> {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::inherit()).kill_on_drop(true);
    let mut child =
        crate::infrastructure::proc::spawn(&mut command).context("Spawn lockscreen worker")?;
    let mut output =
        child.stdout.take().expect("lockscreen worker stdout").take((REPLY_BYTES + 1) as u64);
    let result = match tokio::time::timeout(WORKER_TIMEOUT, async {
        let mut bytes = Vec::new();
        output.read_to_end(&mut bytes).await?;
        ensure!(bytes.len() <= REPLY_BYTES, "Lockscreen worker reply is too large");
        let status = child.wait().await?;
        ensure!(status.success(), "Lockscreen worker exited with {status}");
        Ok(bytes)
    })
    .await
    {
        Ok(result) => result,
        Err(error) => Err(error).context("Lockscreen worker timed out"),
    };
    if result.is_err() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    result
}

#[cfg(test)]
mod tests;
