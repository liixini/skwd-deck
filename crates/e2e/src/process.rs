use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::Duration;

use crate::rpc::Client;
use crate::sandbox::Sandbox;
use crate::wait::wait_until;

pub struct Walld {
    child: Child,
    socket: PathBuf,
    log: PathBuf,
}

impl Walld {
    pub fn start(sandbox: &Sandbox) -> Self {
        let log = sandbox.root.join("walld.log");
        let out = std::fs::File::create(&log).expect("walld log");
        let mut cmd = sandbox.walld_command();
        cmd.stdout(out.try_clone().expect("log clone")).stderr(out).stdin(Stdio::null());
        let child = cmd.spawn().expect("spawn walld");
        let walld = Self { child, socket: sandbox.socket(), log };
        assert!(
            walld.wait_responsive(Duration::from_secs(8)),
            "walld not up on {}\n{}",
            walld.socket.display(),
            walld.log_contents()
        );
        walld
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn responsive(&self) -> bool {
        Client::connect(&self.socket)
            .and_then(|mut client| client.call("status", json!({}), 1))
            .is_some_and(|resp| resp.get("result").is_some())
    }

    pub fn wait_responsive(&self, timeout: Duration) -> bool {
        wait_until(|| self.responsive(), timeout)
    }

    pub fn log_contents(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    pub fn wait_log(&self, needle: &str, timeout: Duration) -> bool {
        wait_until(|| self.log_contents().contains(needle), timeout)
    }

    pub fn log_lines(&self, needle: &str) -> Vec<String> {
        self.log_contents()
            .lines()
            .filter(|line| line.contains(needle))
            .map(str::to_string)
            .collect()
    }

    pub fn client(&self) -> Client {
        Client::connect(&self.socket).expect("connect walld socket")
    }
}

impl Drop for Walld {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn pss_mb(pid: u32) -> f64 {
    let Ok(text) = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")) else {
        return 0.0;
    };
    let kb: u64 = text
        .lines()
        .filter_map(|line| line.strip_prefix("Pss:"))
        .filter_map(|rest| rest.split_whitespace().next())
        .filter_map(|num| num.parse::<u64>().ok())
        .sum();
    kb as f64 / 1024.0
}

pub fn scan_pids(walld_pid: u32) -> Vec<u32> {
    child_pids(walld_pid, "skwd-wall-scan")
}

pub fn child_pids(parent: u32, comm: &str) -> Vec<u32> {
    pgrep_x(comm).into_iter().filter(|&pid| parent_pid(pid) == Some(parent)).collect()
}

pub fn procs_with_env(comms: &[&str], env_needle: &str) -> Vec<u32> {
    let needle = env_needle.as_bytes();
    comms
        .iter()
        .flat_map(|comm| pgrep_x(comm))
        .filter(|&pid| {
            std::fs::read(format!("/proc/{pid}/environ"))
                .is_ok_and(|env| env.windows(needle.len()).any(|slice| slice == needle))
        })
        .collect()
}

fn pgrep_x(comm: &str) -> Vec<u32> {
    let Ok(out) = std::process::Command::new("pgrep").args(["-x", comm]).output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .filter_map(|tok| tok.parse().ok())
        .collect()
}

fn parent_pid(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = stat.rsplit_once(')')?.1;
    after_comm.split_whitespace().nth(1)?.parse().ok()
}
