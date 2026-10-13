use url::Url;

const STEAM_HOST: &str = "steamcommunity.com";
const STEAM_WWW_HOST: &str = "www.steamcommunity.com";
const STEAM_APP_ID: &str = "431960";
const STEAM_SCHEME: &str = "steam";
const STEAM_URL_HOST: &str = "url";
const STEAM_PAGE: &str = "CommunityFilePage";
const WEB_SCHEMES: [&str; 2] = ["http", "https"];
const WEB_PATHS: [&str; 2] = ["sharedfiles/filedetails", "workshop/filedetails"];
const APP_ID_PARAM: &str = "appid";
const ID_PARAM: &str = "id";

pub fn workshop_id(input: &str) -> Option<u64> {
    let input = input.trim();
    if !input.is_empty() && input.bytes().all(|byte| byte.is_ascii_digit()) {
        return input.parse::<u64>().ok().filter(|id| *id != 0);
    }

    let url = Url::parse(input).ok()?;
    if url.query_pairs().any(|(key, value)| key == APP_ID_PARAM && value != STEAM_APP_ID) {
        return None;
    }
    let id = if url.scheme() == STEAM_SCHEME && url.host_str() == Some(STEAM_URL_HOST) {
        let (page, id) = url.path().trim_matches('/').split_once('/')?;
        if !page.eq_ignore_ascii_case(STEAM_PAGE) {
            return None;
        }
        id
    } else if WEB_SCHEMES.contains(&url.scheme())
        && url.host_str().is_some_and(|host| host == STEAM_HOST || host == STEAM_WWW_HOST)
        && WEB_PATHS.contains(&url.path().trim_matches('/'))
    {
        return url
            .query_pairs()
            .find_map(|(key, value)| (key == ID_PARAM).then(|| value.parse::<u64>().ok()).flatten())
            .filter(|id| *id != 0);
    } else {
        return None;
    };
    if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    id.parse::<u64>().ok().filter(|id| *id != 0)
}

#[cfg(test)]
#[path = "workshop_tests.rs"]
mod tests;
