use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::Sandbox;

pub struct FakeWayland {
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FakeWayland {
    pub fn start(sandbox: &mut Sandbox, interfaces: &[&str]) -> Self {
        let path = sandbox.root.join("runtime/wayland-test");
        let listener = UnixListener::bind(&path).expect("Wayland fixture socket");
        listener.set_nonblocking(true).unwrap();
        sandbox.set_env("WAYLAND_DISPLAY", &path.to_string_lossy());
        let interfaces: Vec<String> = interfaces.iter().map(|value| (*value).into()).collect();
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopped);
        let thread = std::thread::spawn(move || {
            let mut clients = Vec::new();
            while !stop.load(Ordering::Relaxed) {
                if let Ok((stream, _)) = listener.accept() {
                    let interfaces = interfaces.clone();
                    let stop = Arc::clone(&stop);
                    clients.push(std::thread::spawn(move || serve(stream, &interfaces, &stop)));
                } else {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            for client in clients {
                client.join().unwrap();
            }
        });
        Self { stopped, thread: Some(thread) }
    }
}

impl Drop for FakeWayland {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn serve(mut stream: UnixStream, interfaces: &[String], stop: &AtomicBool) {
    stream.set_nonblocking(true).unwrap();
    let mut pending = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        let mut bytes = [0; 4096];
        match stream.read(&mut bytes) {
            Ok(0) => return,
            Ok(count) => pending.extend_from_slice(&bytes[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(_) => return,
        }
        while pending.len() >= 8 {
            let object = word(&pending[..4]);
            let header = word(&pending[4..8]);
            let size = (header >> 16) as usize;
            if size < 12 || pending.len() < size {
                break;
            }
            let id = word(&pending[8..12]);
            let mut response = Vec::new();
            match (object, header & 0xffff) {
                (1, 1) => {
                    for (index, interface) in interfaces.iter().enumerate() {
                        let mut payload = ((index + 1) as u32).to_ne_bytes().to_vec();
                        payload.extend_from_slice(&((interface.len() + 1) as u32).to_ne_bytes());
                        payload.extend_from_slice(interface.as_bytes());
                        payload.push(0);
                        payload.resize(payload.len().next_multiple_of(4), 0);
                        payload.extend_from_slice(&1u32.to_ne_bytes());
                        event(&mut response, id, 0, &payload);
                    }
                }
                (1, 0) => {
                    event(&mut response, id, 0, &0u32.to_ne_bytes());
                    event(&mut response, 1, 1, &id.to_ne_bytes());
                }
                _ => {}
            }
            if stream.write_all(&response).is_err() {
                return;
            }
            pending.drain(..size);
        }
    }
}

fn word(bytes: &[u8]) -> u32 {
    u32::from_ne_bytes(bytes.try_into().unwrap())
}

fn event(bytes: &mut Vec<u8>, object: u32, opcode: u32, payload: &[u8]) {
    bytes.extend_from_slice(&object.to_ne_bytes());
    bytes.extend_from_slice(&((((payload.len() + 8) as u32) << 16) | opcode).to_ne_bytes());
    bytes.extend_from_slice(payload);
}
