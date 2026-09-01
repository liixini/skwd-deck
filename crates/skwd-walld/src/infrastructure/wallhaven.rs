use serde::Deserialize;

pub struct SearchParams {
    pub query: String,
    pub categories: String,
    pub purity: String,
    pub sorting: String,
    pub order: String,
    pub top_range: String,
    pub atleast: String,
    pub resolutions: String,
    pub ratios: String,
    pub colors: String,
    pub page: u32,
}

impl Default for SearchParams {
    fn default() -> Self {
        Self {
            query: String::new(),
            categories: "111".into(),
            purity: "100".into(),
            sorting: "toplist".into(),
            order: "desc".into(),
            top_range: "1M".into(),
            atleast: String::new(),
            resolutions: String::new(),
            ratios: String::new(),
            colors: String::new(),
            page: 1,
        }
    }
}

pub fn query_pairs(params: &SearchParams) -> Vec<(&'static str, String)> {
    let mut pairs = Vec::new();
    if !params.query.is_empty() {
        pairs.push(("q", params.query.clone()));
    }
    pairs.push(("categories", params.categories.clone()));
    pairs.push(("purity", params.purity.clone()));
    pairs.push(("sorting", params.sorting.clone()));
    pairs.push(("order", params.order.clone()));
    if params.sorting == "toplist" && !params.top_range.is_empty() {
        pairs.push(("topRange", params.top_range.clone()));
    }
    if !params.resolutions.is_empty() {
        pairs.push(("resolutions", params.resolutions.clone()));
    } else if !params.atleast.is_empty() {
        pairs.push(("atleast", params.atleast.clone()));
    }
    if !params.ratios.is_empty() {
        pairs.push(("ratios", params.ratios.clone()));
    }
    if !params.colors.is_empty() {
        pairs.push(("colors", params.colors.clone()));
    }
    pairs.push(("page", params.page.to_string()));
    pairs
}

#[derive(Deserialize)]
struct ApiThumbs {
    #[serde(default)]
    small: String,
    #[serde(default)]
    large: String,
}

#[derive(Deserialize)]
struct ApiItem {
    id: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    resolution: String,
    #[serde(default)]
    file_size: u64,
    #[serde(default)]
    purity: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    thumbs: Option<ApiThumbs>,
}

#[derive(Deserialize)]
struct ApiMeta {
    last_page: Option<u32>,
    current_page: Option<u32>,
}

#[derive(Deserialize)]
struct ApiResp {
    error: Option<String>,
    data: Option<Vec<ApiItem>>,
    meta: Option<ApiMeta>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WhResult {
    pub id: String,
    pub full_url: String,
    pub thumb_small: String,
    pub thumb_large: String,
    pub resolution: String,
    pub file_size: u64,
    pub purity: String,
    pub category: String,
}

pub struct SearchPage {
    pub results: Vec<WhResult>,
    pub last_page: u32,
    pub current_page: u32,
}

pub fn parse_search(json: &str) -> anyhow::Result<SearchPage> {
    let resp: ApiResp = serde_json::from_str(json)?;
    if let Some(err) = resp.error {
        anyhow::bail!("wallhaven error: {err}");
    }
    let results = resp
        .data
        .unwrap_or_default()
        .into_iter()
        .map(|item| {
            let (thumb_small, thumb_large) = match item.thumbs {
                Some(thumbs) => {
                    let large =
                        if thumbs.large.is_empty() { thumbs.small.clone() } else { thumbs.large };
                    let small = if thumbs.small.is_empty() { large.clone() } else { thumbs.small };
                    (small, large)
                }
                None => (String::new(), String::new()),
            };
            WhResult {
                id: item.id,
                full_url: item.path,
                thumb_small,
                thumb_large,
                resolution: item.resolution,
                file_size: item.file_size,
                purity: item.purity,
                category: item.category,
            }
        })
        .collect();
    let meta = resp.meta;
    Ok(SearchPage {
        results,
        last_page: meta.as_ref().and_then(|meta| meta.last_page).unwrap_or(1),
        current_page: meta.and_then(|meta| meta.current_page).unwrap_or(1),
    })
}

fn parse_wxh(text: &str) -> Option<(u64, u64)> {
    let (w, h) = text.trim().split_once(['x', 'X'])?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

pub fn within_max(resolution: &str, atmost: &str) -> bool {
    let Some((mw, mh)) = parse_wxh(atmost) else {
        return true;
    };
    match parse_wxh(resolution) {
        Some((w, h)) => w <= mw && h <= mh,
        None => true,
    }
}

pub fn search(params: &SearchParams, api_key: &str) -> anyhow::Result<SearchPage> {
    let mut req = crate::infrastructure::http::agent()
        .get("https://wallhaven.cc/api/v1/search")
        .set("User-Agent", crate::infrastructure::http::USER_AGENT);
    for (key, val) in query_pairs(params) {
        req = req.query(key, &val);
    }
    if !api_key.is_empty() {
        req = req.query("apikey", api_key);
    }
    let body = crate::infrastructure::http::read_text(req, "Wallhaven")?;
    parse_search(&body)
}

#[derive(Deserialize)]
struct ApiCollection {
    id: u64,
    #[serde(default)]
    label: String,
    #[serde(default)]
    count: u64,
}

#[derive(Deserialize)]
struct ApiCollections {
    #[serde(default)]
    data: Vec<ApiCollection>,
}

pub struct Collection {
    pub id: u64,
    pub label: String,
    pub count: u64,
}

pub fn parse_collections(body: &str) -> anyhow::Result<Vec<Collection>> {
    let parsed: ApiCollections = serde_json::from_str(body)?;
    Ok(parsed
        .data
        .into_iter()
        .map(|col| Collection { id: col.id, label: col.label, count: col.count })
        .collect())
}

pub fn collections(username: &str, api_key: &str) -> anyhow::Result<Vec<Collection>> {
    let mut req = crate::infrastructure::http::agent()
        .get(&format!("https://wallhaven.cc/api/v1/collections/{username}"))
        .set("User-Agent", crate::infrastructure::http::USER_AGENT);
    if !api_key.is_empty() {
        req = req.query("apikey", api_key);
    }
    let body = crate::infrastructure::http::read_text(req, "Wallhaven")?;
    parse_collections(&body)
}

pub fn collection_page(
    username: &str,
    id: &str,
    page: u32,
    api_key: &str,
) -> anyhow::Result<SearchPage> {
    let mut req = crate::infrastructure::http::agent()
        .get(&format!("https://wallhaven.cc/api/v1/collections/{username}/{id}"))
        .set("User-Agent", crate::infrastructure::http::USER_AGENT)
        .query("page", &page.to_string());
    if !api_key.is_empty() {
        req = req.query("apikey", api_key);
    }
    let body = crate::infrastructure::http::read_text(req, "Wallhaven")?;
    parse_search(&body)
}

pub use crate::infrastructure::sources::ext_from_url;

pub fn library_path(wallpaper_dir: &str, id: &str) -> Option<std::path::PathBuf> {
    crate::infrastructure::sources::library_path(wallpaper_dir, "wallhaven", id)
}

pub fn library_ids(wallpaper_dir: &str) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    if let Ok(rd) = std::fs::read_dir(wallpaper_dir) {
        for entry in rd.filter_map(Result::ok) {
            let name = entry.file_name();
            if let Some(rest) = name.to_string_lossy().strip_prefix("wallhaven-") {
                let id: String = rest.chars().take_while(char::is_ascii_alphanumeric).collect();
                if !id.is_empty() {
                    set.insert(id);
                }
            }
        }
    }
    set
}

use crate::infrastructure::sources::safe_seg;

fn download_destination(full_url: &str, wallpaper_dir: &str, id: &str) -> std::path::PathBuf {
    let ext = safe_seg(ext_from_url(full_url));
    let id = safe_seg(id);
    std::path::Path::new(wallpaper_dir).join(format!("wallhaven-{id}.{ext}"))
}

fn import_preview(preview: &std::path::Path, destination: &std::path::Path) -> anyhow::Result<()> {
    crate::infrastructure::http::validate_cached_preview(preview)?;
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::hard_link(preview, destination).is_ok() {
        return Ok(());
    }
    let mut source = std::fs::File::open(preview)?;
    let (mut file, temporary, cleanup) = crate::infrastructure::http::partial_file(destination)?;
    crate::infrastructure::http::copy_bounded(
        &mut source,
        &mut file,
        crate::infrastructure::http::PREVIEW_MAX_ENCODED_BYTES,
    )?;
    drop(file);
    std::fs::rename(&temporary, destination)?;
    cleanup.commit();
    Ok(())
}

pub fn download(
    full_url: &str,
    wallpaper_dir: &str,
    id: &str,
) -> anyhow::Result<std::path::PathBuf> {
    crate::infrastructure::http::require_wallhaven(full_url)?;
    let ext = ext_from_url(full_url);
    let dest = download_destination(full_url, wallpaper_dir, id);
    let preview = skwd_wall_core::paths::remote_preview("wallhaven-full", id, ext);
    if import_preview(&preview, &dest).is_ok() {
        return Ok(dest);
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let resp = crate::infrastructure::http::get_guarded(
        full_url,
        crate::infrastructure::http::require_wallhaven,
    )?;
    crate::infrastructure::http::require_length_within(
        &resp,
        crate::infrastructure::http::MAX_DOWNLOAD_BYTES,
    )?;
    let mut reader = resp.into_reader();
    let (mut file, tmp, cleanup) = crate::infrastructure::http::partial_file(&dest)?;
    let copied = crate::infrastructure::http::copy_bounded(
        &mut reader,
        &mut file,
        crate::infrastructure::http::MAX_DOWNLOAD_BYTES,
    );
    drop(file);
    let checked = copied.and_then(|_| {
        crate::infrastructure::sniff::check_file(&tmp, crate::infrastructure::sniff::Kind::Image)
    });
    match checked {
        Ok(()) => {
            std::fs::rename(&tmp, &dest)?;
            cleanup.commit();
            Ok(dest)
        }
        Err(err) => Err(err),
    }
}

mod tests;
