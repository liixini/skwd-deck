use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use wall_proto::rpc;

use super::args::{first_positional, take_flag, take_option};
use super::error::CliError;
use super::pack::{ExportOpts, apply_import, build_pack, bundle_asset, plan_import};
use super::rpc::call;

pub(super) fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() { "look".to_string() } else { trimmed.to_string() }
}

pub(super) fn export(socket: &Path, mut args: Vec<String>) -> Result<(), CliError> {
    let name = take_option(&mut args, &["--name"]).unwrap_or_else(|| "My look".to_string());
    let destination = take_option(&mut args, &["--out"]);
    let include_library = take_flag(&mut args, &["--include-library"]);
    let assets_mode =
        take_option(&mut args, &["--assets"]).unwrap_or_else(|| "reference".to_string());
    if assets_mode != "reference" && assets_mode != "bundle" {
        return Err(CliError::BadArgs("usage: export [--assets reference|bundle]".into()));
    }
    let bundle = assets_mode == "bundle";

    let config = crate::config::Config::load();
    let wallpaper = current_wallpaper_key(socket, &config);
    let options = ExportOpts { name: name.clone(), wallpaper: wallpaper.clone() };
    let mut pack = build_pack(&config, skwd_wall_core::version(), &options);
    let directory =
        PathBuf::from(destination.unwrap_or_else(|| format!("{}.skwdpack", sanitize_name(&name))));

    if bundle && let Some(key) = &wallpaper {
        let (asset, warnings) = bundle_asset(&config, key, &directory.join("assets"));
        for warning in warnings {
            eprintln!("skwd-helm export: {warning}");
        }
        pack.manifest.assets = vec![asset];
    }

    if include_library {
        match skwd_wall_core::db::open() {
            Ok(connection) => {
                pack.library_jsonl = skwd_wall_core::db::export_library(&connection)
                    .map_err(|error| CliError::local(error.to_string()))?;
            }
            Err(error) => {
                eprintln!("skwd-helm export: library unavailable ({error}); skipping");
            }
        }
    }

    skwd_wall_core::pack::write_pack(&directory, &pack)
        .map_err(|error| CliError::local(error.to_string()))?;
    println!("exported look '{name}' to {}", directory.display());
    if let Some(key) = &wallpaper {
        println!("  wallpaper: {key}");
    }
    println!("  look keys: {}", skwd_wall_core::pack::overlay_keys(&pack.overlay).len());
    if include_library {
        println!("  library entries: {}", pack.library_jsonl.lines().count());
    }
    Ok(())
}

pub(super) fn import(socket: &Path, mut args: Vec<String>) -> Result<(), CliError> {
    let replace = take_flag(&mut args, &["--replace"]);
    let _merge = take_flag(&mut args, &["--merge"]);
    let dry_run = take_flag(&mut args, &["--dry-run"]);
    let allow_hooks = take_flag(&mut args, &["--allow-hooks"]);
    let directory = first_positional(&args).ok_or_else(|| {
        CliError::BadArgs("usage: import <DIR> [--replace] [--dry-run] [--allow-hooks]".into())
    })?;

    let pack = skwd_wall_core::pack::read_pack(Path::new(&directory))
        .map_err(|error| CliError::BadArgs(format!("cannot read pack: {error}")))?;
    let (planned, skipped_hooks) = plan_import(&pack.overlay, allow_hooks);
    if dry_run {
        print_import_plan(&pack, &planned, replace, skipped_hooks);
        return Ok(());
    }

    let mut config = crate::config::Config::load();
    let report = apply_import(&mut config, &pack, replace, allow_hooks);
    config.persist().map_err(|error| CliError::local(error.to_string()))?;
    if let Some(palette) = &pack.palette {
        write_palette(&config, palette);
    }
    let library_message = import_library_message(&pack.library_jsonl);
    let _ = call(socket, rpc::WALL_RETHEME, &json!({}));
    let _ = call(socket, rpc::WALL_RELOAD_WE, &json!({}));

    println!(
        "imported look '{}': applied {} keys, quarantined {} hook(s){library_message}",
        pack.manifest.name,
        report.applied.len(),
        report.skipped_hooks
    );
    Ok(())
}

pub(super) fn asset_present(config: &crate::config::Config, key: &str) -> bool {
    if let Some(relative) = key.strip_prefix("static:") {
        return Path::new(&format!(
            "{}/{}",
            config.wallpaper_dir().trim_end_matches('/'),
            relative
        ))
        .exists();
    }
    if let Some(relative) = key.strip_prefix("video:") {
        return Path::new(&format!("{}/{}", config.video_dir().trim_end_matches('/'), relative))
            .exists();
    }
    false
}

fn current_wallpaper_key(socket: &Path, config: &crate::config::Config) -> Option<String> {
    let response = call(socket, rpc::WALL_OUTPUTS, &json!({})).ok()?;
    let outputs = response.get("outputs").and_then(Value::as_array)?;
    for output in outputs {
        let workshop_id = skwd_config::str_ref(output, "we_id", "");
        if !workshop_id.is_empty() {
            return Some(format!("we:{workshop_id}"));
        }
        let path = skwd_config::str_ref(output, "path", "");
        if !path.is_empty()
            && let Some(key) = skwd_wall_core::paths::key_for_path(
                Path::new(path),
                &config.wallpaper_dir(),
                &config.video_dir(),
            )
        {
            return Some(key);
        }
    }
    None
}

fn print_import_plan(
    pack: &skwd_wall_core::pack::Pack,
    planned: &[(String, Value)],
    replace: bool,
    skipped_hooks: usize,
) {
    println!("pack '{}' by {}", pack.manifest.name, pack.manifest.created_by);
    if let Some(key) = &pack.manifest.wallpaper {
        println!("wallpaper: {key}");
    }
    println!(
        "would apply {} look keys ({} mode):",
        planned.len(),
        if replace { "replace" } else { "merge" }
    );
    for (key, value) in planned {
        println!("  {key} = {value}");
    }
    if skipped_hooks > 0 {
        println!(
            "quarantined {skipped_hooks} reload hook(s); re-run with --allow-hooks to install"
        );
    }
    let config = crate::config::Config::load();
    for asset in std::iter::once(pack.manifest.wallpaper.clone()).flatten() {
        let present = asset_present(&config, &asset);
        println!(
            "asset {asset}: {}",
            if present { "present" } else { "MISSING (re-download or skip)" }
        );
    }
    if !pack.library_jsonl.is_empty() {
        println!(
            "library sidecar: {} entries (not applied in dry-run)",
            pack.library_jsonl.lines().count()
        );
    }
}

fn write_palette(config: &crate::config::Config, palette: &Value) {
    let path = PathBuf::from(config.cache_dir()).join("skwd-colors.json");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(palette) {
        let _ = std::fs::write(&path, format!("{text}\n"));
    }
}

fn import_library_message(json_lines: &str) -> String {
    if json_lines.is_empty() {
        return String::new();
    }
    let connection = match skwd_wall_core::db::open() {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("skwd-helm import: library unavailable ({error})");
            return String::new();
        }
    };
    match skwd_wall_core::db::import_library(&connection, json_lines) {
        Ok(stats) => format!(
            "; library matched {} missing {} effects {}",
            stats.matched, stats.missing, stats.effects
        ),
        Err(error) => {
            eprintln!("skwd-helm import: library import failed: {error}");
            String::new()
        }
    }
}
