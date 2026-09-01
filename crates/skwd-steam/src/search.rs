use steamworks::{AppIDs, AppId, Client, UGCQueryType, UGCStatisticType, UGCType};

use crate::callback::pump;
use crate::search_params::SearchParams;

const APP_ID: u32 = 431_960;

fn query_type(code: u64) -> UGCQueryType {
    match code {
        0 => UGCQueryType::RankedByVote,
        1 => UGCQueryType::RankedByPublicationDate,
        9 => UGCQueryType::RankedByTotalUniqueSubscriptions,
        21 => UGCQueryType::RankedByLastUpdatedDate,
        _ => UGCQueryType::RankedByTrend,
    }
}

pub async fn run(client: &Client, params: &SearchParams) -> Result<Vec<serde_json::Value>, String> {
    let wallpaper_engine = AppId(APP_ID);
    let mut handle = client
        .ugc()
        .query_all(
            query_type(params.query_type),
            UGCType::Items,
            AppIDs::Both { creator: wallpaper_engine, consumer: wallpaper_engine },
            params.page,
        )
        .map_err(|error| format!("query_all failed: {error:?}"))?
        .set_return_long_description(false)
        .set_return_metadata(false)
        .set_match_any_tag(false);
    if !params.query.is_empty() {
        handle = handle.set_search_text(&params.query);
    }
    for tag in &params.required_tags {
        if !tag.is_empty() {
            handle = handle.add_required_tag(tag);
        }
    }
    for tag in &params.excluded_tags {
        if !tag.is_empty() {
            handle = handle.add_excluded_tag(tag);
        }
    }
    if params.query_type == 3 {
        handle = handle.set_ranked_by_trend_days(params.trend_days);
    }

    let (sender, receiver) =
        std::sync::mpsc::sync_channel::<Result<Vec<serde_json::Value>, String>>(1);
    handle.fetch(move |result| {
        let mapped = match result {
            Err(error) => Err(format!("fetch failed: {error:?}")),
            Ok(results) => {
                let mut output = Vec::with_capacity(results.returned_results() as usize);
                for index in 0..results.returned_results() {
                    let Some(item) = results.get(index) else {
                        continue;
                    };
                    output.push(serde_json::json!({
                        "id": item.published_file_id.0.to_string(),
                        "title": item.title,
                        "preview_url": results.preview_url(index).unwrap_or_default(),
                        "file_size": u64::from(item.file_size),
                        "subscriptions": results
                            .statistic(index, UGCStatisticType::Subscriptions)
                            .unwrap_or(0),
                        "tags": item.tags.join(", "),
                    }));
                }
                Ok(output)
            }
        };
        let _ = sender.send(mapped);
    });
    pump(|| client.run_callbacks(), &receiver, 20, "search").await?
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
