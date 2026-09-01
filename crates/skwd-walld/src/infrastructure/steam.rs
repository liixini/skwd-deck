use serde::Deserialize;

pub const APP_ID: &str = "431960";

pub struct SearchParams {
    pub query: String,
    pub query_type: u32,
    pub days: u32,
    pub tags: Vec<String>,
    pub excluded_tags: Vec<String>,
    pub page: u32,
    pub numperpage: u32,
}

impl Default for SearchParams {
    fn default() -> Self {
        Self {
            query: String::new(),
            query_type: 3,
            days: 7,
            tags: Vec::new(),
            excluded_tags: Vec::new(),
            page: 1,
            numperpage: 30,
        }
    }
}

pub fn query_pairs(params: &SearchParams) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = vec![
        ("appid".into(), APP_ID.into()),
        ("query_type".into(), params.query_type.to_string()),
        ("numperpage".into(), params.numperpage.max(1).to_string()),
        ("page".into(), params.page.max(1).to_string()),
        ("return_previews".into(), "true".into()),
        ("return_tags".into(), "true".into()),
        ("match_all_tags".into(), "true".into()),
    ];
    if !params.query.is_empty() {
        pairs.push(("search_text".into(), params.query.clone()));
    }
    if params.query_type == 3 && params.days > 0 {
        pairs.push(("days".into(), params.days.clamp(1, 7).to_string()));
    }
    for (idx, tag) in params.tags.iter().filter(|tag| !tag.is_empty()).enumerate() {
        pairs.push((format!("requiredtags[{idx}]"), tag.clone()));
    }
    for (idx, tag) in params.excluded_tags.iter().filter(|tag| !tag.is_empty()).enumerate() {
        pairs.push((format!("excludedtags[{idx}]"), tag.clone()));
    }
    pairs
}

#[derive(Deserialize)]
struct ApiTag {
    #[serde(default)]
    tag: String,
}

#[derive(Deserialize)]
struct ApiDetail {
    #[serde(default)]
    publishedfileid: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    preview_url: String,
    #[serde(default)]
    file_size: String,
    #[serde(default)]
    subscriptions: u64,
    #[serde(default)]
    tags: Vec<ApiTag>,
}

#[derive(Deserialize)]
struct ApiResponse {
    #[serde(default)]
    total: u32,
    #[serde(default)]
    publishedfiledetails: Vec<ApiDetail>,
}

#[derive(Deserialize)]
struct ApiRoot {
    response: Option<ApiResponse>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SteamResult {
    pub id: String,
    pub title: String,
    pub preview_url: String,
    pub file_size: u64,
    pub subscriptions: u64,
    pub tags: String,
}

pub struct SearchPage {
    pub results: Vec<SteamResult>,
    pub last_page: u32,
    pub current_page: u32,
}

pub fn parse_search(json: &str, page: u32, numperpage: u32) -> anyhow::Result<SearchPage> {
    let root: ApiRoot = serde_json::from_str(json)?;
    let resp = root.response.ok_or_else(|| anyhow::anyhow!("steam: empty response"))?;
    let results = resp
        .publishedfiledetails
        .into_iter()
        .filter(|det| {
            !det.publishedfileid.is_empty()
                && det.publishedfileid != "0"
                && det.tags.iter().any(|tag| tag.tag == "Scene" || tag.tag == "Video")
        })
        .map(|det| SteamResult {
            id: det.publishedfileid,
            title: det.title,
            preview_url: det.preview_url,
            file_size: det.file_size.parse().unwrap_or(0),
            subscriptions: det.subscriptions,
            tags: det
                .tags
                .into_iter()
                .map(|tag| tag.tag)
                .filter(|tag| !tag.is_empty())
                .collect::<Vec<_>>()
                .join(", "),
        })
        .collect();
    let per = numperpage.max(1);
    Ok(SearchPage {
        results,
        last_page: resp.total.div_ceil(per).max(1),
        current_page: page.max(1),
    })
}

pub fn parse_helper_search(json: &str, page: u32) -> anyhow::Result<SearchPage> {
    #[derive(Deserialize)]
    struct HItem {
        #[serde(default)]
        id: String,
        #[serde(default)]
        title: String,
        #[serde(default)]
        preview_url: String,
        #[serde(default)]
        file_size: u64,
        #[serde(default)]
        subscriptions: u64,
        #[serde(default)]
        tags: String,
    }
    #[derive(Deserialize)]
    struct HRoot {
        #[serde(default)]
        results: Vec<HItem>,
        #[serde(default)]
        error: String,
    }
    let root: HRoot = serde_json::from_str(json)?;
    if !root.error.is_empty() {
        anyhow::bail!("{}", root.error);
    }
    let results: Vec<SteamResult> = root
        .results
        .into_iter()
        .filter(|item| {
            !item.id.is_empty()
                && item.tags.split(',').any(|tag| matches!(tag.trim(), "Scene" | "Video"))
        })
        .map(|item| SteamResult {
            id: item.id,
            title: item.title,
            preview_url: item.preview_url,
            file_size: item.file_size,
            subscriptions: item.subscriptions,
            tags: item.tags,
        })
        .collect();
    let page = page.max(1);
    let last_page = if results.is_empty() { page } else { page + 1 };
    Ok(SearchPage { results, last_page, current_page: page })
}

pub fn search(params: &SearchParams, api_key: &str) -> anyhow::Result<SearchPage> {
    if api_key.is_empty() {
        anyhow::bail!(
            "No Steam Web API key. Add one in Settings > Steam (free: steamcommunity.com/dev/apikey)"
        );
    }
    let mut req = crate::infrastructure::http::agent()
        .get("https://api.steampowered.com/IPublishedFileService/QueryFiles/v1/")
        .set("User-Agent", crate::infrastructure::http::USER_AGENT)
        .query("key", api_key);
    for (key, val) in query_pairs(params) {
        req = req.query(&key, &val);
    }
    let body = crate::infrastructure::http::read_text(req, "Steam Workshop")?;
    parse_search(&body, params.page, params.numperpage)
}

pub fn ext_from_url(url: &str) -> &str {
    url.split(['?', '#'])
        .next()
        .unwrap_or(url)
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, ext)| ext)
        .filter(|ext| ext.len() <= 5 && ext.chars().all(|ch| ch.is_ascii_alphanumeric()))
        .unwrap_or("jpg")
}

pub fn extract_workshop_id(
    line: &str,
    ids: &[String],
    done: &std::collections::HashSet<String>,
) -> Option<String> {
    line.split(|ch: char| !ch.is_ascii_digit())
        .filter(|tok| tok.len() >= 7)
        .rev()
        .map(String::from)
        .find(|id| ids.contains(id) && !done.contains(id))
}

pub fn extract_percent(line: &str) -> Option<f64> {
    let pct = line.find('%')?;
    let before = line[..pct].trim_end();
    let start = before.rfind(|ch: char| !ch.is_ascii_digit() && ch != '.').map_or(0, |idx| idx + 1);
    before[start..].parse::<f64>().ok().map(|val| val / 100.0)
}

pub fn is_auth_error(line: &str) -> bool {
    line.contains("Cached credentials not found") || line.contains("Login Failure")
}

pub fn steamcmd_args(username: &str, install_dir: &str, ids: &[String]) -> Vec<String> {
    let mut args = Vec::new();
    if !install_dir.is_empty() {
        args.push("+force_install_dir".into());
        args.push(install_dir.into());
    }
    args.push("+login".into());
    args.push(if username.is_empty() { "anonymous".into() } else { username.to_string() });
    for id in ids {
        args.push("+workshop_download_item".into());
        args.push(APP_ID.into());
        args.push(id.clone());
    }
    args.push("+quit".into());
    args
}

pub fn downloaded_ids(we_dir: &std::path::Path) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    if let Ok(rd) = std::fs::read_dir(we_dir) {
        for entry in rd.flatten() {
            if entry.path().is_dir() {
                set.insert(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    set
}

mod tests;
