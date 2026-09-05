use super::*;

#[test]
fn library_policy_reads_linked_items_without_granting_their_parent() {
    let root = std::env::temp_dir().join(format!("skwd-scan-linked-policy-{}", std::process::id()));
    let workshop = root.join("workshop");
    let external = root.join("external/42");
    std::fs::create_dir_all(&workshop).unwrap();
    std::fs::create_dir_all(&external).unwrap();
    std::fs::write(external.join("project.json"), b"{}").unwrap();
    std::fs::write(root.join("external/unlisted"), b"private").unwrap();
    std::os::unix::fs::symlink(&external, workshop.join("42")).unwrap();
    let config = Config::from_root(
        serde_json::json!({"paths": {"steamWorkshop":workshop,"wallpaper":root.join("images"),"videoWallpaper":root.join("videos")}}),
    );
    let policy = library_policy(&config);
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0);
    if pid == 0 {
        let code = if sandbox::restrict_decode(&policy).is_err() {
            1
        } else if std::fs::read(workshop.join("42/project.json")).is_err() {
            2
        } else if !std::fs::read(root.join("external/unlisted"))
            .is_err_and(|error| error.kind() == std::io::ErrorKind::PermissionDenied)
        {
            3
        } else {
            0
        };
        unsafe { libc::_exit(code) };
    }
    let mut status = 0;
    unsafe { libc::waitpid(pid, &mut status, 0) };
    std::fs::remove_dir_all(root).unwrap();
    assert!(libc::WIFEXITED(status));
    assert_eq!(libc::WEXITSTATUS(status), 0);
}
