use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

use serde_json::Value;

use crate::{ErrorInfo, Request, ServerMessage};

pub enum CallError {
    Unreachable,
    Rpc(ErrorInfo),
}

pub struct Conn {
    reader: BufReader<UnixStream>,
    next_id: u64,
}

impl Conn {
    pub fn connect() -> std::io::Result<Self> {
        Self::connect_to(&crate::resolve_socket())
    }

    pub fn connect_to(path: &Path) -> std::io::Result<Self> {
        let stream = UnixStream::connect(path)?;
        Ok(Self { reader: BufReader::new(stream), next_id: 0 })
    }

    pub fn send(&mut self, method: &str, params: &Value) -> std::io::Result<u64> {
        self.next_id += 1;
        let id = self.next_id;
        let req = Request { method: method.to_string(), params: params.clone(), id };
        let mut buf = serde_json::to_vec(&req).map_err(std::io::Error::other)?;
        buf.push(b'\n');
        let stream = self.reader.get_mut();
        stream.write_all(&buf)?;
        stream.flush()?;
        Ok(id)
    }

    pub fn read_raw(&mut self) -> std::io::Result<Option<String>> {
        let mut line = String::new();
        loop {
            line.clear();
            if self.reader.read_line(&mut line)? == 0 {
                return Ok(None);
            }
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                return Ok(Some(trimmed.to_string()));
            }
        }
    }

    pub fn read_message(&mut self) -> std::io::Result<Option<ServerMessage>> {
        loop {
            let Some(raw) = self.read_raw()? else {
                return Ok(None);
            };
            if let Ok(msg) = serde_json::from_str::<ServerMessage>(&raw) {
                return Ok(Some(msg));
            }
        }
    }

    pub fn call(&mut self, method: &str, params: &Value) -> Result<Value, CallError> {
        let id = self.send(method, params).map_err(|_| CallError::Unreachable)?;
        loop {
            match self.read_message() {
                Ok(Some(ServerMessage::Response(resp))) if resp.id == id => {
                    if let Some(err) = resp.error {
                        return Err(CallError::Rpc(err));
                    }
                    return Ok(resp.result.unwrap_or(Value::Null));
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => return Err(CallError::Unreachable),
            }
        }
    }
}
