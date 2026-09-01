use std::io::{BufRead, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

fn ppid() -> Option<u32> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()
}

fn is_long_lived(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--persist" || arg == "-o" || arg == "--scene")
}

fn signal_ready(pid: u32) {
    let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") else {
        return;
    };
    let sock = Path::new(&runtime).join("skwd-wall-v2/wall.sock");
    if let Ok(mut stream) = UnixStream::connect(&sock) {
        let _ = writeln!(
            stream,
            "{{\"method\":\"paper.ready\",\"params\":{{\"pid\":{pid}}},\"id\":0}}"
        );
    }
}

fn is_swap_command(line: &str) -> bool {
    serde_json::from_str::<paper_control::PaperCommand>(line).is_ok_and(|command| {
        matches!(paper_control::classify_command(command), paper_control::CommandClass::Swap(_))
    })
}

fn acknowledge_swaps(pid: u32) {
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { return };
        if is_swap_command(&line) {
            signal_ready(pid);
        }
    }
}

fn main() {
    let pid = std::process::id();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let persist = is_long_lived(&args);
    signal_ready(pid);
    if !persist {
        std::thread::sleep(Duration::from_millis(400));
        return;
    }
    std::thread::spawn(move || acknowledge_swaps(pid));
    let parent = ppid();
    let deadline = Instant::now() + Duration::from_secs(240);
    loop {
        std::thread::sleep(Duration::from_millis(150));
        if ppid() != parent || Instant::now() >= deadline {
            return;
        }
    }
}

#[cfg(test)]
#[path = "fake_renderer/tests.rs"]
mod tests;
