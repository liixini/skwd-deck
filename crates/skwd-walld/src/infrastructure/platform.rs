use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use tokio::sync::Semaphore;
use wall_proto::rpc;

use crate::composition::context::Ctx;

#[cfg(target_env = "gnu")]
const MALLOC_ARENA_MAX: libc::c_int = 2;
#[cfg(target_env = "gnu")]
const MALLOC_MMAP_THRESHOLD: libc::c_int = 1024 * 1024;
pub(crate) const MAX_IPC_CONNECTIONS: usize = 32;
static IPC_CONNECTIONS: Semaphore = Semaphore::const_new(MAX_IPC_CONNECTIONS);
#[cfg(test)]
static REJECTED_CONNECTIONS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

pub(crate) fn tune_allocator() {
    #[cfg(target_env = "gnu")]
    unsafe {
        libc::mallopt(libc::M_ARENA_MAX, MALLOC_ARENA_MAX);
        libc::mallopt(libc::M_MMAP_THRESHOLD, MALLOC_MMAP_THRESHOLD);
    }
}

pub(crate) fn debug_enabled() -> bool {
    std::env::args().any(|argument| argument == "--debug")
        || matches!(std::env::var("SKWD_WALL_DEBUG").as_deref(), Ok("1" | "true"))
}

pub(crate) fn bind_socket() -> anyhow::Result<Option<(UnixListener, PathBuf)>> {
    if std::env::var_os("XDG_RUNTIME_DIR").is_none() {
        log::warn!(
            "XDG_RUNTIME_DIR unset; falling back to /tmp for the socket. The GUI must agree on this or it will not connect."
        );
    }
    let path = wall_proto::resolve_socket();
    if let Some(parent) = path.parent() {
        secure_socket_dir(parent)?;
    }
    if UnixStream::connect(&path).is_ok() {
        log::info!("another skwd-walld already owns {}, exiting", path.display());
        return Ok(None);
    }
    remove_stale_socket(&path)?;
    let listener = UnixListener::bind(&path)?;
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    Ok(Some((listener, path)))
}

pub(crate) fn build_runtime() -> anyhow::Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("skwd-rt")
        .thread_stack_size(512 * 1024)
        .max_blocking_threads(64)
        .enable_all()
        .build()?)
}

pub(crate) fn run_server(
    runtime: &tokio::runtime::Runtime,
    listener: UnixListener,
    ctx: Ctx,
) -> anyhow::Result<()> {
    runtime.block_on(serve(listener, ctx))
}

async fn serve(listener: UnixListener, ctx: Ctx) -> anyhow::Result<()> {
    listener.set_nonblocking(true)?;
    let listener = tokio::net::UnixListener::from_std(listener)?;
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let Ok(permit) = IPC_CONNECTIONS.try_acquire() else {
                    log::warn!(
                        "IPC connection limit ({MAX_IPC_CONNECTIONS}) reached; rejecting peer"
                    );
                    #[cfg(test)]
                    REJECTED_CONNECTIONS.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                    continue;
                };
                let connection_ctx = ctx.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    crate::infrastructure::rpc::handle_conn(stream, &connection_ctx).await;
                });
            }
            Err(error) => log::warn!("accept failed: {error}"),
        }
    }
}

pub(crate) fn run_diag_client() -> anyhow::Result<()> {
    let path = wall_proto::resolve_socket();
    let mut connection = wall_proto::client::Conn::connect_to(&path)
        .map_err(|error| anyhow::anyhow!("no running skwd-walld at {}: {error}", path.display()))?;
    match connection.call(rpc::DIAG, &serde_json::json!({})) {
        Ok(result) => match result.get("banner").and_then(|value| value.as_str()) {
            Some(banner) => println!("{banner}"),
            None => println!("{result}"),
        },
        Err(wall_proto::client::CallError::Rpc(error)) => {
            println!("diag error: {}", error.message);
        }
        Err(wall_proto::client::CallError::Unreachable) => {
            anyhow::bail!("skwd-walld dropped the diag connection");
        }
    }
    Ok(())
}

fn secure_socket_dir(parent: &Path) -> anyhow::Result<()> {
    use anyhow::Context;

    std::fs::create_dir_all(parent)
        .with_context(|| format!("create socket dir {}", parent.display()))?;
    let metadata = std::fs::symlink_metadata(parent)
        .with_context(|| format!("stat socket dir {}", parent.display()))?;
    anyhow::ensure!(
        metadata.file_type().is_dir(),
        "socket dir {} is not a real directory; refusing to bind",
        parent.display()
    );
    let uid = unsafe { libc::geteuid() };
    anyhow::ensure!(
        metadata.uid() == uid,
        "socket dir {} is owned by uid {} not this daemon's uid {}; refusing to bind",
        parent.display(),
        metadata.uid(),
        uid
    );
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
        .with_context(|| format!("chmod 0700 socket dir {}", parent.display()))?;
    let mode = std::fs::symlink_metadata(parent)?.permissions().mode() & 0o777;
    anyhow::ensure!(mode == 0o700, "socket dir {} did not become private", parent.display());
    Ok(())
}

fn remove_stale_socket(path: &Path) -> anyhow::Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let uid = unsafe { libc::geteuid() };
    anyhow::ensure!(
        metadata.uid() == uid,
        "refusing to remove stale socket {} owned by uid {}",
        path.display(),
        metadata.uid()
    );
    anyhow::ensure!(
        metadata.file_type().is_socket(),
        "refusing to replace non-socket path {}",
        path.display()
    );
    std::fs::remove_file(path)?;
    Ok(())
}

#[cfg(test)]
#[path = "platform/tests.rs"]
mod tests;
