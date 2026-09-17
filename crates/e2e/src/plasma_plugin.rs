use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::sandbox::Sandbox;
use crate::wait::wait_until;

pub struct FakePlugin {
    output: String,
    stream: UnixStream,
    lines: Arc<Mutex<Vec<Value>>>,
    reader: Option<JoinHandle<()>>,
}

fn present(root: &Path, line: &Value) {
    if let Some(id) = line["entry"]["presentationId"].as_str() {
        let _ = std::fs::write(root.join(format!("{id}.json")), r#"{"state":"ready"}"#);
    }
}

impl FakePlugin {
    pub fn connect(sandbox: &Sandbox, output: &str) -> Self {
        let socket = sandbox.root.join("runtime/skwd-wall-v2/window-state.sock");
        let mut connected = None;
        assert!(
            wait_until(
                || {
                    connected = UnixStream::connect(&socket).ok();
                    connected.is_some()
                },
                Duration::from_secs(5)
            ),
            "no window-state socket at {}",
            socket.display()
        );
        let stream = connected.expect("window-state stream");
        let reader = stream.try_clone().expect("window-state reader");
        let lines = Arc::new(Mutex::new(Vec::new()));
        let received = Arc::clone(&lines);
        let root = sandbox.root.join("runtime/skwd-paper-plasma");
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(reader).lines() {
                let Some(value) = line.ok().and_then(|line| serde_json::from_str(&line).ok())
                else {
                    return;
                };
                present(&root, &value);
                received.lock().expect("plugin lines").push(value);
            }
        });
        let mut plugin = Self { output: output.to_string(), stream, lines, reader: Some(reader) };
        plugin.report(false);
        plugin
    }

    fn send(&mut self, frame: &Value) {
        writeln!(self.stream, "{frame}").expect("write window-state frame");
    }

    pub fn report(&mut self, fullscreen: bool) {
        let frame = json!({"version": 1, "output": self.output, "supported": true, "fullscreen": fullscreen, "maximized": false});
        self.send(&frame);
    }

    pub fn subscribe(&mut self) {
        assert!(
            self.wait(|lines| lines
                .iter()
                .any(|line| line["capabilities"] == json!(["assignments"]))),
            "walld never offered assignments to {}: {:?}",
            self.output,
            self.lines()
        );
        let frame = json!({"version": 2, "output": self.output, "subscribe": "assignments"});
        self.send(&frame);
    }

    pub fn lines(&self) -> Vec<Value> {
        self.lines.lock().expect("plugin lines").clone()
    }

    pub fn entries(&self) -> Vec<Value> {
        self.lines().into_iter().filter_map(|line| line.get("entry").cloned()).collect()
    }

    pub fn wait(&self, predicate: impl Fn(&[Value]) -> bool) -> bool {
        wait_until(|| predicate(&self.lines.lock().expect("plugin lines")), Duration::from_secs(5))
    }
}

impl Drop for FakePlugin {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
