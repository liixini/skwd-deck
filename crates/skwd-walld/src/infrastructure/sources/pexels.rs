use serde::Deserialize;

use super::{Attribution, SourcePage, SourceResult};

#[derive(Default, Deserialize)]
struct ApiSrc {
    #[serde(default)]
    original: String,
    #[serde(default)]
    large2x: String,
    #[serde(default)]
    large: String,
    #[serde(default)]
    medium: String,
}

#[derive(Default, Deserialize)]
struct ApiPhoto {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
    #[serde(default)]
    url: String,
    #[serde(default)]
    photographer: String,
    #[serde(default)]
    photographer_url: String,
    #[serde(default)]
    src: ApiSrc,
    #[serde(default)]
    alt: String,
}

#[derive(Default, Deserialize)]
struct ApiResp {
    #[serde(default)]
    photos: Vec<ApiPhoto>,
    #[serde(default)]
    total_results: u32,
    #[serde(default)]
    next_page: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

pub fn parse_search(json: &str, page: u32, per_page: u32) -> anyhow::Result<SourcePage> {
    let resp: ApiResp = serde_json::from_str(json)?;
    if let Some(err) = &resp.error {
        anyhow::bail!("pexels error: {err}");
    }
    let results = resp
        .photos
        .into_iter()
        .filter_map(|photo| {
            let full =
                [&photo.src.original, &photo.src.large2x, &photo.src.large, &photo.src.medium]
                    .into_iter()
                    .find(|url| !url.is_empty())
                    .cloned()
                    .unwrap_or_default();
            if full.is_empty() {
                return None;
            }
            let thumb = if photo.src.large.is_empty() {
                photo.src.medium.clone()
            } else {
                photo.src.large.clone()
            };
            let mut res = SourceResult::new(photo.id.to_string(), full);
            res.thumb_url = thumb;
            res.resolution = format!("{}x{}", photo.width, photo.height);
            res.title = photo.alt;
            if !photo.photographer.is_empty() {
                res.attribution = Some(Attribution {
                    text: format!("Photo by {} on Pexels", photo.photographer),
                    link: if photo.url.is_empty() { photo.photographer_url } else { photo.url },
                });
            }
            Some(res)
        })
        .collect();
    let pp = per_page.max(1);
    let last_page = if resp.total_results > 0 {
        resp.total_results.div_ceil(pp).max(1)
    } else if resp.next_page.as_deref().unwrap_or("").is_empty() {
        page.max(1)
    } else {
        page.max(1) + 1
    };
    Ok(SourcePage { results, last_page, current_page: page.max(1), next_cursor: None })
}

pub fn search(
    api_key: &str,
    query: &str,
    page: u32,
    per_page: u32,
    orientation: &str,
    size: &str,
    color: &str,
) -> anyhow::Result<SourcePage> {
    if api_key.is_empty() {
        anyhow::bail!("No Pexels API key. Add one in Settings > Sources (free: pexels.com/api)");
    }
    let page = page.max(1);
    let per_page = per_page.clamp(1, 80);
    let orientation = match orientation {
        "landscape" | "portrait" | "square" => orientation,
        _ => "",
    };
    let size = match size {
        "large" | "medium" | "small" => size,
        _ => "",
    };
    let named_color = matches!(
        color,
        "red"
            | "orange"
            | "yellow"
            | "green"
            | "turquoise"
            | "blue"
            | "violet"
            | "pink"
            | "brown"
            | "black"
            | "gray"
            | "white"
    );
    let hex_color = color.len() == 7
        && color.starts_with('#')
        && color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit);
    let color = if named_color || hex_color { color } else { "" };
    let has_filters = !orientation.is_empty() || !size.is_empty() || !color.is_empty();
    let base = if query.is_empty() && !has_filters {
        crate::infrastructure::http::agent().get("https://api.pexels.com/v1/curated")
    } else {
        crate::infrastructure::http::agent()
            .get("https://api.pexels.com/v1/search")
            .query("query", if query.is_empty() { "wallpaper" } else { query })
    };
    let mut request = base
        .set("User-Agent", crate::infrastructure::http::USER_AGENT)
        .set("Authorization", api_key)
        .query("page", &page.to_string())
        .query("per_page", &per_page.to_string());
    if !orientation.is_empty() {
        request = request.query("orientation", orientation);
    }
    if !size.is_empty() {
        request = request.query("size", size);
    }
    if !color.is_empty() {
        request = request.query("color", color);
    }
    let body = crate::infrastructure::http::read_text(request, "Pexels")?;
    parse_search(&body, page, per_page)
}

mod tests;
