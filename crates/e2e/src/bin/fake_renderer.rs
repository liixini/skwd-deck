use std::io::{BufRead, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

fn ppid() -> Option<u32> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()
}

fn is_long_lived(args: &[String]) -> bool {
    args.iter().any(|arg| {
        arg == "--persist" || arg == "-o" || arg == "--scene" || arg == "--transition-hold"
    })
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

fn acknowledge_swaps(pid: u32, held: bool) {
    let delay = std::env::var("SKWD_FAKE_SWAP_DELAY_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(Duration::ZERO, Duration::from_millis);
    let mut trace = std::env::var_os("SKWD_FAKE_SWAP_TRACE")
        .and_then(|path| std::fs::File::options().create(true).append(true).open(path).ok());
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { return };
        let value: serde_json::Value = serde_json::from_str(&line).unwrap_or_default();
        if value["reveal"] == true {
            signal_ready(pid);
            continue;
        }
        if held && value["pause"] == false {
            std::thread::sleep(Duration::from_millis(400));
            std::process::exit(0);
        }
        if is_swap_command(&line) {
            if let Some(trace) = trace.as_mut() {
                let command: serde_json::Value = serde_json::from_str(&line).unwrap();
                let record = format!(
                    "{}\n",
                    serde_json::json!({"event": "received", "pid": pid, "command": command})
                );
                let _ = trace.write_all(record.as_bytes());
            }
            std::thread::sleep(delay);
            if let Some(trace) = trace.as_mut() {
                let record = format!("{}\n", serde_json::json!({"event": "ready", "pid": pid}));
                let _ = trace.write_all(record.as_bytes());
            }
            signal_ready(pid);
        }
    }
}

fn main() {
    let pid = std::process::id();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let persist = is_long_lived(&args);
    if args.iter().any(|arg| arg == "--transition-from")
        && let Ok(ms) = std::env::var("SKWD_E2E_TRANSITION_DELAY_MS")
        && let Ok(ms) = ms.parse::<u64>()
    {
        std::thread::sleep(Duration::from_millis(ms));
    }
    signal_ready(pid);
    if !persist {
        std::thread::sleep(Duration::from_millis(400));
        return;
    }
    let held = args.iter().any(|arg| arg == "--transition-hold")
        && !args.iter().any(|arg| arg == "--persist");
    std::thread::spawn(move || acknowledge_swaps(pid, held));
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
