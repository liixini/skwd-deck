use serde::Deserialize;

use super::{Attribution, SourcePage, SourceResult};

pub const BIN: &str = "yt-dlp";

#[derive(Default, Deserialize)]
struct ApiThumb {
    #[serde(default)]
    url: String,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
}

#[derive(Default, Deserialize)]
struct ApiEntry {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    channel: String,
    #[serde(default)]
    uploader: String,
    #[serde(default)]
    thumbnails: Vec<ApiThumb>,
    #[serde(default)]
    live_status: Option<String>,
}

pub fn watch_url(id: &str) -> String {
    format!("https://www.youtube.com/watch?v={id}")
}

pub fn thumb_for(id: &str) -> String {
    format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg")
}

pub fn preview_for(id: &str) -> String {
    format!("https://i.ytimg.com/vi/{id}/maxresdefault.jpg")
}

pub fn fmt_duration(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    let (hr, min, sec) = (total / 3600, (total % 3600) / 60, total % 60);
    if hr > 0 { format!("{hr}:{min:02}:{sec:02}") } else { format!("{min}:{sec:02}") }
}

pub fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

const OVERSAMPLE: u32 = 4;
const MAX_SEARCH: u32 = 200;
const MAX_ID_LEN: usize = 24;

pub fn raw_search_size(page: u32, per_page: u32, max_secs: u64) -> u32 {
    let end = page.max(1).saturating_mul(per_page.max(1));
    let over = if max_secs > 0 { OVERSAMPLE } else { 1 };
    end.saturating_mul(over).saturating_add(1).min(MAX_SEARCH)
}

pub fn keeps_duration(duration_secs: u64, max_secs: u64) -> bool {
    if max_secs == 0 {
        return true;
    }
    duration_secs > 0 && duration_secs <= max_secs
}

pub fn parse_search(ndjson: &str, page: u32, per_page: u32, max_secs: u64) -> SourcePage {
    let all: Vec<SourceResult> = ndjson
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<ApiEntry>(line).ok())
        .filter(|entry| safe_id(&entry.id))
        .filter(|entry| entry.live_status.as_deref() != Some("is_live"))
        .map(|entry| {
            let mut res = SourceResult::new(entry.id.clone(), watch_url(&entry.id));
            res.thumb_url = entry
                .thumbnails
                .iter()
                .filter(|thumb| !thumb.url.is_empty())
                .max_by_key(|thumb| u64::from(thumb.width) * u64::from(thumb.height))
                .map_or_else(|| thumb_for(&entry.id), |thumb| thumb.url.clone());
            res.title = entry.title;
            res.resolution = entry.duration.map(fmt_duration).unwrap_or_default();
            res.duration_secs = entry.duration.map_or(0, |dur| dur.max(0.0) as u64);
            let who = if entry.channel.is_empty() { entry.uploader } else { entry.channel };
            if !who.is_empty() {
                res.attribution = Some(Attribution { text: who, link: watch_url(&entry.id) });
            }
            res
        })
        .filter(|res| keeps_duration(res.duration_secs, max_secs))
        .collect();
    let per = per_page.max(1) as usize;
    let page = page.max(1);
    let start = (page as usize - 1).saturating_mul(per);
    let has_more = all.len() > start + per;
    let results: Vec<SourceResult> = all.into_iter().skip(start).take(per).collect();
    SourcePage {
        results,
        last_page: if has_more { page + 1 } else { page },
        current_page: page,
        next_cursor: None,
    }
}

pub fn search(query: &str, page: u32, per_page: u32, max_secs: u64) -> anyhow::Result<SourcePage> {
    let query = query.trim();
    let query = if query.is_empty() { "4k wallpaper" } else { query };
    let per = per_page.clamp(1, 50);
    let page = page.max(1);
    let raw = raw_search_size(page, per, max_secs);
    let out = crate::infrastructure::proc::tool(BIN)
        .arg(format!("ytsearch{raw}:{query}"))
        .arg("--flat-playlist")
        .arg("--dump-json")
        .arg("--no-warnings")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output();
    let out = match out {
        Ok(out) => out,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("yt-dlp not found - install it to browse YouTube (e.g. pacman -S yt-dlp)")
        }
        Err(err) => anyhow::bail!("yt-dlp failed to start: {err}"),
    };
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let last = err.lines().rfind(|line| !line.trim().is_empty()).unwrap_or("");
        anyhow::bail!("yt-dlp search failed: {last}");
    }
    Ok(parse_search(&String::from_utf8_lossy(&out.stdout), page, per, max_secs))
}

mod tests;
