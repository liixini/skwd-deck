use std::collections::HashSet;
use std::io::Write;

use serde::Serialize;
use steamworks::{Client, ItemState, PublishedFileId};

use crate::download;
use crate::event_output::{emit, emit_with_folder};
use crate::policy::numeric_ids;
use crate::search;
use crate::search_params::SearchParams;

const APP_ID: u32 = 431_960;
const INVENTORY_COMMAND: &str = "inventory";

#[derive(Serialize)]
struct Inventory {
    account: String,
    subscribed: HashSet<String>,
    installed: std::collections::HashMap<String, String>,
}

pub async fn run(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("search") {
        return run_search_command(args.get(1).map_or("{}", String::as_str)).await;
    }
    if args.first().map(String::as_str) == Some(INVENTORY_COMMAND) {
        return run_inventory(&args[1..]);
    }
    let unsubscribe = args.first().map(String::as_str) == Some("unsub");
    let ids = numeric_ids(if unsubscribe { &args[1..] } else { args });
    if ids.is_empty() {
        eprintln!("usage: skwd-steam <workshop_id>... | unsub <workshop_id>... | search <json>");
        return 2;
    }

    let client: Client = match Client::init_app(APP_ID) {
        Ok(client) => client,
        Err(error) => {
            let message = format!(
                "Steam is not running, or the account does not own Wallpaper Engine ({error})"
            );
            for id in &ids {
                emit(id, "error", 0.0, &message);
            }
            return 1;
        }
    };

    let mut completed = HashSet::new();
    for id in &ids {
        let outcome = if unsubscribe {
            download::unsubscribe(&client, id).await.map(|()| String::new())
        } else {
            download::download(&client, id).await
        };
        match outcome {
            Ok(folder) if unsubscribe => {
                completed.insert(id.clone());
                emit_with_folder(id, "unsubscribed", 1.0, "Unsubscribed", &folder);
            }
            Ok(folder) => {
                completed.insert(id.clone());
                emit_with_folder(id, "done", 1.0, "Download complete", &folder);
            }
            Err(error) => emit(id, "error", 0.0, &error),
        }
    }
    i32::from(completed.len() != ids.len())
}

fn run_inventory(args: &[String]) -> i32 {
    let Ok(client) = Client::init_app(APP_ID) else { return 1 };
    if !client.user().logged_on() {
        return 1;
    }
    let subscribed = client
        .ugc()
        .subscribed_items(true)
        .into_iter()
        .map(|item| item.0.to_string())
        .collect::<HashSet<_>>();
    let installed = args
        .iter()
        .filter_map(|id| id.parse::<u64>().ok().map(|raw| (id, PublishedFileId(raw))))
        .filter(|(_, item)| client.ugc().item_state(*item).contains(ItemState::INSTALLED))
        .filter_map(|(id, item)| {
            client.ugc().item_install_info(item).map(|info| (id.clone(), info.folder))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let inventory =
        Inventory { account: client.user().steam_id().raw().to_string(), subscribed, installed };
    println!("{}", serde_json::to_string(&inventory).expect("inventory serializes"));
    0
}

async fn run_search_command(params_json: &str) -> i32 {
    let value = serde_json::from_str(params_json).unwrap_or(serde_json::Value::Null);
    let client: Client = match Client::init_app(APP_ID) {
        Ok(client) => client,
        Err(error) => {
            println!(
                "{}",
                serde_json::json!({"error": format!(
                    "Steam is not running, or the account does not own Wallpaper Engine ({error})"
                )})
            );
            return 1;
        }
    };
    match search::run(&client, &SearchParams::decode(&value)).await {
        Ok(results) => {
            println!("{}", serde_json::json!({"results": results}));
            let _ = std::io::stdout().flush();
            0
        }
        Err(error) => {
            println!("{}", serde_json::json!({"error": error}));
            1
        }
    }
}
