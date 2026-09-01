use std::path::{Path, PathBuf};

pub(crate) struct Policy {
    read: Vec<PathBuf>,
    write: Vec<PathBuf>,
    hardware: bool,
    cpu_seconds: u64,
}

impl Default for Policy {
    fn default() -> Self {
        Self { read: Vec::new(), write: Vec::new(), hardware: false, cpu_seconds: 120 }
    }
}

impl Policy {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn read(mut self, path: impl Into<PathBuf>) -> Self {
        self.read.push(path.into());
        self
    }

    pub(crate) fn write(mut self, path: impl Into<PathBuf>) -> Self {
        self.write.push(path.into());
        self
    }

    pub(crate) fn hardware(mut self) -> Self {
        self.hardware = true;
        self
    }

    pub(crate) fn cpu_seconds(mut self, seconds: u64) -> Self {
        self.cpu_seconds = seconds.max(1);
        self
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn restrict_decode(_policy: &Policy) -> anyhow::Result<()> {
    anyhow::bail!("decode sandbox requires Linux")
}

#[cfg(target_os = "linux")]
pub(crate) fn restrict_decode(policy: &Policy) -> anyhow::Result<()> {
    validate_policy(policy)?;
    for path in &policy.write {
        std::fs::create_dir_all(path).map_err(|error| {
            anyhow::anyhow!("decode sandbox create {}: {error}", path.display())
        })?;
    }
    landlock::restrict(policy).map_err(|error| anyhow::anyhow!("decode sandbox: {error}"))?;
    apply_resource_limits(policy).map_err(|error| anyhow::anyhow!("decode sandbox: {error}"))?;
    paper_runtime::seccomp::apply(
        &paper_runtime::seccomp::deny_filter(BLOCKED),
        true,
        Some(NOFILE_CAP),
    )
    .map_err(|error| anyhow::anyhow!("decode sandbox: {error}"))?;
    log::debug!("sandbox: decode filesystem and syscall filters active");
    Ok(())
}

#[cfg(target_os = "linux")]
fn validate_policy(policy: &Policy) -> anyhow::Result<()> {
    for path in policy.read.iter().chain(&policy.write) {
        anyhow::ensure!(!path.as_os_str().is_empty(), "decode sandbox refuses an empty path");
        anyhow::ensure!(path != Path::new("/"), "decode sandbox refuses the root filesystem");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn apply_resource_limits(policy: &Policy) -> Result<(), String> {
    set_limit(libc::RLIMIT_NOFILE, NOFILE_CAP, "RLIMIT_NOFILE")?;
    set_limit(libc::RLIMIT_FSIZE, FILE_SIZE_CAP, "RLIMIT_FSIZE")?;
    set_limit(libc::RLIMIT_CPU, policy.cpu_seconds, "RLIMIT_CPU")?;
    if !policy.hardware {
        set_limit(libc::RLIMIT_AS, ADDRESS_SPACE_CAP, "RLIMIT_AS")?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn set_limit(resource: libc::__rlimit_resource_t, value: u64, name: &str) -> Result<(), String> {
    let mut inherited = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    if unsafe { libc::getrlimit(resource, &mut inherited) } != 0 {
        return Err(format!("get {name}: {}", std::io::Error::last_os_error()));
    }
    let value = value.min(inherited.rlim_cur).min(inherited.rlim_max);
    let limit = libc::rlimit { rlim_cur: value, rlim_max: value };
    if unsafe { libc::setrlimit(resource, &limit) } == 0 {
        return Ok(());
    }
    Err(format!("{name}: {}", std::io::Error::last_os_error()))
}

#[cfg(target_os = "linux")]
const NOFILE_CAP: u64 = 4096;
#[cfg(target_os = "linux")]
const FILE_SIZE_CAP: u64 = 256 * 1024 * 1024;
#[cfg(target_os = "linux")]
const ADDRESS_SPACE_CAP: u64 = 4 * 1024 * 1024 * 1024;

#[cfg(target_os = "linux")]
const BLOCKED: &[libc::c_long] = &[
    libc::SYS_execve,
    libc::SYS_execveat,
    libc::SYS_ptrace,
    libc::SYS_process_vm_readv,
    libc::SYS_process_vm_writev,
    libc::SYS_process_madvise,
    libc::SYS_pidfd_getfd,
    libc::SYS_pidfd_send_signal,
    libc::SYS_kill,
    libc::SYS_tkill,
    libc::SYS_socket,
    libc::SYS_socketpair,
    libc::SYS_connect,
    libc::SYS_bind,
    libc::SYS_listen,
    libc::SYS_accept,
    libc::SYS_accept4,
    libc::SYS_sendto,
    libc::SYS_sendmsg,
    libc::SYS_recvfrom,
    libc::SYS_recvmsg,
    libc::SYS_unshare,
    libc::SYS_setns,
    libc::SYS_mount,
    libc::SYS_umount2,
    libc::SYS_pivot_root,
    libc::SYS_chroot,
    libc::SYS_bpf,
    libc::SYS_add_key,
    libc::SYS_keyctl,
    libc::SYS_request_key,
    libc::SYS_kexec_load,
    libc::SYS_init_module,
    libc::SYS_finit_module,
    libc::SYS_delete_module,
    libc::SYS_open_by_handle_at,
    libc::SYS_name_to_handle_at,
    libc::SYS_fsopen,
    libc::SYS_fsconfig,
    libc::SYS_fsmount,
    libc::SYS_move_mount,
    libc::SYS_open_tree,
    libc::SYS_mount_setattr,
    libc::SYS_userfaultfd,
    libc::SYS_perf_event_open,
    libc::SYS_io_uring_setup,
    libc::SYS_fanotify_init,
    libc::SYS_setxattr,
    libc::SYS_lsetxattr,
    libc::SYS_fsetxattr,
    libc::SYS_getxattr,
    libc::SYS_lgetxattr,
    libc::SYS_fgetxattr,
    libc::SYS_listxattr,
    libc::SYS_llistxattr,
    libc::SYS_flistxattr,
    libc::SYS_removexattr,
    libc::SYS_lremovexattr,
    libc::SYS_fremovexattr,
    libc::SYS_chmod,
    libc::SYS_fchmod,
    libc::SYS_fchmodat,
    libc::SYS_chown,
    libc::SYS_fchown,
    libc::SYS_lchown,
    libc::SYS_fchownat,
    libc::SYS_utime,
    libc::SYS_utimes,
    libc::SYS_futimesat,
    libc::SYS_utimensat,
    libc::SYS_truncate,
];

#[cfg(target_os = "linux")]
mod landlock {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;

    use super::Policy;

    const CREATE_RULESET_VERSION: u32 = 1;
    const RULE_PATH_BENEATH: u32 = 1;
    const MIN_ABI: libc::c_long = 3;

    const EXECUTE: u64 = 1 << 0;
    const WRITE_FILE: u64 = 1 << 1;
    const READ_FILE: u64 = 1 << 2;
    const READ_DIR: u64 = 1 << 3;
    const REMOVE_DIR: u64 = 1 << 4;
    const REMOVE_FILE: u64 = 1 << 5;
    const MAKE_CHAR: u64 = 1 << 6;
    const MAKE_DIR: u64 = 1 << 7;
    const MAKE_REG: u64 = 1 << 8;
    const MAKE_SOCK: u64 = 1 << 9;
    const MAKE_FIFO: u64 = 1 << 10;
    const MAKE_BLOCK: u64 = 1 << 11;
    const MAKE_SYM: u64 = 1 << 12;
    const REFER: u64 = 1 << 13;
    const TRUNCATE: u64 = 1 << 14;

    const READ_ACCESS: u64 = READ_FILE | READ_DIR;
    const DEVICE_ACCESS: u64 = READ_ACCESS | WRITE_FILE;
    const WRITE_ACCESS: u64 = READ_ACCESS
        | WRITE_FILE
        | REMOVE_DIR
        | REMOVE_FILE
        | MAKE_DIR
        | MAKE_REG
        | REFER
        | TRUNCATE;
    const HANDLED_ACCESS: u64 = EXECUTE
        | WRITE_FILE
        | READ_FILE
        | READ_DIR
        | REMOVE_DIR
        | REMOVE_FILE
        | MAKE_CHAR
        | MAKE_DIR
        | MAKE_REG
        | MAKE_SOCK
        | MAKE_FIFO
        | MAKE_BLOCK
        | MAKE_SYM
        | REFER
        | TRUNCATE;

    #[repr(C)]
    struct RulesetAttr {
        handled_access_fs: u64,
    }

    #[repr(C, packed)]
    struct PathBeneathAttr {
        allowed_access: u64,
        parent_fd: i32,
    }

    pub(super) fn restrict(policy: &Policy) -> Result<(), String> {
        let abi = abi_version()?;
        let attr = RulesetAttr { handled_access_fs: HANDLED_ACCESS };
        let raw = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                std::ptr::from_ref(&attr),
                std::mem::size_of::<RulesetAttr>(),
                0,
            )
        };
        if raw < 0 {
            return Err(format!("create Landlock ruleset: {}", std::io::Error::last_os_error()));
        }
        let ruleset = unsafe { OwnedFd::from_raw_fd(raw as i32) };
        for path in &policy.read {
            add_if_present(&ruleset, path, READ_ACCESS)?;
        }
        for path in &policy.write {
            add_path(&ruleset, path, WRITE_ACCESS)?;
        }
        if policy.hardware {
            add_if_present(&ruleset, std::path::Path::new("/sys"), READ_ACCESS)?;
            add_if_present(&ruleset, std::path::Path::new("/dev/dri"), DEVICE_ACCESS)?;
            add_if_present(&ruleset, std::path::Path::new("/dev/nvidia-caps"), DEVICE_ACCESS)?;
            if let Ok(entries) = std::fs::read_dir("/dev") {
                for path in entries.flatten().map(|entry| entry.path()).filter(|path| {
                    path.file_name().is_some_and(|name| name.as_bytes().starts_with(b"nvidia"))
                }) {
                    add_if_present(&ruleset, &path, DEVICE_ACCESS)?;
                }
            }
        }
        if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
            return Err(format!("PR_SET_NO_NEW_PRIVS: {}", std::io::Error::last_os_error()));
        }
        if unsafe { libc::syscall(libc::SYS_landlock_restrict_self, ruleset.as_raw_fd(), 0) } == 0 {
            Ok(())
        } else {
            Err(format!("restrict with Landlock ABI {abi}: {}", std::io::Error::last_os_error()))
        }
    }

    pub(super) fn abi_version() -> Result<libc::c_long, String> {
        let abi = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                std::ptr::null::<RulesetAttr>(),
                0,
                CREATE_RULESET_VERSION,
            )
        };
        if abi < MIN_ABI {
            Err(if abi < 0 {
                format!("Landlock ABI query: {}", std::io::Error::last_os_error())
            } else {
                format!("Landlock ABI {abi}; ABI {MIN_ABI} or newer is required")
            })
        } else {
            Ok(abi)
        }
    }

    fn add_if_present(
        ruleset: &OwnedFd,
        path: &std::path::Path,
        access: u64,
    ) -> Result<(), String> {
        match add_path(ruleset, path, access) {
            Err(error) if error.starts_with("missing ") => Ok(()),
            result => result,
        }
    }

    fn add_path(ruleset: &OwnedFd, path: &std::path::Path, access: u64) -> Result<(), String> {
        let path_bytes = path.as_os_str().as_bytes();
        let path_text = path.display();
        let path =
            CString::new(path_bytes).map_err(|_| format!("path contains NUL: {path_text}"))?;
        let raw = unsafe { libc::open(path.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
        if raw < 0 {
            let error = std::io::Error::last_os_error();
            return if error.kind() == std::io::ErrorKind::NotFound {
                Err(format!("missing {path_text}"))
            } else {
                Err(format!("open {path_text}: {error}"))
            };
        }
        let parent = unsafe { OwnedFd::from_raw_fd(raw) };
        let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe { libc::fstat(parent.as_raw_fd(), metadata.as_mut_ptr()) } != 0 {
            return Err(format!("stat {path_text}: {}", std::io::Error::last_os_error()));
        }
        let metadata = unsafe { metadata.assume_init() };
        let mut root = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe { libc::stat(c"/".as_ptr(), root.as_mut_ptr()) } != 0 {
            return Err(format!("stat /: {}", std::io::Error::last_os_error()));
        }
        let root = unsafe { root.assume_init() };
        if metadata.st_dev == root.st_dev && metadata.st_ino == root.st_ino {
            return Err(format!("refuse root filesystem target {path_text}"));
        }
        let allowed_access = if metadata.st_mode & libc::S_IFMT == libc::S_IFDIR {
            access
        } else {
            access & (EXECUTE | WRITE_FILE | READ_FILE | TRUNCATE)
        };
        let attr = PathBeneathAttr { allowed_access, parent_fd: parent.as_raw_fd() };
        let result = unsafe {
            libc::syscall(
                libc::SYS_landlock_add_rule,
                ruleset.as_raw_fd(),
                RULE_PATH_BENEATH,
                std::ptr::from_ref(&attr),
                0,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(format!("allow {path_text}: {}", std::io::Error::last_os_error()))
        }
    }
}

mod tests;
