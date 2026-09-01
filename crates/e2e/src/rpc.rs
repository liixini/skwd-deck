use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

pub struct Client {
    stream: UnixStream,
    buf: Vec<u8>,
}

impl Client {
    pub fn connect(path: &Path) -> Option<Self> {
        let stream = UnixStream::connect(path).ok()?;
        stream.set_read_timeout(Some(Duration::from_secs(10))).ok()?;
        Some(Self { stream, buf: Vec::new() })
    }

    pub fn send_raw(&mut self, data: &[u8]) {
        let _ = self.stream.write_all(data);
    }

    pub fn send(&mut self, method: &str, params: Value, id: u64) {
        let mut msg = serde_json::Map::new();
        msg.insert("method".into(), Value::from(method));
        msg.insert("params".into(), params);
        msg.insert("id".into(), Value::from(id));
        self.send_raw(Value::Object(msg).to_string().as_bytes());
        self.send_raw(b"\n");
    }

    pub fn recv(&mut self, timeout: Duration) -> Option<Value> {
        let _ = self.stream.set_read_timeout(Some(timeout));
        loop {
            if let Some(nl) = self.buf.iter().position(|&byte| byte == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=nl).collect();
                let line = &line[..line.len() - 1];
                return Some(serde_json::from_slice(line).unwrap_or_else(|_| {
                    let head = &line[..line.len().min(200)];
                    serde_json::json!({ "unparseable": String::from_utf8_lossy(head) })
                }));
            }
            let mut chunk = [0u8; 8192];
            match self.stream.read(&mut chunk) {
                Ok(0) | Err(_) => return None,
                Ok(read) => self.buf.extend_from_slice(&chunk[..read]),
            }
        }
    }

    pub fn call(&mut self, method: &str, params: Value, id: u64) -> Option<Value> {
        self.send(method, params, id);
        self.recv(Duration::from_secs(10))
    }
}

pub fn err_code(resp: Option<&Value>) -> Option<i64> {
    resp?.get("error")?.get("code")?.as_i64()
}

pub fn err_message(resp: Option<&Value>) -> String {
    resp.and_then(|value| value.get("error"))
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

pub fn wall_outputs(client: &mut Client) -> Vec<Value> {
    client
        .call("wall.outputs", serde_json::json!({}), 9999)
        .and_then(|resp| resp.get("result")?.get("outputs")?.as_array().cloned())
        .unwrap_or_default()
}

pub fn field<'a>(entry: &'a Value, key: &str) -> &'a str {
    entry.get(key).and_then(Value::as_str).unwrap_or("")
}
