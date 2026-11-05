use serde::Serialize;
use steamworks::{AppIDs, AppId, Client, PublishedFileId, UGCQueryType, UGCStatisticType, UGCType};
use wall_proto::sources::{WorkshopDetails, WorkshopPreview};

use crate::callback::pump;
use crate::search_params::SearchParams;

const APP_ID: u32 = 431_960;
const HTTPS_PREFIX: &str = "https://";
const YOUTUBE_ID_LEN: usize = 11;

#[derive(Serialize)]
struct NativeItem {
    id: String,
    title: String,
    preview_url: String,
    file_size: u64,
    subscriptions: u64,
    tags: String,
    workshop: WorkshopDetails,
}

fn query_type(code: u64) -> UGCQueryType {
    match code {
        0 => UGCQueryType::RankedByVote,
        1 => UGCQueryType::RankedByPublicationDate,
        9 => UGCQueryType::RankedByTotalUniqueSubscriptions,
        21 => UGCQueryType::RankedByLastUpdatedDate,
        _ => UGCQueryType::RankedByTrend,
    }
}

fn workshop_preview(value: String, preview_type: u32) -> Option<WorkshopPreview> {
    match preview_type {
        0 if value.starts_with(HTTPS_PREFIX) => Some(WorkshopPreview::Image { url: value }),
        1 if value.len() == YOUTUBE_ID_LEN
            && value.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_')) =>
        {
            Some(WorkshopPreview::Youtube { id: value })
        }
        _ => None,
    }
}

pub async fn run(client: &Client, params: &SearchParams) -> Result<Vec<serde_json::Value>, String> {
    let wallpaper_engine = AppId(APP_ID);
    let exact = params.query.parse::<u64>().ok();
    let handle = if let Some(id) = exact {
        client.ugc().query_item(PublishedFileId(id))
    } else {
        client.ugc().query_all(
            query_type(params.query_type),
            UGCType::Items,
            AppIDs::Both { creator: wallpaper_engine, consumer: wallpaper_engine },
            params.page,
        )
    }
    .map_err(|error| format!("query failed: {error:?}"))?;
    let handle = handle.set_return_long_description(true).set_return_additional_previews(true);
    let handle = if exact.is_some() {
        handle
    } else {
        let mut handle = handle.set_return_metadata(false).set_match_any_tag(false);
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
        handle
    };

    let (sender, receiver) =
        std::sync::mpsc::sync_channel::<Result<Vec<serde_json::Value>, String>>(1);
    let names = client.clone();
    handle.fetch(move |result| {
        let mapped = match result {
            Err(error) => Err(format!("fetch failed: {error:?}")),
            Ok(results) => {
                let mut output = Vec::with_capacity(results.returned_results() as usize);
                for index in 0..results.returned_results() {
                    let Some(item) = results.get(index) else {
                        continue;
                    };
                    if exact.is_some() && item.consumer_app_id != Some(wallpaper_engine) {
                        continue;
                    }
                    let subscriptions = results.statistic(index, UGCStatisticType::Subscriptions);
                    let previews = results
                        .additional_previews(index)
                        .into_iter()
                        .filter_map(|(value, preview_type)| workshop_preview(value, preview_type))
                        .collect();
                    output.push(serde_json::json!(NativeItem {
                        id: item.published_file_id.0.to_string(),
                        title: item.title,
                        preview_url: results.preview_url(index).unwrap_or_default(),
                        file_size: u64::from(item.file_size),
                        subscriptions: subscriptions.unwrap_or_default(),
                        tags: item.tags.join(", "),
                        workshop: WorkshopDetails {
                            description: item.description,
                            creator_id: item.owner.raw().to_string(),
                            creator_name: names.friends().get_friend(item.owner).name(),
                            subscriptions,
                            votes_up: Some(item.num_upvotes),
                            votes_down: Some(item.num_downvotes),
                            time_created: u64::from(item.time_created),
                            time_updated: u64::from(item.time_updated),
                            previews,
                        },
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
