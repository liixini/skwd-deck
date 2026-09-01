use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageReader};

const JPEG_QUALITY: u8 = 95;
const WEBP_QUALITY: f32 = 95.0;

pub(crate) fn webp_is_lossless(header: &[u8]) -> bool {
    if header.len() < 16 || &header[0..4] != b"RIFF" || &header[8..12] != b"WEBP" {
        return false;
    }
    match &header[12..16] {
        b"VP8L" => true,
        b"VP8X" => header
            .windows(4)
            .skip(20)
            .find(|chunk| *chunk == b"VP8L" || *chunk == b"VP8 ")
            .is_some_and(|chunk| chunk == b"VP8L"),
        _ => false,
    }
}

pub(crate) fn source_is_lossless_webp(input: &Path) -> bool {
    use std::io::Read;

    let Ok(mut file) = std::fs::File::open(input) else {
        return false;
    };
    let mut header = [0; 64];
    let read = file.read(&mut header).unwrap_or(0);
    webp_is_lossless(&header[..read])
}

pub(crate) fn has_alpha(image: &DynamicImage) -> bool {
    match image {
        DynamicImage::ImageRgba8(buffer) => buffer.pixels().any(|pixel| pixel.0[3] < u8::MAX),
        DynamicImage::ImageLumaA8(_)
        | DynamicImage::ImageLumaA16(_)
        | DynamicImage::ImageRgba16(_)
        | DynamicImage::ImageRgba32F(_) => true,
        _ => false,
    }
}

pub(crate) fn target_ext(input: &Path, alpha: bool) -> &'static str {
    let extension = input
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "webp" => "webp",
        "jpg" | "jpeg" if !alpha => "jpg",
        _ => "png",
    }
}

fn write_jpeg(image: &DynamicImage, output: &Path) -> anyhow::Result<()> {
    let rgb = image.to_rgb8();
    let file = std::fs::File::create(output)?;
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        JPEG_QUALITY,
    );
    encoder.encode_image(&rgb)?;
    Ok(())
}

fn write_webp(image: &DynamicImage, output: &Path, lossless: bool) -> anyhow::Result<()> {
    let rgba = image.to_rgba8();
    let encoder = webp::Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height());
    let encoded = if lossless { encoder.encode_lossless() } else { encoder.encode(WEBP_QUALITY) };
    Ok(std::fs::write(output, &*encoded)?)
}

pub(crate) fn write_image(
    image: &DynamicImage,
    output: &Path,
    lossless: bool,
) -> anyhow::Result<()> {
    match output.extension().and_then(|extension| extension.to_str()).unwrap_or_default() {
        "jpg" => write_jpeg(image, output),
        "webp" => match write_webp(image, output, lossless) {
            Ok(()) => Ok(()),
            Err(error) => {
                eprintln!("webp encode failed ({error:#}), falling back to the built-in codec");
                Ok(image.save(output)?)
            }
        },
        _ => Ok(image.save(output)?),
    }
}

pub(crate) fn write_fast_png(image: &DynamicImage, output: &Path) -> anyhow::Result<()> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};

    let file = std::fs::File::create(output)?;
    let encoder = PngEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        CompressionType::Fast,
        FilterType::NoFilter,
    );
    image.write_with_encoder(encoder)?;
    Ok(())
}

fn cache_root() -> PathBuf {
    if let Ok(directory) = std::env::var("XDG_CACHE_HOME")
        && !directory.is_empty()
    {
        return PathBuf::from(directory).join("skwd-wall-v2");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".cache").join("skwd-wall-v2");
    }
    std::env::temp_dir().join("skwd-wall-v2")
}

fn working_key(input: &Path, max_dimension: u32) -> Option<String> {
    use std::hash::{Hash, Hasher};

    let metadata = std::fs::metadata(input).ok()?;
    let modified = metadata.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    let canonical = std::fs::canonicalize(input).ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    canonical.to_string_lossy().hash(&mut hasher);
    modified.hash(&mut hasher);
    max_dimension.hash(&mut hasher);
    metadata.len().hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}

fn prune_working(directory: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .collect();
    if files.len() <= keep {
        return;
    }
    files.sort_by_key(|(time, _)| *time);
    for (_, path) in files.iter().take(files.len() - keep) {
        let _ = std::fs::remove_file(path);
    }
}

fn decode_resized(input: &Path, max_dimension: u32) -> anyhow::Result<DynamicImage> {
    let mut image = ImageReader::open(input)?.with_guessed_format()?.decode()?;
    if max_dimension > 0 && (image.width() > max_dimension || image.height() > max_dimension) {
        image = image.resize(max_dimension, max_dimension, image::imageops::FilterType::Triangle);
    }
    Ok(image)
}

pub(crate) fn load_working(input: &Path, max_dimension: u32) -> anyhow::Result<DynamicImage> {
    if max_dimension == 0 {
        return decode_resized(input, 0);
    }
    let Some(key) = working_key(input, max_dimension) else {
        return decode_resized(input, max_dimension);
    };
    let directory = cache_root().join("effects-src");
    let cache = directory.join(format!("{key}.png"));
    if let Ok(reader) = ImageReader::open(&cache)
        && let Ok(guessed) = reader.with_guessed_format()
        && let Ok(image) = guessed.decode()
    {
        return Ok(image);
    }
    let image = decode_resized(input, max_dimension)?;
    if std::fs::create_dir_all(&directory).is_ok() {
        prune_working(&directory, 32);
        let _ = write_fast_png(&image, &cache);
    }
    Ok(image)
}
