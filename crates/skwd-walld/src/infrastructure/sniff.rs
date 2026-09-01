use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    Video,
}

impl std::fmt::Display for Kind {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str(match self {
            Kind::Image => "image",
            Kind::Video => "video",
        })
    }
}

pub const HEAD_LEN: usize = 16;

pub fn detect(head: &[u8]) -> Option<Kind> {
    if head.len() >= 12 {
        if &head[4..8] == b"ftyp" {
            let image = matches!(&head[8..12], b"avif" | b"avis" | b"heic" | b"heix" | b"mif1");
            return Some(if image { Kind::Image } else { Kind::Video });
        }
        if &head[0..4] == b"RIFF" {
            if &head[8..12] == b"WEBP" {
                return Some(Kind::Image);
            }
            if &head[8..11] == b"AVI" {
                return Some(Kind::Video);
            }
            return None;
        }
    }
    if head.starts_with(&[0xFF, 0xD8, 0xFF])
        || head.starts_with(&[0x89, b'P', b'N', b'G'])
        || head.starts_with(b"GIF87a")
        || head.starts_with(b"GIF89a")
        || head.starts_with(b"BM")
    {
        return Some(Kind::Image);
    }
    if head.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return Some(Kind::Video);
    }
    None
}

pub fn describe(head: &[u8]) -> String {
    if head.is_empty() {
        return "an empty file".into();
    }
    let trimmed = head.trim_ascii_start();
    if trimmed.starts_with(b"<") {
        return "an HTML/text page".into();
    }
    if trimmed.starts_with(b"{") || trimmed.starts_with(b"[") {
        return "a JSON document".into();
    }
    let hex: Vec<String> = head.iter().take(8).map(|byte| format!("{byte:02x}")).collect();
    format!("unrecognised data ({})", hex.join(" "))
}

pub fn verify(head: &[u8], want: Kind) -> anyhow::Result<()> {
    match detect(head) {
        Some(kind) if kind == want => Ok(()),
        Some(kind) => anyhow::bail!("expected {want} data but the server sent a {kind}"),
        None => anyhow::bail!("expected {want} data but the server sent {}", describe(head)),
    }
}

pub fn check_file(path: &Path, want: Kind) -> anyhow::Result<()> {
    let mut head = [0u8; HEAD_LEN];
    let mut file = std::fs::File::open(path)?;
    let mut filled = 0;
    loop {
        let got = file.read(&mut head[filled..])?;
        if got == 0 || filled + got == HEAD_LEN {
            filled += got;
            break;
        }
        filled += got;
    }
    verify(&head[..filled], want)
}

mod tests;
