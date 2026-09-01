use serde::Deserialize;

use super::{Attribution, SourcePage, SourceResult};

const HOST: &str = "https://www.bing.com";

#[derive(Deserialize)]
struct ApiImage {
    #[serde(default)]
    urlbase: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    startdate: String,
    #[serde(default)]
    copyright: String,
    #[serde(default)]
    copyrightlink: String,
    #[serde(default)]
    title: String,
}

#[derive(Deserialize)]
struct ApiResp {
    #[serde(default)]
    images: Vec<ApiImage>,
}

fn id_from_urlbase(urlbase: &str, startdate: &str) -> String {
    urlbase
        .split_once("id=")
        .map(|(_, rest)| rest.split(['&', '?']).next().unwrap_or(rest))
        .filter(|id| !id.is_empty())
        .unwrap_or(startdate)
        .to_string()
}

fn absolute(path: &str) -> String {
    if path.starts_with("http") { path.to_string() } else { format!("{HOST}{path}") }
}

pub fn parse_search(json: &str) -> anyhow::Result<SourcePage> {
    let resp: ApiResp = serde_json::from_str(json)?;
    let results = resp
        .images
        .into_iter()
        .filter(|img| !img.urlbase.is_empty() || !img.url.is_empty())
        .map(|img| {
            let base = if img.urlbase.is_empty() { img.url.clone() } else { img.urlbase.clone() };
            let full = if img.urlbase.is_empty() {
                absolute(&img.url)
            } else {
                absolute(&format!("{base}_UHD.jpg"))
            };
            let thumb = if img.urlbase.is_empty() {
                absolute(&img.url)
            } else {
                absolute(&format!("{base}_400x240.jpg"))
            };
            let mut res = SourceResult::new(id_from_urlbase(&img.urlbase, &img.startdate), full);
            res.thumb_url = thumb;
            res.resolution = "3840x2160".to_string();
            res.title = img.title;
            if !img.copyright.is_empty() {
                res.attribution =
                    Some(Attribution { text: img.copyright, link: img.copyrightlink });
            }
            res
        })
        .collect();
    Ok(SourcePage { results, last_page: 1, current_page: 1, next_cursor: None })
}

pub fn search(market: &str) -> anyhow::Result<SourcePage> {
    let mkt = if market.is_empty() { "en-US" } else { market };
    let request = crate::infrastructure::http::agent()
        .get(&format!("{HOST}/HPImageArchive.aspx"))
        .set("User-Agent", crate::infrastructure::http::USER_AGENT)
        .query("format", "js")
        .query("idx", "0")
        .query("n", "8")
        .query("mkt", mkt);
    let body = crate::infrastructure::http::read_text(request, "Bing")?;
    parse_search(&body)
}

mod tests;
