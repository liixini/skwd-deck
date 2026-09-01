use std::sync::OnceLock;

pub(crate) use skwd_wall_core::proc::tool;

static RUNTIME: OnceLock<tokio::runtime::Handle> = OnceLock::new();

pub(crate) fn register_runtime(handle: tokio::runtime::Handle) {
    let _ = RUNTIME.set(handle);
}

pub(crate) fn runtime() -> tokio::runtime::Handle {
    tokio::runtime::Handle::try_current()
        .ok()
        .or_else(|| RUNTIME.get().cloned())
        .expect("subprocess supervision requested before the skwd-walld runtime started")
}

pub(crate) fn spawn(
    command: &mut tokio::process::Command,
) -> std::io::Result<tokio::process::Child> {
    let handle = runtime();
    let _handle = handle.enter();
    command.spawn()
}

pub(crate) fn tool_async(program: impl AsRef<std::ffi::OsStr>) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);
    unsafe {
        command.pre_exec(|| {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() == 1 {
                std::process::exit(0);
            }
            Ok(())
        });
    }
    command
}

mod tests;
