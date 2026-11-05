use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
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

const SOCKET_ENV: &str = "SKWD_PAPER_V2_SOCKET";
const SOCKET_FILE: &str = "paper.sock";
const SOCKET_DIRECTORY: &str = "skwd-paper-v2";
const START_TIMEOUT: Duration = Duration::from_secs(3);
const IO_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_REQUEST: usize = 1024 * 1024;
const MAX_RESPONSE: u64 = 1024 * 1024;
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy)]
pub enum Startup {
    Allowed,
    ExistingOnly,
}

#[derive(Debug, Clone)]
pub struct PaperClient {
    binary: PathBuf,
    socket: PathBuf,
    start_timeout: Duration,
    io_timeout: Duration,
}

impl PaperClient {
    pub fn existing() -> Result<Option<Self>> {
        let socket = paper_socket_path();
        Ok(socket.try_exists()?.then(|| Self::new(PathBuf::new(), socket)))
    }

    pub fn configured(config: &Config) -> Self {
        let mut client = Self::new(config.renderer().paper_bin(), paper_socket_path());
        client.io_timeout = IO_TIMEOUT.max(config.renderer().load_timeout() + IO_TIMEOUT);
        client
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
        self.exchange(RequestParams::Apply(request), Startup::Allowed)
    }

    pub fn stop(&self, outputs: Vec<String>) -> Result<StopResult> {
        let request = StopRequest { outputs };
        request.validate().map_err(|error| anyhow!("Paper {error}"))?;
        self.exchange(RequestParams::Stop(request), Startup::Allowed)
    }

    pub fn mirror(&self, target: paper_control::MirrorTarget) -> Result<()> {
        target.validate()?;
        self.exchange::<paper_control::MirrorTarget>(
            RequestParams::Mirror(target),
            Startup::Allowed,
        )?;
        Ok(())
    }

    pub fn set_paused(&self, request: paper_control::PauseRequest) -> Result<()> {
        request.validate()?;
        if !request.outputs.is_empty()
            && !self
                .capabilities(Startup::ExistingOnly)?
                .controls
                .playback
                .contains(&paper_control::PlaybackControl::ScopedPause)
        {
            bail!("Paper does not support scoped playback pause");
        }
        self.exchange::<paper_control::PauseResult>(
            if request.outputs.is_empty() {
                RequestParams::Pause(request)
            } else {
                RequestParams::PlaybackPause(request)
            },
            Startup::ExistingOnly,
        )?;
        Ok(())
    }

    pub fn audio(&self, request: paper_control::AudioSetRequest) -> Result<()> {
        request.validate()?;
        if request.duck.is_some()
            && !self
                .capabilities(Startup::ExistingOnly)?
                .controls
                .playback
                .contains(&paper_control::PlaybackControl::Duck)
        {
            bail!("Paper does not support independent audio ducking");
        }
        self.exchange::<paper_control::AudioSetResult>(
            if request.duck.is_some() {
                RequestParams::PlaybackAudio(request)
            } else {
                RequestParams::AudioSet(request)
            },
            Startup::ExistingOnly,
        )?;
        Ok(())
    }

    pub fn set_surface_overrides(
        &self,
        request: paper_control::SurfaceOverridesRequest,
    ) -> Result<()> {
        request.validate()?;
        let supported = self.capabilities(Startup::ExistingOnly)?.controls.surface_kinds;
        if supported.is_empty() || request.surfaces.keys().any(|kind| !supported.contains(kind)) {
            bail!("Paper does not support the requested per-kind surface effects");
        }
        self.exchange::<paper_control::SurfaceOverridesRequest>(
            RequestParams::SurfaceOverrides(request),
            Startup::ExistingOnly,
        )?;
        Ok(())
    }

    pub fn set_surface(&self, surface: paper_control::SurfacePolicy) -> Result<()> {
        surface.validate()?;
        self.exchange::<paper_control::SurfacePolicy>(
            RequestParams::SurfaceSet(surface),
            Startup::Allowed,
        )?;
        Ok(())
    }

    pub fn set_widgets(&self, scene: paper_control::WidgetScene) -> Result<()> {
        scene.validate()?;
        self.exchange::<serde_json::Value>(
            RequestParams::Widgets(Box::new(scene)),
            Startup::ExistingOnly,
        )?;
        Ok(())
    }

    pub fn status(&self, startup: Startup) -> Result<StatusResult> {
        self.exchange(RequestParams::Status(StatusRequest {}), startup)
    }

    pub fn capabilities(&self, startup: Startup) -> Result<CapabilitiesResult> {
        let capabilities: CapabilitiesResult =
            self.exchange(RequestParams::Capabilities(CapabilitiesRequest::default()), startup)?;
        if capabilities.protocol != PROTOCOL_NAME || capabilities.version != PROTOCOL_VERSION {
            bail!("unsupported Paper protocol {} v{}", capabilities.protocol, capabilities.version);
        }
        Ok(capabilities)
    }

    fn exchange<T: DeserializeOwned>(&self, params: RequestParams, startup: Startup) -> Result<T> {
        let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let request = Request::new(id, params);
        let line = encode_ndjson(&request).context("encode Paper request")?;
        if line.len() > MAX_REQUEST {
            bail!("Paper request exceeds {MAX_REQUEST} bytes");
        }
        let mut stream = match startup {
            Startup::Allowed => self.connect()?,
            Startup::ExistingOnly => UnixStream::connect(&self.socket)?,
        };
        let timeout = match startup {
            Startup::Allowed => self.io_timeout,
            Startup::ExistingOnly => START_TIMEOUT,
        };
        stream.set_read_timeout(Some(timeout)).context("set Paper read timeout")?;
        stream.set_write_timeout(Some(timeout)).context("set Paper write timeout")?;
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
                let controller = self.start()?;
                self.wait_for_socket(controller)
            }
            Err(error) => Err(error)
                .with_context(|| format!("connect to Paper socket {}", self.socket.display())),
        }
    }

    fn start(&self) -> Result<Child> {
        let mut command = Command::new(&self.binary);
        command
            .arg("serve")
            .env(SOCKET_ENV, &self.socket)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        command.spawn().with_context(|| format!("start Paper controller {}", self.binary.display()))
    }

    fn wait_for_socket(&self, mut controller: Child) -> Result<UnixStream> {
        let deadline = Instant::now() + self.start_timeout;
        loop {
            if let Ok(stream) = UnixStream::connect(&self.socket) {
                std::thread::spawn(move || {
                    let _ = controller.wait();
                });
                return Ok(stream);
            }
            if let Some(status) = controller.try_wait().context("poll Paper controller")? {
                if let Ok(stream) = UnixStream::connect(&self.socket) {
                    return Ok(stream);
                }
                bail!(
                    "Paper controller {} exited with {status} before binding {}",
                    self.binary.display(),
                    self.socket.display()
                );
            }
            if Instant::now() >= deadline {
                std::thread::spawn(move || {
                    let _ = controller.wait();
                });
                bail!(
                    "Paper controller {} did not bind {} within {:?}",
                    self.binary.display(),
                    self.socket.display(),
                    self.start_timeout
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[cfg(test)]
fn isolate() {
    static ISOLATE: std::sync::Once = std::sync::Once::new();
    ISOLATE.call_once(|| {
        if std::env::var_os(SOCKET_ENV).is_none() {
            let socket = crate::paths::test_scratch().join(SOCKET_FILE);
            unsafe { std::env::set_var(SOCKET_ENV, socket) };
        }
    });
}

pub fn paper_socket_path() -> PathBuf {
    #[cfg(test)]
    isolate();
    if let Some(path) = std::env::var_os(SOCKET_ENV) {
        return PathBuf::from(path);
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(
        || PathBuf::from("/tmp").join(format!("skwd-paper-v2-{}", unsafe { libc::geteuid() })),
        PathBuf::from,
    );
    runtime.join(SOCKET_DIRECTORY).join(SOCKET_FILE)
}

pub fn socket_at(runtime: &Path) -> PathBuf {
    runtime.join(SOCKET_DIRECTORY).join(SOCKET_FILE)
}
