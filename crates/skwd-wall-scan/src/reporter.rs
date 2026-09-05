use std::os::fd::AsRawFd;
use std::path::Path;

use serde_json::{Value, json};
use tokio::io::Interest;
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;

pub(crate) struct Reporter {
    sink: Option<UnboundedSender<Value>>,
    writer: Option<JoinHandle<()>>,
    reader: Option<JoinHandle<()>>,
}

impl Clone for Reporter {
    fn clone(&self) -> Self {
        Self { sink: self.sink.clone(), writer: None, reader: None }
    }
}

impl Reporter {
    pub(crate) async fn connect() -> Self {
        Self::connect_at(&wall_proto::resolve_socket()).await
    }

    pub(crate) async fn connect_at(path: &Path) -> Self {
        let stream = match UnixStream::connect(path).await {
            Ok(stream) => stream,
            Err(error) => {
                log::warn!("reporter connect to {} failed: {error}", path.display());
                return Self { sink: None, writer: None, reader: None };
            }
        };
        let (reader, writer) = stream.into_split();
        let reader = tokio::spawn(drain_responses(reader));
        let (sink, queue) = unbounded_channel();
        let writer = tokio::spawn(write_messages(writer, queue));
        Self { sink: Some(sink), writer: Some(writer), reader: Some(reader) }
    }

    pub(crate) fn send(&self, method: &str, params: &Value) {
        let Some(sink) = &self.sink else {
            return;
        };
        let _ = sink.send(json!({ "method": method, "params": params, "id": 0 }));
    }

    pub(crate) async fn finish(mut self) {
        self.sink = None;
        if let Some(writer) = self.writer.take() {
            let _ = writer.await;
        }
        if let Some(mut reader) = self.reader.take()
            && tokio::time::timeout(std::time::Duration::from_secs(5), &mut reader).await.is_err()
        {
            log::warn!("reporter response wait timed out");
            reader.abort();
        }
    }
}

async fn write_messages(stream: OwnedWriteHalf, mut queue: UnboundedReceiver<Value>) {
    while let Some(message) = queue.recv().await {
        let Ok(mut line) = serde_json::to_vec(&message) else {
            continue;
        };
        line.push(b'\n');
        if let Err(error) = write_all(&stream, &line).await {
            log::warn!("reporter write failed: {error}");
            break;
        }
    }
    unsafe { libc::shutdown(stream.as_ref().as_raw_fd(), libc::SHUT_WR) };
}

async fn write_all(stream: &OwnedWriteHalf, mut bytes: &[u8]) -> std::io::Result<()> {
    while !bytes.is_empty() {
        stream.writable().await?;
        let result = stream.as_ref().try_io(Interest::WRITABLE, || {
            let written = unsafe {
                libc::write(stream.as_ref().as_raw_fd(), bytes.as_ptr().cast(), bytes.len())
            };
            if written < 0 { Err(std::io::Error::last_os_error()) } else { Ok(written as usize) }
        });
        match result {
            Ok(0) => return Err(std::io::ErrorKind::WriteZero.into()),
            Ok(written) => bytes = &bytes[written..],
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
                ) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

async fn drain_responses(stream: OwnedReadHalf) {
    let mut bytes = [0u8; 4096];
    loop {
        if stream.readable().await.is_err() {
            return;
        }
        let result = stream.as_ref().try_io(Interest::READABLE, || {
            let read = unsafe {
                libc::read(stream.as_ref().as_raw_fd(), bytes.as_mut_ptr().cast(), bytes.len())
            };
            if read < 0 { Err(std::io::Error::last_os_error()) } else { Ok(read) }
        });
        match result {
            Ok(0) => return,
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
                ) => {}
            Err(_) => return,
        }
    }
}

mod tests;
