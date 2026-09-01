use serde::Deserialize;

use super::{Attribution, SourcePage, SourceResult};

pub const APP_UTM: &str = "skwd-wall";

#[derive(Default, Deserialize)]
struct ApiUrls {
    #[serde(default)]
    full: String,
    #[serde(default)]
    raw: String,
    #[serde(default)]
    small: String,
    #[serde(default)]
    thumb: String,
}

#[derive(Default, Deserialize)]
struct ApiLinks {
    #[serde(default)]
    download_location: String,
    #[serde(default)]
    html: String,
}

#[derive(Default, Deserialize)]
struct ApiUser {
    #[serde(default)]
    name: String,
    #[serde(default)]
    links: ApiLinks,
}

#[derive(Default, Deserialize)]
struct ApiPhoto {
    #[serde(default)]
    id: String,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
    #[serde(default)]
    urls: ApiUrls,
    #[serde(default)]
    links: ApiLinks,
    #[serde(default)]
    user: ApiUser,
}

#[derive(Default, Deserialize)]
struct ApiResp {
    #[serde(default)]
    total_pages: u32,
    #[serde(default)]
    results: Vec<ApiPhoto>,
    #[serde(default)]
    errors: Vec<String>,
}

pub fn referral_link(profile: &str) -> String {
    if profile.is_empty() {
        return String::new();
    }
    let sep = if profile.contains('?') { '&' } else { '?' };
    format!("{profile}{sep}utm_source={APP_UTM}&utm_medium=referral")
}

pub fn parse_search(json: &str, page: u32) -> anyhow::Result<SourcePage> {
    let resp: ApiResp = serde_json::from_str(json)?;
    if let Some(err) = resp.errors.first() {
        anyhow::bail!("unsplash error: {err}");
    }
    let results = resp
        .results
        .into_iter()
        .filter_map(|photo| {
            let full = if photo.urls.full.is_empty() {
                photo.urls.raw.clone()
            } else {
                photo.urls.full.clone()
            };
            if full.is_empty() {
                return None;
            }
            let thumb = if photo.urls.small.is_empty() {
                photo.urls.thumb.clone()
            } else {
                photo.urls.small.clone()
            };
            let mut res = SourceResult::new(photo.id, full);
            res.thumb_url = thumb;
            res.resolution = format!("{}x{}", photo.width, photo.height);
            res.track_url = photo.links.download_location;
            if !photo.user.name.is_empty() {
                res.attribution = Some(Attribution {
                    text: format!("Photo by {} on Unsplash", photo.user.name),
                    link: referral_link(&photo.user.links.html),
                });
            }
            Some(res)
        })
        .collect();
    Ok(SourcePage {
        results,
        last_page: resp.total_pages.max(1),
        current_page: page.max(1),
        next_cursor: None,
    })
}

pub fn search(
    access_key: &str,
    query: &str,
    page: u32,
    per_page: u32,
    order_by: &str,
    orientation: &str,
    color: &str,
    content_filter: &str,
) -> anyhow::Result<SourcePage> {
    if access_key.is_empty() {
        anyhow::bail!(
            "No Unsplash access key. Add one in Settings > Sources (free: unsplash.com/developers)"
        );
    }
    let query = if query.is_empty() { "wallpaper" } else { query };
    let order_by = if order_by == "latest" { "latest" } else { "relevant" };
    let orientation = match orientation {
        "landscape" | "portrait" | "squarish" => orientation,
        _ => "",
    };
    let color = match color {
        "black_and_white" | "black" | "white" | "yellow" | "orange" | "red" | "purple"
        | "magenta" | "green" | "teal" | "blue" => color,
        _ => "",
    };
    let content_filter = if content_filter == "low" { "low" } else { "high" };
    let mut request = crate::infrastructure::http::agent()
        .get("https://api.unsplash.com/search/photos")
        .set("User-Agent", crate::infrastructure::http::USER_AGENT)
        .set("Authorization", &format!("Client-ID {access_key}"))
        .query("query", query)
        .query("page", &page.max(1).to_string())
        .query("per_page", &per_page.clamp(1, 30).to_string())
        .query("order_by", order_by)
        .query("content_filter", content_filter);
    if !orientation.is_empty() {
        request = request.query("orientation", orientation);
    }
    if !color.is_empty() {
        request = request.query("color", color);
    }
    let body = crate::infrastructure::http::read_text(request, "Unsplash")?;
    parse_search(&body, page)
}

pub fn track_download(track_url: &str, access_key: &str) -> anyhow::Result<()> {
    if track_url.is_empty() || access_key.is_empty() {
        return Ok(());
    }
    crate::infrastructure::http::require_source("unsplash", track_url)?;
    let request = crate::infrastructure::http::agent()
        .get(track_url)
        .set("User-Agent", crate::infrastructure::http::USER_AGENT)
        .set("Authorization", &format!("Client-ID {access_key}"));
    crate::infrastructure::http::send(request, "Unsplash")?;
    Ok(())
}

mod tests;
