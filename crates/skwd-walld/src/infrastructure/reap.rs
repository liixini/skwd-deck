const REAPABLE_RENDERER_BINS: [&str; 3] =
    ["skwd-wall-still", "skwd-wall-vk", "linux-wallpaperengine"];

fn arg0_base(arg0: &str) -> &str {
    arg0.rsplit('/').next().unwrap_or(arg0)
}

fn is_reapable_stale_renderer(
    args: &[String],
    ppid: i32,
    walld_pids: &[i32],
    plasma_pids: &[i32],
    owned_dirs: &[String],
) -> bool {
    if walld_pids.contains(&ppid) {
        return false;
    }
    let Some(arg0) = args.first() else {
        return false;
    };
    let name = arg0_base(arg0);
    if name == "skwd-wall-vk"
        && args.iter().any(|arg| arg == "--video-stream")
        && plasma_pids.contains(&ppid)
    {
        return false;
    }
    if REAPABLE_RENDERER_BINS.contains(&name) {
        return true;
    }
    if name == "ffmpeg" && writes_into(args, owned_dirs) {
        return true;
    }
    name == "skwd-wall-scan" && args.iter().any(|arg| arg == "--analyze")
}

fn writes_into(args: &[String], owned_dirs: &[String]) -> bool {
    owned_dirs
        .iter()
        .filter(|dir| !dir.is_empty())
        .any(|dir| args.iter().any(|arg| arg.contains(dir.as_str())))
}

fn collect_process_pids(proc_root: &std::path::Path, process: &str) -> Vec<i32> {
    let mut pids = Vec::new();
    let Ok(entries) = std::fs::read_dir(proc_root) else {
        return pids;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|name| name.parse::<i32>().ok()) else {
            continue;
        };
        if let Some((_, args)) = read_proc_ppid_args(&entry.path())
            && args.first().is_some_and(|arg0| arg0_base(arg0) == process)
        {
            pids.push(pid);
        }
    }
    pids
}

fn read_proc_ppid_args(proc_dir: &std::path::Path) -> Option<(i32, Vec<String>)> {
    let raw = std::fs::read(proc_dir.join("cmdline")).ok()?;
    let args: Vec<String> = raw
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect();
    let status = std::fs::read_to_string(proc_dir.join("status")).ok()?;
    let ppid = status
        .lines()
        .find_map(|line| line.strip_prefix("PPid:"))
        .and_then(|val| val.trim().parse::<i32>().ok())?;
    Some((ppid, args))
}

fn collect_reapable_renderers(proc_root: &std::path::Path, owned_dirs: &[String]) -> Vec<i32> {
    let walld_pids = collect_process_pids(proc_root, "skwd-walld");
    let plasma_pids = collect_process_pids(proc_root, "plasmashell");
    let mut pids = Vec::new();
    let Ok(entries) = std::fs::read_dir(proc_root) else {
        return pids;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|name| name.parse::<i32>().ok()) else {
            continue;
        };
        if let Some((ppid, args)) = read_proc_ppid_args(&entry.path())
            && is_reapable_stale_renderer(&args, ppid, &walld_pids, &plasma_pids, owned_dirs)
        {
            pids.push(pid);
        }
    }
    pids
}

pub(crate) fn kill_stale_renderers(owned_dirs: &[String]) {
    if std::env::var_os("SKWD_WALLD_NO_REAP").is_some() {
        return;
    }
    for pid in collect_reapable_renderers(std::path::Path::new("/proc"), owned_dirs) {
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
    }
}

mod tests;
