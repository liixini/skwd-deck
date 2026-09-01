fn effects_bin() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let cand = dir.join("skwd-wall-effects");
        if cand.exists() {
            return cand;
        }
    }
    std::path::PathBuf::from("skwd-wall-effects")
}

pub(crate) fn effects_list() -> anyhow::Result<serde_json::Value> {
    let out = crate::infrastructure::proc::tool(effects_bin()).arg("list").output()?;
    if !out.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(serde_json::from_slice(&out.stdout)?)
}

pub(crate) fn effect_ids() -> std::collections::HashSet<String> {
    let mut ids = std::collections::HashSet::new();
    if let Ok(list) = effects_list()
        && let Some(arr) = list.as_array()
    {
        for item in arr {
            if let Some(id) = item.get("id").and_then(serde_json::Value::as_str)
                && id != "theme"
            {
                ids.insert(id.to_string());
            }
        }
    }
    ids
}

pub(crate) fn effects_render(
    input: &str,
    effect: &str,
    params: &serde_json::Value,
    output: &std::path::Path,
    max_dim: u32,
    preview: bool,
) -> anyhow::Result<std::path::PathBuf> {
    let effects = serde_json::json!([{ "effect": effect, "params": params }]);
    effects_render_chain(input, &effects, output, max_dim, preview)
}

pub(crate) fn effects_render_chain(
    input: &str,
    effects: &serde_json::Value,
    output: &std::path::Path,
    max_dim: u32,
    preview: bool,
) -> anyhow::Result<std::path::PathBuf> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut cmd = crate::infrastructure::proc::tool(effects_bin());
    cmd.arg("render")
        .arg("--input")
        .arg(input)
        .arg("--effects")
        .arg(effects.to_string())
        .arg("--output")
        .arg(output)
        .arg("--max-dim")
        .arg(max_dim.to_string());
    if preview {
        cmd.arg("--preview");
    }
    let out = cmd.output()?;
    if !out.status.success() {
        anyhow::bail!("effects helper render exited with {}", out.status);
    }
    let written = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if written.is_empty() {
        anyhow::bail!("effects helper render reported no output file");
    }
    Ok(std::path::PathBuf::from(written))
}

fn effects_suffix(effect: &str, params: &serde_json::Value) -> String {
    if effect == "theme"
        && let Some(theme) = params.get("theme").and_then(serde_json::Value::as_str)
    {
        return format!("theme-{}", theme.to_lowercase().replace(' ', "-"));
    }
    effect.to_string()
}

pub(crate) fn requested_effects(
    effect: &str,
    params: &serde_json::Value,
    effects: Option<&serde_json::Value>,
) -> serde_json::Value {
    if let Some(serde_json::Value::Array(steps)) = effects
        && !steps.is_empty()
    {
        return serde_json::Value::Array(steps.iter().take(64).cloned().collect());
    }
    serde_json::json!([{ "effect": effect, "params": params }])
}

fn effect_chain_suffix(effects: &serde_json::Value) -> String {
    let suffix = effects
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|step| {
            let effect = step.get("effect")?.as_str()?;
            Some(effects_suffix(effect, step.get("params").unwrap_or(&serde_json::Value::Null)))
        })
        .collect::<Vec<_>>()
        .join("-");
    if suffix.len() <= 180 {
        return suffix;
    }
    let hash = suffix.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
    });
    let prefix: String = suffix.chars().take(120).collect();
    format!("{prefix}-stack-{hash:016x}")
}

pub(crate) fn effect_tag_label(effect: &str, params: &serde_json::Value) -> String {
    if effect == "theme"
        && let Some(theme) = params.get("theme").and_then(serde_json::Value::as_str)
    {
        return theme.to_lowercase().replace(' ', "-");
    }
    effect.to_lowercase().replace(' ', "-")
}

pub(crate) fn effect_chain_tag_label(effects: &serde_json::Value) -> String {
    effects
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|step| {
            let effect = step.get("effect")?.as_str()?;
            Some(effect_tag_label(effect, step.get("params").unwrap_or(&serde_json::Value::Null)))
        })
        .filter(|label| !label.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

pub(crate) fn effects_preview(input: &str, effects: &serde_json::Value) -> anyhow::Result<String> {
    let stem = std::path::Path::new(input)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("wallpaper");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |dur| dur.as_millis());
    let output =
        skwd_wall_core::paths::cache_dir().join("effects-preview").join(format!("{ts}-{stem}.png"));
    let written =
        effects_render_chain(input, effects, &output, wall_proto::EFFECT_PREVIEW_MAX_EDGE, true)?;
    Ok(written.to_string_lossy().into_owned())
}

pub(crate) fn effects_commit(
    input: &str,
    effects: &serde_json::Value,
    wallpaper_dir: &str,
    video_dir: &str,
) -> anyhow::Result<String> {
    let in_path = std::path::Path::new(input);
    if !within_dir(in_path, std::path::Path::new(wallpaper_dir))
        && !within_dir(in_path, std::path::Path::new(video_dir))
    {
        anyhow::bail!("effects.commit: input path is outside the wallpaper/video dirs");
    }
    let parent = in_path.parent().ok_or_else(|| anyhow::anyhow!("input has no parent dir"))?;
    let stem = in_path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("wallpaper");
    let suffix = effect_chain_suffix(effects);
    if suffix.is_empty() {
        anyhow::bail!("effects.commit: no effects requested");
    }
    let base = parent.join("effects").join(format!("{stem}-{suffix}"));
    let written = effects_render_chain(input, effects, &base, 0, false)?;
    Ok(written.to_string_lossy().into_owned())
}

pub(crate) fn within_dir(path: &std::path::Path, dir: &std::path::Path) -> bool {
    match (path.canonicalize(), dir.canonicalize()) {
        (Ok(path), Ok(dir)) => path.starts_with(&dir),
        _ => false,
    }
}

pub(crate) fn safe_remove_preview(preview: &str) {
    let dir = skwd_wall_core::paths::cache_dir().join("effects-preview");
    let file = std::path::Path::new(preview);
    if file.is_file() && within_dir(file, &dir) {
        let _ = std::fs::remove_file(file);
    }
}

pub(crate) fn sweep_effects_previews(dir: &std::path::Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut count = 0u32;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "png" || ext == "webp" || ext == "jpg")
            && std::fs::remove_file(&path).is_ok()
        {
            count += 1;
        }
    }
    if count > 0 {
        log::info!("swept {count} stale effects previews");
    }
    count
}

mod tests;
