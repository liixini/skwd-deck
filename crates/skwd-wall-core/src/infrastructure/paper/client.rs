use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde::de::DeserializeOwned;

use crate::config::Config;

use paper_control::{
    ApplyRequest, ApplyResult, CapabilitiesRequest, CapabilitiesResult, PROTOCOL_NAME,
    PROTOCOL_VERSION, Request, RequestParams, Response, ResponseBody, StatusRequest, StatusResult,
    StopRequest, StopResult, decode_ndjson, encode_ndjson,
};

const START_TIMEOUT: Duration = Duration::from_secs(3);
const IO_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_REQUEST: usize = 1024 * 1024;
const MAX_RESPONSE: u64 = 1024 * 1024;
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct PaperClient {
    binary: PathBuf,
    socket: PathBuf,
    start_timeout: Duration,
    io_timeout: Duration,
}

impl PaperClient {
    pub fn configured(config: &Config) -> Self {
        Self::new(config.renderer().paper_bin(), paper_socket_path())
    }

    pub fn new(binary: impl Into<PathBuf>, socket: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            socket: socket.into(),
            start_timeout: START_TIMEOUT,
            io_timeout: IO_TIMEOUT,
        }
    }

    pub fn apply(&self, request: ApplyRequest) -> Result<ApplyResult> {
        request.validate().map_err(|error| anyhow!("Paper {error}"))?;
        self.exchange(RequestParams::Apply(request))
    }

    pub fn stop(&self, outputs: Vec<String>) -> Result<StopResult> {
        let request = StopRequest { outputs };
        request.validate().map_err(|error| anyhow!("Paper {error}"))?;
        self.exchange(RequestParams::Stop(request))
    }

    pub fn status(&self) -> Result<StatusResult> {
        self.exchange(RequestParams::Status(StatusRequest {}))
    }

    pub fn capabilities(&self) -> Result<CapabilitiesResult> {
        let capabilities: CapabilitiesResult =
            self.exchange(RequestParams::Capabilities(CapabilitiesRequest::default()))?;
        if capabilities.protocol != PROTOCOL_NAME || capabilities.version != PROTOCOL_VERSION {
            bail!("unsupported Paper protocol {} v{}", capabilities.protocol, capabilities.version);
        }
        Ok(capabilities)
    }

    fn exchange<T: DeserializeOwned>(&self, params: RequestParams) -> Result<T> {
        let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let request = Request::new(id, params);
        let line = encode_ndjson(&request).context("encode Paper request")?;
        if line.len() > MAX_REQUEST {
            bail!("Paper request exceeds {MAX_REQUEST} bytes");
        }
        let mut stream = self.connect()?;
        stream.set_read_timeout(Some(self.io_timeout)).context("set Paper read timeout")?;
        stream.set_write_timeout(Some(self.io_timeout)).context("set Paper write timeout")?;
        stream.write_all(line.as_bytes()).context("write Paper request")?;

        let mut response = String::new();
        BufReader::new(stream)
            .take(MAX_RESPONSE + 1)
            .read_line(&mut response)
            .context("read Paper response")?;
        if response.is_empty() {
            bail!("Paper closed without a response");
        }
        if response.len() as u64 > MAX_RESPONSE {
            bail!("Paper response exceeds {MAX_RESPONSE} bytes");
        }
        let response: Response<T> = decode_ndjson(&response).context("decode Paper response")?;
        if response.id != id {
            bail!("Paper response id {} does not match request {id}", response.id);
        }
        match response.body {
            ResponseBody::Success { result } => Ok(result),
            ResponseBody::Failure { error } => {
                Err(anyhow!("Paper {}: {}", error.code, error.message))
            }
        }
    }

    fn connect(&self) -> Result<UnixStream> {
        match UnixStream::connect(&self.socket) {
            Ok(stream) => Ok(stream),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                self.start()?;
                self.wait_for_socket()
            }
            Err(error) => Err(error)
                .with_context(|| format!("connect to Paper socket {}", self.socket.display())),
        }
    }

    fn start(&self) -> Result<()> {
        let mut command = Command::new(&self.binary);
        command
            .arg("serve")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        crate::infrastructure::process::spawn_reaped_pid_result(&mut command)
            .with_context(|| format!("start Paper controller {}", self.binary.display()))?;
        Ok(())
    }

    fn wait_for_socket(&self) -> Result<UnixStream> {
        let deadline = Instant::now() + self.start_timeout;
        loop {
            match UnixStream::connect(&self.socket) {
                Ok(stream) => return Ok(stream),
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("Paper controller did not bind {}", self.socket.display())
                    });
                }
            }
        }
    }
}

pub fn paper_socket_path() -> PathBuf {
    if let Some(path) = std::env::var_os("SKWD_PAPER_V2_SOCKET") {
        return PathBuf::from(path);
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(
        || PathBuf::from("/tmp").join(format!("skwd-paper-v2-{}", unsafe { libc::geteuid() })),
        PathBuf::from,
    );
    runtime.join("skwd-paper-v2").join("paper.sock")
}

pub fn socket_at(runtime: &Path) -> PathBuf {
    runtime.join("skwd-paper-v2").join("paper.sock")
}
