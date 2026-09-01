#![cfg(all(test, target_os = "linux"))]

use paper_runtime::seccomp::{apply, deny_filter};

use super::{
    ADDRESS_SPACE_CAP, BLOCKED, FILE_SIZE_CAP, NOFILE_CAP, Policy, landlock, restrict_decode,
};

#[test]
fn filter_program_size() {
    let prog = deny_filter(BLOCKED);
    assert!(prog.len() >= 8);
    assert!(prog.len() < u16::MAX as usize);
}

#[test]
fn blocked_list_pinned() {
    assert_eq!(BLOCKED.len(), 71);
    assert!(BLOCKED.contains(&libc::SYS_execve));
    assert!(BLOCKED.contains(&libc::SYS_open_by_handle_at));
    assert!(BLOCKED.contains(&libc::SYS_pidfd_getfd));
    assert!(BLOCKED.contains(&libc::SYS_io_uring_setup));
    assert!(BLOCKED.contains(&libc::SYS_setxattr));
    assert!(BLOCKED.contains(&libc::SYS_mount_setattr));
}

#[test]
fn landlock_abi_contract() {
    assert!(landlock::abi_version().unwrap() >= 3);
}

#[test]
fn enforcement_blocks_new_sockets_and_exec() {
    let filter = deny_filter(BLOCKED);
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0, "fork failed");
    if pid == 0 {
        let code = child_probe(&filter, true);
        unsafe { libc::_exit(code) };
    }
    let mut status = 0;
    unsafe { libc::waitpid(pid, &mut status, 0) };
    assert!(libc::WIFEXITED(status), "child did not exit");
    let code = libc::WEXITSTATUS(status);
    assert_eq!(code, 0, "probe {code:#b}: bit1=inet bit2=unix bit3=exec bit4=inherited");
}

#[test]
fn filesystem_policy_blocks_unlisted_paths() {
    let root = std::env::temp_dir().join(format!("skwd-sandbox-{}", std::process::id()));
    let allowed = root.join("allowed");
    let denied = root.join("denied");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&allowed).unwrap();
    std::fs::create_dir_all(&denied).unwrap();
    std::fs::write(allowed.join("read"), b"allowed").unwrap();
    std::fs::write(denied.join("secret"), b"denied").unwrap();
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0, "fork failed");
    if pid == 0 {
        let code = filesystem_probe(&allowed, &denied);
        unsafe { libc::_exit(code) };
    }
    let mut status = 0;
    unsafe { libc::waitpid(pid, &mut status, 0) };
    let _ = std::fs::remove_dir_all(root);
    assert!(libc::WIFEXITED(status), "child did not exit");
    assert_eq!(libc::WEXITSTATUS(status), 0, "filesystem policy probe failed");
}

fn filesystem_probe(allowed: &std::path::Path, denied: &std::path::Path) -> i32 {
    let preopened = std::fs::OpenOptions::new().write(true).open(denied.join("secret")).unwrap();
    if restrict_decode(&Policy::new().write(allowed)).is_err() {
        return 1;
    }
    if !std::fs::read(allowed.join("read")).is_ok_and(|bytes| bytes == b"allowed") {
        return 2;
    }
    if std::fs::write(allowed.join("write"), b"ok").is_err() {
        return 3;
    }
    let limits_ok = limit(libc::RLIMIT_AS).is_some_and(|value| value <= ADDRESS_SPACE_CAP)
        && limit(libc::RLIMIT_CPU).is_some_and(|value| value <= 120)
        && limit(libc::RLIMIT_FSIZE).is_some_and(|value| value <= FILE_SIZE_CAP)
        && limit(libc::RLIMIT_NOFILE).is_some_and(|value| value <= NOFILE_CAP);
    if !limits_ok {
        return 5;
    }
    if preopened.set_len(0).is_err() {
        return 6;
    }
    match std::fs::read(denied.join("secret")) {
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => 0,
        _ => 4,
    }
}

#[test]
fn root_alias_is_rejected() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!("skwd-sandbox-root-{}", std::process::id()));
    let _ = std::fs::remove_file(&root);
    symlink("/", &root).unwrap();
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0, "fork failed");
    if pid == 0 {
        let rejected = restrict_decode(&Policy::new().read(&root)).is_err();
        unsafe { libc::_exit(i32::from(!rejected)) };
    }
    let mut status = 0;
    unsafe { libc::waitpid(pid, &mut status, 0) };
    let _ = std::fs::remove_file(root);
    assert!(libc::WIFEXITED(status), "child did not exit");
    assert_eq!(libc::WEXITSTATUS(status), 0, "root alias policy was accepted");
}

fn limit(resource: libc::__rlimit_resource_t) -> Option<u64> {
    let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    (unsafe { libc::getrlimit(resource, &mut limit) } == 0).then_some(limit.rlim_cur)
}

fn child_probe(filter: &[libc::sock_filter], block_exec: bool) -> i32 {
    let mut inherited = [-1; 2];
    if unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, inherited.as_mut_ptr()) } != 0
    {
        return 16;
    }
    if apply(filter, true, Some(NOFILE_CAP)).is_err() {
        return 1;
    }
    let inet = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0) };
    let inet_blocked =
        inet < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
    let unix = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    let unix_blocked =
        unix < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
    let byte = [42u8];
    let mut received = [0u8];
    let inherited_ok = unsafe {
        libc::write(inherited[0], byte.as_ptr().cast(), byte.len()) == 1
            && libc::read(inherited[1], received.as_mut_ptr().cast(), received.len()) == 1
            && received == byte
    };
    unsafe {
        libc::close(inherited[0]);
        libc::close(inherited[1]);
    }
    let missing = c"/skwd-nonexistent-xyz".as_ptr();
    let argv = [std::ptr::null::<libc::c_char>()];
    unsafe { libc::execv(missing, argv.as_ptr()) };
    let exec_errno = std::io::Error::last_os_error().raw_os_error();
    let exec_policy_ok =
        if block_exec { exec_errno == Some(libc::EPERM) } else { exec_errno == Some(libc::ENOENT) };

    let mut code = 0;
    if !inet_blocked {
        code |= 0b0010;
    }
    if !unix_blocked {
        code |= 0b0100;
    }
    if !exec_policy_ok {
        code |= 0b1000;
    }
    if !inherited_ok {
        code |= 0b1_0000;
    }
    code
}
