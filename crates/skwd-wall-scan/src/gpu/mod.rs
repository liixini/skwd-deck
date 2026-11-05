mod codec;

use std::fs::File;
use std::io;
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Stdio};

use skwd_wall_core::{blocks, paths};

use crate::sandbox;

const SHADER_DIRECTORY: &str = "gpu-shaders";
const NVIDIA_SHADER_CACHE: &str = "__GL_SHADER_DISK_CACHE_PATH";
const MESA_SHADER_CACHE: &str = "MESA_SHADER_CACHE_DIR";
const WATCHDOG_NAME: &str = "scan-gpu-peer";
const REAPER_NAME: &str = "scan-gpu-reap";
const PEER_CLOSED: i32 = 0;
const PEER_FAILED: i32 = 1;
const WAIT_FOREVER: i32 = -1;
const DRIVER_READ_PATHS: &[&str] = &[
    "/usr",
    "/lib",
    "/lib64",
    "/etc/ld.so.cache",
    "/etc/vulkan",
    "/etc/nvidia",
    "/etc/machine-id",
    "/proc/self",
    "/proc/driver/nvidia",
    "/proc/meminfo",
    "/proc/cpuinfo",
    "/proc/sys/vm",
    "/run/udev/data",
];

pub(crate) struct Session {
    pub(crate) encoder: blocks::Encoder,
    child: Option<Child>,
}

impl Session {
    pub(crate) fn start(active: bool) -> Self {
        if !active || std::env::var(blocks::GPU_MODE_ENV).as_deref() == Ok(blocks::GPU_CPU_ONLY) {
            return Self { encoder: blocks::Encoder::default(), child: None };
        }
        match Self::spawn() {
            Ok(session) => session,
            Err(error) => {
                log::warn!("scan: gpu helper unavailable: {error}; using cpu");
                Self { encoder: blocks::Encoder::default(), child: None }
            }
        }
    }

    fn spawn() -> io::Result<Self> {
        let (parent, peer) = UnixStream::pair()?;
        let encoder = blocks::Encoder::from_stream(parent)?;
        let mut command = skwd_wall_core::proc::tool(std::env::current_exe()?);
        command
            .arg(blocks::GPU_WORKER_FLAG)
            .arg(paths::cache_dir().join(SHADER_DIRECTORY))
            .stdin(Stdio::from(OwnedFd::from(peer.try_clone()?)))
            .stdout(Stdio::from(OwnedFd::from(peer)))
            .stderr(Stdio::inherit());
        let child = command.spawn()?;
        Ok(Self { encoder, child: Some(child) })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.encoder.shutdown();
        if let Some(mut child) = self.child.take() {
            match child.try_wait() {
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => {
                    if let Err(error) =
                        std::thread::Builder::new().name(REAPER_NAME.to_string()).spawn(move || {
                            let _ = child.wait();
                        })
                    {
                        log::debug!("scan: gpu reaper unavailable: {error}");
                    }
                }
            }
        }
    }
}

pub(crate) fn run(cache: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(cache.is_absolute(), "GPU shader cache path");
    unsafe {
        std::env::set_var(NVIDIA_SHADER_CACHE, cache);
        std::env::set_var(MESA_SHADER_CACHE, cache);
    }
    let mut policy = sandbox::Policy::new().hardware().write(cache);
    for path in DRIVER_READ_PATHS {
        policy = policy.read(path);
    }
    sandbox::restrict_decode(&policy)?;
    let mut socket = File::from(io::stdin().as_fd().try_clone_to_owned()?);
    watch_peer(socket.try_clone()?)?;
    let mut encoder = None;
    while let Some(mips) = blocks::read_request(&mut socket)? {
        if encoder.is_none() {
            encoder = Some(codec::Encoder::new()?);
            log::info!("scan: gpu encoder ready");
        }
        if let Some(encoder) = &mut encoder {
            let bytes = encoder.encode(&mips)?;
            blocks::write_response(&mut socket, &bytes)?;
        }
    }
    Ok(())
}

fn watch_peer(peer: File) -> io::Result<()> {
    std::thread::Builder::new().name(WATCHDOG_NAME.to_string()).spawn(move || {
        let mut descriptor =
            libc::pollfd { fd: peer.as_raw_fd(), events: libc::POLLRDHUP, revents: 0 };
        loop {
            let ready = unsafe { libc::poll(&raw mut descriptor, 1, WAIT_FOREVER) };
            if ready > 0 {
                unsafe { libc::_exit(PEER_CLOSED) };
            }
            if ready < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                unsafe { libc::_exit(PEER_FAILED) };
            }
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests;
