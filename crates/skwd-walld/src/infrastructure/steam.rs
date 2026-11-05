use serde::Deserialize;
use std::io::Read;
use wall_proto::sources::{WorkshopDetails, WorkshopPreview};

pub const APP_ID: &str = "431960";
pub const CLIENT_BACKEND: &str = "steam";
const APP_ID_NUMBER: u32 = 431_960;
const SCENE_TAG: &str = "Scene";
const VIDEO_TAG: &str = "Video";
const USER_AGENT_HEADER: &str = "User-Agent";
const ITEM_COUNT_PARAM: &str = "itemcount";
const ITEM_ID_PARAM: &str = "publishedfileids[0]";
const ONE_ITEM: &str = "1";
const ITEM_DETAILS_URL: &str =
    "https://api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails/v1/";
const WEB_DETAILS_URL: &str = "https://api.steampowered.com/IPublishedFileService/GetDetails/v1/";
const PLAYER_SUMMARIES_URL: &str = "https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v2/";
const WEB_DETAILS_PROVIDER: &str = "Steam Workshop details";
const WEB_DETAILS_TOO_LARGE: &str = "Steam Workshop details are too large";
const WEB_KEY_MISSING: &str = "Steam Web API key is missing";
const API_KEY_PARAM: &str = "key";
const PLAYER_IDS_PARAM: &str = "steamids";
const PLAYER_SUMMARIES_PROVIDER: &str = "Steam creator profile";
const PLAYER_SUMMARIES_TOO_LARGE: &str = "Steam creator profile is too large";
const PLAYER_SUMMARIES_LIMIT: u64 = 128 * 1024;
const PLAYER_NAME_CHARS: usize = 128;
const PLAYER_SUMMARIES_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
const WEB_DETAILS_LIMIT: u64 = 2 * 1024 * 1024;
const WEB_DETAILS_FLAGS: [(&str, &str); 4] = [
    ("includeadditionalpreviews", "true"),
    ("includetags", "true"),
    ("includevotes", "true"),
    ("includemetadata", "true"),
];
const WORKSHOP_PAGE_URL: &str = "https://steamcommunity.com/sharedfiles/filedetails/";
const STEAM_SOURCE: &str = wall_proto::sources::Provider::Steam.key();
const PAGE_ID_PARAM: &str = "id";
const GALLERY_PROVIDER: &str = "Steam Workshop gallery";
const GALLERY_TOO_LARGE: &str = "Steam gallery page is too large";
const GALLERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);
const GALLERY_HTML_LIMIT: u64 = 2 * 1024 * 1024;
const YOUTUBE_ID_LEN: usize = 11;
const GALLERY_CACHE_ITEMS: usize = 64;
const GALLERY_CACHE_LIFETIME: std::time::Duration = std::time::Duration::from_secs(300);
const SCREENSHOTS_START: &str = "var rgFullScreenshotURLs = [";
const SCREENSHOTS_END: &str = "];";
const MOVIES_START: &str = "var rgMovieFlashvars = {";
const MOVIES_END: &str = "var rgCommonFlashVars = {";
const SCREENSHOT_URL_MARKER: &str = "'url': '";
const YOUTUBE_ID_MARKER: &str = "YOUTUBE_VIDEO_ID: \"";
const CREATORS_START: &str = "<div class=\"creatorsBlock\">";
const CREATOR_NAME_START: &str = "<div class=\"friendBlockContent\">";
const CREATOR_NAME_END: &str = "<br";
const HTML_ENTITIES: [(&str, &str); 5] =
    [("&amp;", "&"), ("&lt;", "<"), ("&gt;", ">"), ("&quot;", "\""), ("&#39;", "'")];
const DESCRIPTION_INPUT_CHARS: usize = 16_384;
const DESCRIPTION_CHARS: usize = 8_192;
const BB_TAGS: [&str; 14] = [
    "b", "i", "u", "h1", "h2", "h3", "url", "img", "list", "olist", "code", "quote", "spoiler", "*",
];

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
    const VOTES_FLAG: &str = "return_vote_data";
    const FLAG_ON: &str = "true";
    let mut pairs: Vec<(String, String)> = vec![
        ("appid".into(), APP_ID.into()),
        ("query_type".into(), params.query_type.to_string()),
        ("numperpage".into(), params.numperpage.max(1).to_string()),
        ("page".into(), params.page.max(1).to_string()),
        ("return_previews".into(), "true".into()),
        ("return_tags".into(), "true".into()),
        (VOTES_FLAG.into(), FLAG_ON.into()),
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
    file_size: serde_json::Value,
    #[serde(default, alias = "consumer_appid")]
    consumer_app_id: Option<u32>,
    #[serde(default)]
    result: Option<u32>,
    #[serde(default)]
    subscriptions: Option<u64>,
    #[serde(default)]
    creator: String,
    #[serde(default, alias = "description")]
    file_description: String,
    #[serde(default)]
    time_created: u64,
    #[serde(default)]
    time_updated: u64,
    #[serde(default)]
    vote_data: ApiVotes,
    #[serde(default)]
    votes_up: Option<u32>,
    #[serde(default)]
    votes_down: Option<u32>,
    #[serde(default)]
    previews: Vec<ApiPreview>,
    #[serde(default)]
    tags: Vec<ApiTag>,
}

#[derive(Default, Deserialize)]
struct ApiVotes {
    #[serde(default)]
    votes_up: Option<u32>,
    #[serde(default)]
    votes_down: Option<u32>,
}

#[derive(Deserialize)]
struct ApiPreview {
    #[serde(default)]
    preview_type: u32,
    #[serde(default)]
    url: String,
    #[serde(default)]
    youtubevideoid: String,
}

impl ApiPreview {
    fn into_preview(self) -> Option<WorkshopPreview> {
        match self.preview_type {
            0 if crate::infrastructure::http::require_source(STEAM_SOURCE, &self.url).is_ok() => {
                Some(WorkshopPreview::Image { url: self.url })
            }
            1 if valid_youtube_id(&self.youtubevideoid) => {
                Some(WorkshopPreview::Youtube { id: self.youtubevideoid })
            }
            _ => None,
        }
    }
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

#[derive(Deserialize)]
struct ItemDetailsResponse {
    #[serde(default)]
    publishedfiledetails: Vec<ApiDetail>,
}

#[derive(Deserialize)]
struct ItemDetailsRoot {
    response: Option<ItemDetailsResponse>,
}

#[derive(Deserialize)]
struct PlayerSummariesRoot {
    response: PlayerSummariesResponse,
}

#[derive(Deserialize)]
struct PlayerSummariesResponse {
    players: Vec<PlayerSummary>,
}

#[derive(Deserialize)]
struct PlayerSummary {
    steamid: String,
    personaname: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SteamResult {
    pub id: String,
    pub title: String,
    pub preview_url: String,
    pub file_size: u64,
    pub subscriptions: u64,
    pub tags: String,
    pub workshop: WorkshopDetails,
}

#[derive(Clone)]
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
                && det.tags.iter().any(|tag| tag.tag == SCENE_TAG || tag.tag == VIDEO_TAG)
        })
        .map(result_from_detail)
        .collect();
    let per = numperpage.max(1);
    Ok(SearchPage {
        results,
        last_page: resp.total.div_ceil(per).max(1),
        current_page: page.max(1),
    })
}

fn result_from_detail(det: ApiDetail) -> SteamResult {
    let workshop = WorkshopDetails {
        description: description_text(&det.file_description),
        creator_id: det.creator,
        subscriptions: det.subscriptions,
        votes_up: det.vote_data.votes_up.or(det.votes_up),
        votes_down: det.vote_data.votes_down.or(det.votes_down),
        time_created: det.time_created,
        time_updated: det.time_updated,
        previews: det.previews.into_iter().filter_map(ApiPreview::into_preview).collect(),
        ..WorkshopDetails::default()
    };
    SteamResult {
        id: det.publishedfileid,
        title: det.title,
        preview_url: det.preview_url,
        file_size: det
            .file_size
            .as_u64()
            .or_else(|| det.file_size.as_str()?.parse().ok())
            .unwrap_or(0),
        subscriptions: det.subscriptions.unwrap_or_default(),
        workshop,
        tags: det
            .tags
            .into_iter()
            .map(|tag| tag.tag)
            .filter(|tag| !tag.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
    }
}

fn description_text(raw: &str) -> String {
    let bounded: String = raw.chars().take(DESCRIPTION_INPUT_CHARS).collect();
    let mut output = String::with_capacity(bounded.len());
    let mut rest = bounded.as_str();
    while let Some(start) = rest.find('[') {
        output.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(']') else {
            rest = &rest[start..];
            break;
        };
        let tag = after[..end].trim_start_matches('/').split('=').next().unwrap_or("");
        if BB_TAGS.iter().any(|known| tag.eq_ignore_ascii_case(known)) {
            rest = &after[end + 1..];
        } else {
            output.push('[');
            rest = after;
        }
    }
    output.push_str(rest);
    output.chars().take(DESCRIPTION_CHARS).collect()
}

pub fn parse_item_details(json: &str, id: u64) -> anyhow::Result<SearchPage> {
    let root: ItemDetailsRoot = serde_json::from_str(json)?;
    let response = root.response.ok_or_else(|| anyhow::anyhow!("steam: empty response"))?;
    let results = response
        .publishedfiledetails
        .into_iter()
        .filter(|det| det.publishedfileid == id.to_string())
        .filter(|det| det.result == Some(1))
        .filter(|det| det.consumer_app_id == Some(APP_ID_NUMBER))
        .filter(|det| det.tags.iter().any(|tag| tag.tag == SCENE_TAG || tag.tag == VIDEO_TAG))
        .map(result_from_detail)
        .collect();
    Ok(SearchPage { results, last_page: 1, current_page: 1 })
}

pub fn matches_tags(item: &SteamResult, params: &SearchParams) -> bool {
    let tags: Vec<&str> = item.tags.split(',').map(str::trim).collect();
    params.tags.iter().all(|tag| tags.contains(&tag.as_str()))
        && params.excluded_tags.iter().all(|tag| !tags.contains(&tag.as_str()))
}

pub fn item_details(id: u64) -> anyhow::Result<SearchPage> {
    let id_text = id.to_string();
    let response = crate::infrastructure::http::agent()
        .post(ITEM_DETAILS_URL)
        .set(USER_AGENT_HEADER, crate::infrastructure::http::USER_AGENT)
        .send_form(&[(ITEM_COUNT_PARAM, ONE_ITEM), (ITEM_ID_PARAM, &id_text)])?;
    parse_item_details(&response.into_string()?, id)
}

struct WebDetailsEntry {
    id: u64,
    time: std::time::Instant,
    page: SearchPage,
}

fn parse_creator_name(json: &str, creator_id: &str) -> anyhow::Result<String> {
    let root: PlayerSummariesRoot = serde_json::from_str(json)?;
    Ok(root
        .response
        .players
        .into_iter()
        .find(|player| player.steamid == creator_id)
        .map_or_else(String::new, |player| {
            player.personaname.chars().take(PLAYER_NAME_CHARS).collect()
        }))
}

fn creator_name(creator_id: &str, api_key: &str) -> anyhow::Result<String> {
    let request = crate::infrastructure::http::agent()
        .get(PLAYER_SUMMARIES_URL)
        .set(USER_AGENT_HEADER, crate::infrastructure::http::USER_AGENT)
        .query(API_KEY_PARAM, api_key)
        .query(PLAYER_IDS_PARAM, creator_id)
        .timeout(PLAYER_SUMMARIES_TIMEOUT);
    let response = crate::infrastructure::http::send(request, PLAYER_SUMMARIES_PROVIDER)?;
    let mut body = Vec::new();
    response.into_reader().take(PLAYER_SUMMARIES_LIMIT + 1).read_to_end(&mut body)?;
    anyhow::ensure!(body.len() as u64 <= PLAYER_SUMMARIES_LIMIT, PLAYER_SUMMARIES_TOO_LARGE);
    parse_creator_name(std::str::from_utf8(&body)?, creator_id)
}

pub fn web_item_details(id: u64, api_key: &str) -> anyhow::Result<SearchPage> {
    use std::collections::VecDeque;
    use std::sync::{Mutex, OnceLock};

    static CACHE: OnceLock<Mutex<VecDeque<WebDetailsEntry>>> = OnceLock::new();
    anyhow::ensure!(!api_key.is_empty(), WEB_KEY_MISSING);
    let cache = CACHE.get_or_init(|| Mutex::new(VecDeque::new()));
    if let Some(entry) = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .find(|entry| entry.id == id && entry.time.elapsed() < GALLERY_CACHE_LIFETIME)
    {
        return Ok(entry.page.clone());
    }
    let id_text = id.to_string();
    let mut request = crate::infrastructure::http::agent()
        .get(WEB_DETAILS_URL)
        .set(USER_AGENT_HEADER, crate::infrastructure::http::USER_AGENT)
        .query(API_KEY_PARAM, api_key)
        .query(ITEM_ID_PARAM, &id_text)
        .timeout(GALLERY_TIMEOUT);
    for (name, value) in WEB_DETAILS_FLAGS {
        request = request.query(name, value);
    }
    let response = crate::infrastructure::http::send(request, WEB_DETAILS_PROVIDER)?;
    let mut body = Vec::new();
    response.into_reader().take(WEB_DETAILS_LIMIT + 1).read_to_end(&mut body)?;
    anyhow::ensure!(body.len() as u64 <= WEB_DETAILS_LIMIT, WEB_DETAILS_TOO_LARGE);
    let mut page = parse_item_details(std::str::from_utf8(&body)?, id)?;
    if let Some(item) = page.results.first_mut()
        && !item.workshop.creator_id.is_empty()
    {
        item.workshop.creator_name =
            creator_name(&item.workshop.creator_id, api_key).unwrap_or_default();
    }
    let mut entries = cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    entries.retain(|entry| entry.id != id);
    entries.push_back(WebDetailsEntry { id, time: std::time::Instant::now(), page: page.clone() });
    if entries.len() > GALLERY_CACHE_ITEMS {
        entries.pop_front();
    }
    Ok(page)
}

fn quoted_values<'a>(block: &'a str, marker: &str, end: char) -> impl Iterator<Item = &'a str> {
    block
        .split(marker)
        .skip(1)
        .filter_map(move |fragment| fragment.split_once(end).map(|(value, _)| value))
}

fn valid_youtube_id(id: &str) -> bool {
    id.len() == YOUTUBE_ID_LEN
        && id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

fn gallery_from_html(html: &str) -> anyhow::Result<(String, Vec<WorkshopPreview>)> {
    let name = html
        .split_once(CREATORS_START)
        .and_then(|(_, tail)| tail.split_once(CREATOR_NAME_START))
        .and_then(|(_, tail)| tail.split_once(CREATOR_NAME_END))
        .map_or("", |(name, _)| name)
        .trim();
    let creator_name = HTML_ENTITIES
        .iter()
        .fold(name.to_string(), |name, (encoded, decoded)| name.replace(encoded, decoded));
    let screenshots = html
        .split_once(SCREENSHOTS_START)
        .and_then(|(_, tail)| tail.split_once(SCREENSHOTS_END))
        .ok_or_else(|| anyhow::anyhow!("Steam gallery is unavailable"))?
        .0;
    let movies = html
        .split_once(MOVIES_START)
        .and_then(|(_, tail)| tail.split_once(MOVIES_END))
        .map_or("", |(block, _)| block);
    let images = quoted_values(screenshots, SCREENSHOT_URL_MARKER, '\'')
        .map(|url| url.split_once('?').map_or(url, |(base, _)| base))
        .filter(|url| crate::infrastructure::http::require_source(STEAM_SOURCE, url).is_ok())
        .map(|url| WorkshopPreview::Image { url: url.to_string() });
    let youtube = quoted_values(movies, YOUTUBE_ID_MARKER, '"')
        .filter(|id| valid_youtube_id(id))
        .map(|id| WorkshopPreview::Youtube { id: id.to_string() });
    Ok((creator_name, images.chain(youtube).collect()))
}

struct GalleryEntry {
    id: String,
    time: std::time::Instant,
    creator_name: String,
    previews: Vec<WorkshopPreview>,
}

pub fn gallery(id: &str) -> anyhow::Result<(String, Vec<WorkshopPreview>)> {
    use std::collections::VecDeque;
    use std::sync::{Mutex, OnceLock};

    static CACHE: OnceLock<Mutex<VecDeque<GalleryEntry>>> = OnceLock::new();
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(VecDeque::new()));
    if let Some(entry) = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .find(|entry| entry.id == id && entry.time.elapsed() < GALLERY_CACHE_LIFETIME)
    {
        return Ok((entry.creator_name.clone(), entry.previews.clone()));
    }
    let agent = AGENT.get_or_init(|| ureq::AgentBuilder::new().try_proxy_from_env(true).build());
    let response = crate::infrastructure::http::send(
        agent.get(WORKSHOP_PAGE_URL).query(PAGE_ID_PARAM, id).timeout(GALLERY_TIMEOUT),
        GALLERY_PROVIDER,
    )?;
    let mut body = Vec::new();
    std::io::Read::take(response.into_reader(), GALLERY_HTML_LIMIT + 1).read_to_end(&mut body)?;
    anyhow::ensure!(body.len() as u64 <= GALLERY_HTML_LIMIT, GALLERY_TOO_LARGE);
    let (creator_name, previews) = gallery_from_html(std::str::from_utf8(&body)?)?;
    let mut entries = cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    entries.retain(|entry| entry.id != id);
    entries.push_back(GalleryEntry {
        id: id.to_string(),
        time: std::time::Instant::now(),
        creator_name: creator_name.clone(),
        previews: previews.clone(),
    });
    if entries.len() > GALLERY_CACHE_ITEMS {
        entries.pop_front();
    }
    Ok((creator_name, previews))
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
        #[serde(default)]
        workshop: WorkshopDetails,
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
            workshop: WorkshopDetails {
                description: description_text(&item.workshop.description),
                ..item.workshop
            },
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
            if super::steam_download::presets::validate(&entry.path()).is_ok() {
                set.insert(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    set
}

mod tests;
