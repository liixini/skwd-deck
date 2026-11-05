use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::{BLOCK_EDGE, Mip, RGBA_CHANNELS, mip_shapes, near_bc7, near_size};

pub const GPU_WORKER_FLAG: &str = "--gpu-thumbnail-worker";
pub const GPU_MODE_ENV: &str = "SKWD_SCAN_GPU";
pub const GPU_CPU_ONLY: &str = "0";
pub const GPU_STARTUP_TIMEOUT: Duration = Duration::from_secs(20);
pub const GPU_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
pub const NEAR_BLOCK_BYTES: usize = 307_520;
const REQUEST_MAGIC: &[u8; 4] = b"SGQ1";
const RESPONSE_MAGIC: &[u8; 4] = b"SGR1";
const HEADER_BYTES: usize = 8;
const WORD_BYTES: usize = 4;
const SHAPE_BYTES: usize = WORD_BYTES * 2;
const REQUEST_BYTES: usize = NEAR_BLOCK_BYTES * RGBA_CHANNELS + SHAPE_BYTES * 5;

#[derive(Default)]
pub struct Encoder {
    client: Mutex<Option<Client>>,
}

struct Client {
    file: File,
    timeout: Duration,
}

impl Encoder {
    pub fn from_stream(stream: UnixStream) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self {
            client: Mutex::new(Some(Client {
                file: File::from(OwnedFd::from(stream)),
                timeout: GPU_STARTUP_TIMEOUT,
            })),
        })
    }

    pub fn shutdown(&self) {
        crate::lock(&self.client).take();
    }

    pub(super) fn encode(&self, mips: &[Mip]) -> Vec<u8> {
        let mut session = crate::lock(&self.client);
        if let Some(client) = session.as_mut() {
            match client.encode(mips) {
                Ok(blocks) => return blocks,
                Err(error) => {
                    log::warn!("scan: gpu thumbnail encoding failed: {error}; using cpu");
                    session.take();
                }
            }
        }
        drop(session);
        mips.iter().flat_map(|mip| near_bc7(&mip.rgba, mip.width, mip.height)).collect()
    }
}

impl Client {
    fn encode(&mut self, mips: &[Mip]) -> io::Result<Vec<u8>> {
        let deadline = Instant::now() + self.timeout;
        let mut request = Vec::with_capacity(HEADER_BYTES + REQUEST_BYTES);
        request.extend_from_slice(&header(*REQUEST_MAGIC, REQUEST_BYTES));
        for mip in mips {
            request.extend_from_slice(&mip.width.to_le_bytes());
            request.extend_from_slice(&mip.height.to_le_bytes());
            request.extend_from_slice(&mip.rgba);
        }
        write_deadline(&mut self.file, &request, deadline)?;
        let mut response = [0; HEADER_BYTES];
        read_deadline(&mut self.file, &mut response, deadline)?;
        validate_header(response, *RESPONSE_MAGIC, NEAR_BLOCK_BYTES)?;
        let mut blocks = vec![0; NEAR_BLOCK_BYTES];
        read_deadline(&mut self.file, &mut blocks, deadline)?;
        if blocks.chunks_exact(near_size(BLOCK_EDGE, BLOCK_EDGE)).any(|block| block[0] == 0) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid gpu BC7 mode"));
        }
        self.timeout = GPU_REQUEST_TIMEOUT;
        Ok(blocks)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        unsafe { libc::shutdown(self.file.as_raw_fd(), libc::SHUT_RDWR) };
    }
}

fn header(magic: [u8; WORD_BYTES], size: usize) -> [u8; HEADER_BYTES] {
    let mut bytes = [0; HEADER_BYTES];
    bytes[..WORD_BYTES].copy_from_slice(&magic);
    bytes[WORD_BYTES..].copy_from_slice(&(size as u32).to_le_bytes());
    bytes
}

fn validate_header(
    bytes: [u8; HEADER_BYTES],
    magic: [u8; WORD_BYTES],
    size: usize,
) -> io::Result<()> {
    if bytes != header(magic, size) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid gpu thumbnail header"));
    }
    Ok(())
}

fn wait(file: &File, deadline: Instant, events: libc::c_short) -> io::Result<()> {
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))?;
        let timeout = remaining.as_millis().saturating_add(1).min(i32::MAX as u128) as i32;
        let mut descriptor = libc::pollfd { fd: file.as_raw_fd(), events, revents: 0 };
        let result = unsafe { libc::poll(&raw mut descriptor, 1, timeout) };
        if result > 0 {
            return Ok(());
        }
        if result == 0 {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

fn write_deadline(file: &mut File, bytes: &[u8], deadline: Instant) -> io::Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        wait(file, deadline, libc::POLLOUT)?;
        match file.write(&bytes[offset..]) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(count) => offset += count,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                ) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn read_deadline(file: &mut File, bytes: &mut [u8], deadline: Instant) -> io::Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        wait(file, deadline, libc::POLLIN)?;
        match file.read(&mut bytes[offset..]) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(count) => offset += count,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                ) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

pub fn read_request(file: &mut File) -> io::Result<Option<Vec<Mip>>> {
    let mut bytes = [0; HEADER_BYTES];
    loop {
        match file.read(&mut bytes[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    file.read_exact(&mut bytes[1..])?;
    validate_header(bytes, *REQUEST_MAGIC, REQUEST_BYTES)?;
    let mut mips = Vec::new();
    for (width, height) in mip_shapes() {
        let mut shape = [0; SHAPE_BYTES];
        file.read_exact(&mut shape)?;
        if shape[..WORD_BYTES] != width.to_le_bytes() || shape[WORD_BYTES..] != height.to_le_bytes()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid gpu thumbnail dimensions",
            ));
        }
        let mut rgba = vec![0; width as usize * height as usize * RGBA_CHANNELS];
        file.read_exact(&mut rgba)?;
        mips.push(Mip { width, height, rgba });
    }
    Ok(Some(mips))
}

pub fn write_response(file: &mut File, blocks: &[u8]) -> io::Result<()> {
    if blocks.len() != NEAR_BLOCK_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid gpu thumbnail block count",
        ));
    }
    file.write_all(&header(*RESPONSE_MAGIC, blocks.len()))?;
    file.write_all(blocks)
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
