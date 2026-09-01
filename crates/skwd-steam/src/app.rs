use std::collections::HashSet;
use std::io::Write;

use steamworks::Client;

use crate::download;
use crate::event_output::{emit, emit_with_folder};
use crate::policy::numeric_ids;
use crate::search;
use crate::search_params::SearchParams;

const APP_ID: u32 = 431_960;

pub async fn run(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("search") {
        return run_search_command(args.get(1).map_or("{}", String::as_str)).await;
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
