use std::os::unix::fs::OpenOptionsExt;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

static AGENT: OnceLock<ureq::Agent> = OnceLock::new();

pub fn agent() -> &'static ureq::Agent {
    #[cfg(debug_assertions)]
    assert_blocking_http_context();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout_connect(CONNECT_TIMEOUT)
            .timeout_read(READ_TIMEOUT)
            .build()
    })
}

#[cfg(debug_assertions)]
fn assert_blocking_http_context() {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let drives_async =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle.block_on(async {})))
            .is_err();
    assert!(!drives_async, "ureq on tokio worker");
}

pub fn send(request: ureq::Request, provider: &str) -> anyhow::Result<ureq::Response> {
    request.call().map_err(|error| match error {
        ureq::Error::Status(status, _) => {
            anyhow::anyhow!("{provider} request failed: HTTP {status}")
        }
        ureq::Error::Transport(_) => anyhow::anyhow!("{provider} request failed: network error"),
    })
}

pub fn read_text(request: ureq::Request, provider: &str) -> anyhow::Result<String> {
    Ok(send(request, provider)?.into_string()?)
}

pub const MAX_DOWNLOAD_BYTES: u64 = 500 * 1024 * 1024;
pub const PREVIEW_MAX_ENCODED_BYTES: u64 = 24 * 1024 * 1024;
pub const PREVIEW_MAX_DIMENSION: u32 = 8192;
pub const PREVIEW_MAX_PIXELS: u64 = 16 * 1024 * 1024;
pub const PREVIEW_MAX_DECODE_ALLOC: u64 = 96 * 1024 * 1024;
pub const MAX_REDIRECTS: u32 = 5;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const PREVIEW_DECODE_BYTES_PER_PIXEL: u64 = 4;

static PART_SEQUENCE: AtomicU64 = AtomicU64::new(0);

static NOREDIR_AGENT: OnceLock<ureq::Agent> = OnceLock::new();

fn noredir_agent() -> &'static ureq::Agent {
    #[cfg(debug_assertions)]
    assert_blocking_http_context();
    NOREDIR_AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout_connect(CONNECT_TIMEOUT)
            .timeout_read(READ_TIMEOUT)
            .redirects(0)
            .build()
    })
}

#[cfg(test)]
pub use wall_proto::net::host_is_private;
pub use wall_proto::net::scheme_host;

pub fn require_public(raw: &str) -> anyhow::Result<()> {
    wall_proto::net::require_public(raw).map_err(anyhow::Error::msg)
}

fn wallhaven_host(host: &str) -> bool {
    matches!(host, "wallhaven.cc" | "whvn.cc")
        || host.ends_with(".wallhaven.cc")
        || host.ends_with(".whvn.cc")
}

pub fn require_wallhaven(raw: &str) -> anyhow::Result<()> {
    require_public(raw)?;
    let (_, host) = scheme_host(raw).unwrap_or_default();
    if !wallhaven_host(&host) {
        anyhow::bail!("blocked non-wallhaven host: {host}");
    }
    Ok(())
}

fn source_host_allowed(source: &str, host: &str) -> bool {
    match source {
        "wallhaven" => wallhaven_host(host),
        "unsplash" => host == "unsplash.com" || host.ends_with(".unsplash.com"),
        "pexels" => host == "pexels.com" || host.ends_with(".pexels.com"),
        "youtube" => host.ends_with(".ytimg.com") || host.ends_with(".ggpht.com"),
        "steam" => {
            host == "steamstatic.com"
                || host.ends_with(".steamstatic.com")
                || host == "steamusercontent.com"
                || host.ends_with(".steamusercontent.com")
                || host.ends_with(".akamaihd.net")
        }
        "bing" => host == "bing.com" || host.ends_with(".bing.com"),
        _ => false,
    }
}

pub fn require_source(source: &str, raw: &str) -> anyhow::Result<()> {
    require_public(raw)?;
    let (_, host) = scheme_host(raw).unwrap_or_default();
    if !source_host_allowed(source, &host) {
        anyhow::bail!("blocked non-{source} host: {host}");
    }
    Ok(())
}

pub use wall_proto::net::resolve_redirect;

pub fn get_guarded<F>(url: &str, policy: F) -> anyhow::Result<ureq::Response>
where
    F: Fn(&str) -> anyhow::Result<()>,
{
    let mut current = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        policy(&current)?;
        let request = noredir_agent().get(&current).set("User-Agent", USER_AGENT);
        let resp = send(request, "Remote preview")?;
        if (300..400).contains(&resp.status()) {
            let loc = resp
                .header("Location")
                .ok_or_else(|| anyhow::anyhow!("redirect without Location"))?;
            current = resolve_redirect(&current, loc)
                .ok_or_else(|| anyhow::anyhow!("blocked bad redirect location"))?;
            continue;
        }
        return Ok(resp);
    }
    anyhow::bail!("too many redirects")
}

#[derive(Clone, Copy)]
struct PreviewLimits {
    encoded: u64,
    dimension: u32,
    pixels: u64,
    allocation: u64,
}

const REMOTE_PREVIEW_LIMITS: PreviewLimits = PreviewLimits {
    encoded: PREVIEW_MAX_ENCODED_BYTES,
    dimension: PREVIEW_MAX_DIMENSION,
    pixels: PREVIEW_MAX_PIXELS,
    allocation: PREVIEW_MAX_DECODE_ALLOC,
};

pub(crate) struct PartialFile(Option<std::path::PathBuf>);

impl PartialFile {
    pub(crate) fn commit(mut self) {
        self.0 = None;
    }
}

impl Drop for PartialFile {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub(crate) fn partial_file(
    dest: &std::path::Path,
) -> anyhow::Result<(std::fs::File, std::path::PathBuf, PartialFile)> {
    let parent = dest.parent().unwrap_or_else(|| std::path::Path::new("."));
    let name = dest.file_name().unwrap_or_default().to_string_lossy();
    for _ in 0..8 {
        let serial = PART_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".{name}.{}.{serial}.part", std::process::id()));
        match std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path) {
            Ok(file) => {
                let cleanup = PartialFile(Some(path.clone()));
                return Ok((file, path, cleanup));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    anyhow::bail!("could not allocate a unique partial file beside {}", dest.display())
}

pub fn declared_length(response: &ureq::Response) -> Option<u64> {
    response.header("Content-Length").and_then(|value| value.parse().ok())
}

pub fn require_length_within(response: &ureq::Response, max: u64) -> anyhow::Result<()> {
    if let Some(length) = declared_length(response)
        && length > max
    {
        anyhow::bail!("remote body is {length} bytes; limit is {max} bytes");
    }
    Ok(())
}

pub fn copy_bounded(
    mut reader: impl std::io::Read,
    mut writer: impl std::io::Write,
    max: u64,
) -> anyhow::Result<u64> {
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    let mut copied = 0_u64;
    loop {
        let room = max.saturating_sub(copied);
        let request =
            usize::try_from(room.saturating_add(1)).unwrap_or(usize::MAX).min(buffer.len());
        let read = reader.read(&mut buffer[..request])?;
        if read == 0 {
            return Ok(copied);
        }
        let next = copied
            .checked_add(read as u64)
            .ok_or_else(|| anyhow::anyhow!("remote body size overflow"))?;
        if next > max {
            anyhow::bail!("remote body exceeds {max} byte limit");
        }
        writer.write_all(&buffer[..read])?;
        copied = next;
    }
}

fn validate_preview_dimensions(
    path: &std::path::Path,
    limits: PreviewLimits,
) -> anyhow::Result<()> {
    let encoded = std::fs::metadata(path)?.len();
    if encoded > limits.encoded {
        anyhow::bail!("preview is {encoded} encoded bytes; limit is {}", limits.encoded);
    }

    let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let mut decode_limits = image::Limits::default();
    decode_limits.max_image_width = Some(limits.dimension);
    decode_limits.max_image_height = Some(limits.dimension);
    decode_limits.max_alloc = Some(limits.allocation);
    reader.limits(decode_limits);
    let (width, height) = reader.into_dimensions()?;
    if width == 0 || height == 0 {
        anyhow::bail!("preview has an empty {width}x{height} canvas");
    }
    if width > limits.dimension || height > limits.dimension {
        anyhow::bail!(
            "preview dimensions {width}x{height} exceed {} pixel edge limit",
            limits.dimension
        );
    }
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| anyhow::anyhow!("preview pixel count overflow"))?;
    if pixels > limits.pixels {
        anyhow::bail!("preview has {pixels} pixels; limit is {}", limits.pixels);
    }
    let allocation = pixels
        .checked_mul(PREVIEW_DECODE_BYTES_PER_PIXEL)
        .ok_or_else(|| anyhow::anyhow!("preview allocation estimate overflow"))?;
    if allocation > limits.allocation {
        anyhow::bail!(
            "preview needs at least {allocation} decoded bytes; limit is {}",
            limits.allocation
        );
    }

    Ok(())
}

pub fn validate_cached_preview(path: &std::path::Path) -> anyhow::Result<()> {
    crate::infrastructure::sniff::check_file(path, crate::infrastructure::sniff::Kind::Image)?;
    validate_preview_dimensions(path, REMOTE_PREVIEW_LIMITS)?;
    let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let mut decode_limits = image::Limits::default();
    decode_limits.max_image_width = Some(REMOTE_PREVIEW_LIMITS.dimension);
    decode_limits.max_image_height = Some(REMOTE_PREVIEW_LIMITS.dimension);
    decode_limits.max_alloc = Some(REMOTE_PREVIEW_LIMITS.allocation);
    reader.limits(decode_limits);
    reader.decode()?;
    Ok(())
}

fn stream_to_dest(
    mut reader: impl std::io::Read,
    dest: &std::path::Path,
    want: crate::infrastructure::sniff::Kind,
    limits: PreviewLimits,
) -> anyhow::Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let (mut file, tmp, cleanup) = partial_file(dest)?;
    let copied = copy_bounded(&mut reader, &mut file, limits.encoded);
    drop(file);
    let checked = copied
        .and_then(|_| crate::infrastructure::sniff::check_file(&tmp, want))
        .and_then(|()| validate_preview_dimensions(&tmp, limits));
    checked?;
    std::fs::rename(&tmp, dest)?;
    cleanup.commit();
    Ok(())
}

pub fn fetch_image<F>(url: &str, dest: &std::path::Path, policy: F) -> anyhow::Result<()>
where
    F: Fn(&str) -> anyhow::Result<()>,
{
    let resp = get_guarded(url, policy)?;
    require_length_within(&resp, REMOTE_PREVIEW_LIMITS.encoded)?;
    let reader = resp.into_reader();
    stream_to_dest(reader, dest, crate::infrastructure::sniff::Kind::Image, REMOTE_PREVIEW_LIMITS)
}

mod tests;
