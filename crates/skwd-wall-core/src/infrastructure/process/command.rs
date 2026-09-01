use std::process::Command;

pub fn tool(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program);
    die_with_parent(&mut cmd);
    cmd
}

#[cfg(feature = "daemon")]
pub(crate) fn renderer(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program);
    harden_renderer(&mut cmd);
    cmd
}

fn die_with_parent(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    unsafe {
        cmd.pre_exec(|| {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() == 1 {
                std::process::exit(0);
            }
            Ok(())
        });
    }
}

#[cfg(feature = "daemon")]
fn harden_renderer(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    unsafe {
        cmd.pre_exec(|| {
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            let no_core = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
            if libc::setrlimit(libc::RLIMIT_CORE, &no_core) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            let mut nofile = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
            if libc::getrlimit(libc::RLIMIT_NOFILE, &mut nofile) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            let cap = nofile.rlim_max.min(4096);
            nofile.rlim_cur = cap;
            nofile.rlim_max = cap;
            if libc::setrlimit(libc::RLIMIT_NOFILE, &nofile) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(feature = "daemon")]
pub(crate) fn spawn_reaped(command: &mut Command, what: &str) -> bool {
    spawn_reaped_pid(command, what).is_some()
}

#[cfg(feature = "daemon")]
pub(crate) fn spawn_reaped_pid(command: &mut Command, what: &str) -> Option<u32> {
    match command.spawn() {
        Ok(mut child) => {
            let pid = child.id();
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Some(pid)
        }
        Err(error) => {
            log::warn!("{what} spawn failed: {error}");
            None
        }
    }
}
