use std::path::Path;

use serde_json::{Value, json};
use skwd_wall_core::{config::Config, theme::material, theme_provider};

fn usage() -> ! {
    eprintln!("usage: theme-provider-contract publish [SEED] | normalize PROVIDER INPUT OUTPUT");
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
        _ => usage(),
    };
    if let Err(err) = result {
        eprintln!("theme-provider-contract: {err}");
        std::process::exit(1);
    }
}
