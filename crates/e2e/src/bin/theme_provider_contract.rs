use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use skwd_wall_core::{WallState, config::Config, theme::material, theme_provider, theme_sink};

fn usage() -> ! {
    eprintln!(
        "usage: theme-provider-contract publish [SEED] | normalize PROVIDER INPUT OUTPUT | preview-cycle PROVIDER IMAGE OUTPUT"
    );
    std::process::exit(2);
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|err| err.to_string())?;
    skwd_wall_core::paths::atomic_write(path, &bytes).map_err(|err| err.to_string())
}

fn publish(seed: &str) -> Result<(), String> {
    let document = material::document_with(seed, true, "tonal-spot")
        .ok_or_else(|| format!("invalid seed: {seed}"))?;
    let config = Config::from_root(json!({
        "theme": {
            "authority": "skwd",
            "scheme": "tonal-spot",
            "targets": theme_provider::PROVIDERS,
        }
    }));
    theme_provider::publish(&config, &document);
    let output = Path::new(&config.cache_dir()).join("vm-canonical-scheme.json");
    write_json(&output, &document)?;
    println!("{}", output.display());
    Ok(())
}

fn normalize(provider: &str, input: &Path, output: &Path) -> Result<(), String> {
    let bytes = std::fs::read(input).map_err(|err| format!("{}: {err}", input.display()))?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|err| format!("{}: {err}", input.display()))?;
    let canonical = theme_provider::normalize(provider, &value, true)
        .ok_or_else(|| format!("{provider}: incompatible native palette"))?;
    write_json(output, &canonical)
}

fn noctalia_scheme(config: &Config) -> Value {
    let out = Command::new(skwd_wall_core::noctalia::bin(config))
        .args(["msg", "color-scheme-get"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    match out {
        Ok(out) if out.status.success() => {
            Value::String(String::from_utf8_lossy(&out.stdout).trim().to_string())
        }
        _ => Value::Null,
    }
}

fn observe(state: &WallState, provider: &str) -> Value {
    let config = state.config().clone();
    let native = theme_provider::provider_path(provider)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let mut observation = json!({ "native": native });
    if provider == "noctalia" {
        let hover = skwd_wall_core::noctalia::palettes_dir_from(
            skwd_config::env("NOCTALIA_CONFIG_HOME").as_deref(),
            skwd_config::env("XDG_CONFIG_HOME").as_deref(),
            &skwd_config::home(),
        )
        .join(format!("{}.json", skwd_wall_core::noctalia::PREVIEW_SCHEME));
        observation["scheme"] = noctalia_scheme(&config);
        observation["hoverPalette"] = json!(hover.is_file());
    }
    observation
}

fn preview_cycle(provider: &str, image: &str, output: &Path) -> Result<(), String> {
    if !theme_provider::PROVIDERS.contains(&provider) {
        return Err(format!("{provider}: unknown provider"));
    }
    let state = WallState::open().map_err(|err| format!("state: {err}"))?;
    let backend = state.config().theme().backend();
    if backend != provider {
        return Err(format!("configured theme backend is {backend}, expected {provider}"));
    }
    let sink = theme_sink::active(provider);
    let before = observe(&state, provider);
    if let Some(arm) = sink.arm {
        arm(&state);
    }
    let generation = state.theme().bump_shell_preview();
    let preview = (sink.preview)(&state, image, generation).map_err(|err| format!("{err:#}"));
    let after = observe(&state, provider);
    state.theme().bump_shell_preview();
    (sink.preview_end)(&state);
    let restored = observe(&state, provider);
    let report = json!({
        "provider": provider,
        "sink": sink.name,
        "image": image,
        "previewError": preview.as_ref().err(),
        "before": before,
        "after": after,
        "restored": restored,
    });
    write_json(output, &report)?;
    preview
}

fn main() {
    let mut args = std::env::args().skip(1);
    let result = match args.next().as_deref() {
        Some("publish") => {
            let seed = args.next().unwrap_or_else(|| "#42ff77".to_string());
            if args.next().is_some() {
                usage();
            }
            publish(&seed)
        }
        Some("normalize") => {
            let provider = args.next().unwrap_or_else(|| usage());
            let input = args.next().unwrap_or_else(|| usage());
            let output = args.next().unwrap_or_else(|| usage());
            if args.next().is_some() {
                usage();
            }
            normalize(&provider, Path::new(&input), Path::new(&output))
        }
        Some("preview-cycle") => {
            let provider = args.next().unwrap_or_else(|| usage());
            let image = args.next().unwrap_or_else(|| usage());
            let output = args.next().unwrap_or_else(|| usage());
            if args.next().is_some() {
                usage();
            }
            preview_cycle(&provider, &image, Path::new(&output))
        }
        _ => usage(),
    };
    if let Err(err) = result {
        eprintln!("theme-provider-contract: {err}");
        std::process::exit(1);
    }
}
