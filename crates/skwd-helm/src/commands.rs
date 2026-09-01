use std::path::Path;

use serde_json::{Value, json};
use wall_proto::rpc;

use super::args::{first_positional, take_flag, take_option};
use super::error::CliError;
use super::rpc::call;
use super::wallpaper::{
    apply_params_for_item, kind_for_path, list_items, matches_filters, now_seed, pick_index,
};

pub(super) fn run(socket: &Path, verb: &str, mut args: Vec<String>) -> Result<(), CliError> {
    match verb {
        "help" => {
            super::entry::print_help();
            Ok(())
        }
        "list" => list(socket, args),
        "current" | "outputs" => outputs(socket, verb, args),
        "apply" => apply(socket, args),
        "retheme" => {
            if !args.is_empty() {
                return Err(CliError::BadArgs("usage: retheme".into()));
            }
            call(socket, rpc::WALL_RETHEME, &json!({}))?;
            println!("rethemed");
            Ok(())
        }
        "random" => random(socket, args),
        "next" | "prev" => playlist_step(socket, verb, args),
        "back" | "forward" => history_step(socket, verb, args),
        "history" => history(socket, args),
        "pause" | "resume" => {
            call(socket, rpc::WALL_SET_PAUSED, &json!({ "paused": verb == "pause" }))?;
            println!("{verb}d");
            Ok(())
        }
        "mute" | "unmute" => {
            let output = take_option(&mut args, &["--output", "-o"]);
            let mut params = json!({ "mute": verb == "mute" });
            if let Some(output) = output {
                params["outputs"] = json!([output]);
            }
            call(socket, rpc::WALL_SET_AUDIO, &params)?;
            println!("{verb}d");
            Ok(())
        }
        "volume" => {
            let output = take_option(&mut args, &["--output", "-o"]);
            let volume: u32 = first_positional(&args)
                .and_then(|argument| argument.parse().ok())
                .ok_or_else(|| CliError::BadArgs("usage: volume <0-100> [--output O]".into()))?;
            let mut params = json!({ "volume": volume.min(100) });
            if let Some(output) = output {
                params["outputs"] = json!([output]);
            }
            call(socket, rpc::WALL_SET_AUDIO, &params)?;
            println!("volume {}", volume.min(100));
            Ok(())
        }
        "favourite" => {
            let enabled = !take_flag(&mut args, &["--off"]);
            let key = first_positional(&args)
                .ok_or_else(|| CliError::BadArgs("usage: favourite <key> [--off]".into()))?;
            call(socket, rpc::WALL_SET_FAVOURITE, &json!({ "key": key, "favourite": enabled }))?;
            println!("{key}: favourite={enabled}");
            Ok(())
        }
        "export" => super::look::export(socket, args),
        "import" => super::look::import(socket, args),
        "watch" => super::watch::run(socket, args),
        "ui" => super::ui::run(&args),
        "hw-probe" => {
            println!("{}", crate::hardware::probe_json());
            Ok(())
        }
        "displays" => {
            displays(args);
            Ok(())
        }
        _ => Err(CliError::BadArgs(format!("unknown verb '{verb}'"))),
    }
}

fn displays(mut args: Vec<String>) {
    let as_json = take_flag(&mut args, &["--json"]);
    let outputs = skwd_wall_core::outputs::enumerate();
    if as_json {
        let values: Vec<Value> = outputs
            .iter()
            .map(|output| {
                let (logical_width, logical_height) = output.logical_size();
                json!({
                    "name": output.name,
                    "width": output.width,
                    "height": output.height,
                    "refresh_mhz": output.refresh_mhz,
                    "logical_width": logical_width,
                    "logical_height": logical_height,
                    "scale": output.scale,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&values).unwrap_or_default());
    } else {
        for output in outputs {
            println!(
                "{}\t{}x{}@{:.2}\tscale={}",
                output.name,
                output.width,
                output.height,
                f64::from(output.refresh_mhz) / 1000.0,
                output.scale
            );
        }
    }
}

fn list(socket: &Path, mut args: Vec<String>) -> Result<(), CliError> {
    let as_json = take_flag(&mut args, &["--json"]);
    let favourites = take_flag(&mut args, &["--favourites", "--favorites"]);
    let response = call(socket, rpc::WALL_LIST, &json!({ "favourites": favourites }))?;
    if as_json {
        print_json(&response);
        return Ok(());
    }
    for item in list_items(&response) {
        println!(
            "{}\t{}\t{}",
            item.key.as_deref().unwrap_or(""),
            item.kind.as_deref().unwrap_or(""),
            item.name.as_deref().unwrap_or("")
        );
    }
    Ok(())
}

fn outputs(socket: &Path, verb: &str, mut args: Vec<String>) -> Result<(), CliError> {
    let as_json = take_flag(&mut args, &["--json"]);
    let response = call(socket, rpc::WALL_OUTPUTS, &json!({}))?;
    if as_json {
        print_json(&response);
        return Ok(());
    }
    for output in wall_proto::outputs_from(&response) {
        if verb == "current" {
            println!("{}\t{}", output.name, output.current);
        } else {
            println!(
                "{}\t{}\tmute={}\tvol={}\t{}",
                output.name, output.kind, output.mute, output.volume, output.current
            );
        }
    }
    Ok(())
}

fn apply(socket: &Path, mut args: Vec<String>) -> Result<(), CliError> {
    let output = take_option(&mut args, &["--output", "-o"]).unwrap_or_else(|| "*".into());
    let mute = take_flag(&mut args, &["--mute"]);
    let volume = take_option(&mut args, &["--volume"]);
    let kind_override = take_option(&mut args, &["--type"]);
    let target = first_positional(&args)
        .ok_or_else(|| CliError::BadArgs("usage: apply <path|key> [--output O]".into()))?;
    let mut params = if Path::new(&target).is_file() {
        let absolute = std::fs::canonicalize(&target)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or(target.clone());
        json!({ "type": kind_for_path(&absolute), "path": absolute, "output": output })
    } else {
        lookup_apply_params(socket, &target, &output)?
    };
    if let Some(kind) = kind_override {
        params["type"] = json!(kind);
    }
    if mute {
        params["mute"] = json!(true);
    }
    if let Some(volume) = volume.and_then(|argument| argument.parse::<u32>().ok()) {
        params["volume"] = json!(volume.min(100));
    }
    call(socket, rpc::WALL_APPLY, &params)?;
    println!("applied: {target}");
    Ok(())
}

fn random(socket: &Path, mut args: Vec<String>) -> Result<(), CliError> {
    let output = take_option(&mut args, &["--output", "-o"]).unwrap_or_else(|| "*".into());
    let favourites = take_flag(&mut args, &["--favourites", "--favorites"]);
    let kind = take_option(&mut args, &["--type"]);
    let tag = take_option(&mut args, &["--tag"]);
    let config = crate::config::Config::load();
    let response = call(socket, rpc::WALL_LIST, &json!({ "favourites": favourites }))?;
    let items = list_items(&response);
    let candidates: Vec<&wall_proto::WallpaperItem> = items
        .iter()
        .filter(|item| matches_filters(item, kind.as_deref(), tag.as_deref()))
        .collect();
    let index = pick_index(candidates.len(), now_seed())
        .ok_or_else(|| CliError::NotFound("no wallpapers match the filter".into()))?;
    let item = candidates[index];
    let params = apply_params_for_item(item, &config.wallpaper_dir(), &config.video_dir(), &output)
        .ok_or_else(|| CliError::NotFound("cannot resolve random wallpaper".into()))?;
    call(socket, rpc::WALL_APPLY, &params)?;
    println!("applied: {}", item.name.as_deref().unwrap_or("?"));
    Ok(())
}

fn lookup_apply_params(socket: &Path, target: &str, output: &str) -> Result<Value, CliError> {
    let config = crate::config::Config::load();
    let response = call(socket, rpc::WALL_LIST, &json!({ "favourites": false }))?;
    let items = list_items(&response);
    let item = items
        .iter()
        .find(|item| item.key.as_deref() == Some(target) || item.name.as_deref() == Some(target))
        .ok_or_else(|| CliError::NotFound(format!("no wallpaper matching '{target}'")))?;
    apply_params_for_item(item, &config.wallpaper_dir(), &config.video_dir(), output)
        .ok_or_else(|| CliError::NotFound(format!("cannot resolve path for '{target}'")))
}

fn playlist_step(socket: &Path, verb: &str, mut args: Vec<String>) -> Result<(), CliError> {
    let output = take_option(&mut args, &["--output", "-o"]).unwrap_or_else(|| "*".into());
    let method = if verb == "next" { rpc::WALL_PLAYLIST_NEXT } else { rpc::WALL_PLAYLIST_PREV };
    let response = call(socket, method, &json!({ "output": output }))?;
    if skwd_config::bool_at(&response, "ok", false) {
        println!("{verb}: {output}");
        Ok(())
    } else {
        Err(CliError::NotFound("no playlist is active for that output".into()))
    }
}

fn history_step(socket: &Path, verb: &str, mut args: Vec<String>) -> Result<(), CliError> {
    let output = take_option(&mut args, &["--output", "-o"]).unwrap_or_else(|| "*".into());
    let method = if verb == "back" { rpc::WALL_HISTORY_BACK } else { rpc::WALL_HISTORY_FORWARD };
    let response = call(socket, method, &json!({ "output": output }))?;
    if skwd_config::bool_at(&response, "ok", false) {
        println!("{verb}: {output}");
        Ok(())
    } else {
        let message = response
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("no history for that output")
            .to_string();
        Err(CliError::NotFound(message))
    }
}

fn history(socket: &Path, mut args: Vec<String>) -> Result<(), CliError> {
    let as_json = take_flag(&mut args, &["--json"]);
    let output = take_option(&mut args, &["--output", "-o"]).unwrap_or_else(|| "*".into());
    let response = call(socket, rpc::WALL_HISTORY_LIST, &json!({ "output": output }))?;
    if as_json {
        print_json(&response);
        return Ok(());
    }
    let Some(outputs) = response.get("outputs").and_then(Value::as_object) else {
        return Ok(());
    };
    for (name, ring) in outputs {
        print_history_ring(name, ring);
    }
    Ok(())
}

fn print_history_ring(name: &str, ring: &Value) {
    let position = skwd_config::u64_ref(ring, "pos", 0);
    for (index, entry) in skwd_config::arr_ref(ring, "entries").iter().enumerate() {
        let marker = if index as u64 == position { "*" } else { " " };
        let kind = skwd_config::str_ref(entry, "ty", "");
        let label = if kind == wall_proto::kind::WE {
            skwd_config::str_ref(entry, "we_id", "")
        } else {
            skwd_config::str_ref(entry, "path", "")
        };
        println!("{name}\t{marker}\t{index}\t{kind}\t{label}");
    }
}

fn print_json(value: &Value) {
    println!("{}", serde_json::to_string_pretty(value).unwrap_or_default());
}
