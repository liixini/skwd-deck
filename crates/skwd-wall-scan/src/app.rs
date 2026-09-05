use skwd_wall_core::{WallState, config::Config, paths};

use crate::cli::{self, Command};
use crate::reporter::Reporter;
use crate::{media_jobs, process, remote_thumbs, sandbox, scan_jobs, theme};

pub(crate) fn run() -> anyhow::Result<()> {
    let arguments: Vec<String> = std::env::args().collect();
    let command = cli::parse(&arguments);

    if matches!(command, Command::Version) {
        println!("skwd-wall-scan {}", skwd_wall_core::version());
        return Ok(());
    }
    process::initialize(&arguments);
    process::arm_deadline(command_deadline(&command))?;
    if let Command::SceneProbe { dir } = &command {
        sandbox::restrict_decode(&sandbox::Policy::new().read(dir))?;
        let root = std::path::Path::new(dir);
        let package = ["scene.pkg", "gifscene.pkg"]
            .iter()
            .map(|name| root.join(name))
            .find(|path| path.is_file())
            .ok_or_else(|| anyhow::anyhow!("no scene package in {}", root.display()))?;
        let package = paper_scene::pkg::Package::open(&package)?;
        let features = paper_scene::scene::extract(&package).map_err(anyhow::Error::msg)?;
        let compatibility = paper_scene::capability::assess_native(&features);
        let reasons: Vec<serde_json::Value> = compatibility
            .gaps
            .iter()
            .map(|gap| {
                serde_json::json!({
                    "code": gap.code(),
                    "description": gap.description(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "full_fidelity": compatibility.full_fidelity(),
                "reasons": reasons,
            })
        );
        return Ok(());
    }

    match command {
        Command::Ansi16 { image, dark, auto, variant } => {
            sandbox::restrict_decode(&sandbox::Policy::new().read(&image))?;
            theme::print_ansi16(&image, dark, auto, &variant);
            return Ok(());
        }
        Command::Tone { image } => {
            sandbox::restrict_decode(&sandbox::Policy::new().read(&image))?;
            theme::print_tone(&image);
            return Ok(());
        }
        Command::Semantic { image, dark, auto } => {
            sandbox::restrict_decode(&sandbox::Policy::new().read(&image))?;
            theme::print_semantic(&image, dark, auto);
            return Ok(());
        }
        Command::ThemePreview { image, dark } => {
            sandbox::restrict_decode(&sandbox::Policy::new().read(&image))?;
            theme::print_preview(&image, dark);
            return Ok(());
        }
        Command::Stream { video } => {
            sandbox::restrict_decode(&sandbox::Policy::new().read(&video).hardware())?;
            media_jobs::stream_video(&video);
            return Ok(());
        }
        Command::StreamPersist => {
            let config = Config::load();
            sandbox::restrict_decode(&library_policy(&config).hardware().cpu_seconds(30 * 60))?;
            media_jobs::stream_persist();
            return Ok(());
        }
        Command::RemoteThumb { source } => {
            return tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(report_remote(&source));
        }
        _ => {}
    }

    let state = WallState::open()?;
    match command {
        Command::SceneAudit { dir } => {
            let root = dir.map_or_else(|| state.config().we_dir(), std::path::PathBuf::from);
            sandbox::restrict_decode(&sandbox::Policy::new().read(&root).cpu_seconds(30 * 60))?;
            let totals = paper_scene::audit::audit_workshop(&root);
            print!("{}", paper_scene::audit::render_markdown(&totals));
            Ok(())
        }
        Command::Theme { image, dark } => {
            sandbox::restrict_decode(&theme_policy(&state.config(), &image))?;
            theme::write_native(&state, &image, dark);
            Ok(())
        }
        command => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(report_jobs(state, command)),
    }
}

async fn report_jobs(state: WallState, command: Command) -> anyhow::Result<()> {
    let reporter = Reporter::connect().await;
    match command {
        Command::Recolor => {
            sandbox::restrict_decode(&storage_policy(&state.config()).cpu_seconds(30 * 60))?;
            let worker = reporter.clone();
            tokio::task::spawn_blocking(move || scan_jobs::recolor(&state, &worker)).await?;
        }
        Command::Preview { key, video } => {
            sandbox::restrict_decode(&storage_policy(&state.config()).read(&video))?;
            let worker = reporter.clone();
            tokio::task::spawn_blocking(move || {
                media_jobs::generate_preview(&key, &video, &worker);
            })
            .await?;
        }
        Command::Paths { changed, request_id } => {
            sandbox::restrict_decode(&scan_policy(&state.config()))?;
            let worker = reporter.clone();
            tokio::task::spawn_blocking(move || {
                scan_jobs::changed_paths(&state, &worker, &changed, request_id.as_deref());
            })
            .await?;
        }
        Command::FullScan { request_id } => {
            sandbox::restrict_decode(&scan_policy(&state.config()))?;
            let worker = reporter.clone();
            tokio::task::spawn_blocking(move || {
                scan_jobs::full_scan(&state, &worker, request_id.as_deref());
            })
            .await?;
        }
        Command::Version
        | Command::Ansi16 { .. }
        | Command::Tone { .. }
        | Command::Semantic { .. }
        | Command::ThemePreview { .. }
        | Command::RemoteThumb { .. }
        | Command::SceneProbe { .. }
        | Command::SceneAudit { .. }
        | Command::Theme { .. }
        | Command::Stream { .. }
        | Command::StreamPersist => unreachable!("handled before entering the runtime"),
    }
    reporter.finish().await;
    Ok(())
}

async fn report_remote(source: &str) -> anyhow::Result<()> {
    let reporter = Reporter::connect().await;
    let result = remote_thumbs::run(source, &reporter).await;
    reporter.finish().await;
    result
}

fn library_policy(config: &Config) -> sandbox::Policy {
    let workshop = config.we_dir();
    let mut policy = sandbox::Policy::new()
        .read(config.wallpaper_dir())
        .read(config.video_dir())
        .read(&workshop);
    for entry in std::fs::read_dir(&workshop).into_iter().flatten().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_symlink()
            && path.is_dir()
            && path.join("project.json").is_file()
            && let Ok(target) = path.canonicalize()
            && target != std::path::Path::new("/")
        {
            policy = policy.read(target);
        }
    }
    policy
}

fn storage_policy(config: &Config) -> sandbox::Policy {
    sandbox::Policy::new()
        .write(config.cache_dir())
        .write(paths::cache_dir())
        .write(paths::data_dir())
}

fn scan_policy(config: &Config) -> sandbox::Policy {
    library_policy(config)
        .write(config.cache_dir())
        .write(paths::cache_dir())
        .write(paths::data_dir())
        .cpu_seconds(30 * 60)
}

fn theme_policy(config: &Config, image: &str) -> sandbox::Policy {
    let template_directory = config.theme().templates_dir();
    let mut policy = storage_policy(config).read(image).read(&template_directory);
    if let Some(parent) = config.theme().native_colors_path().parent() {
        policy = policy.write(parent);
    }
    for (template, output) in config.theme().native_templates() {
        let input = if template.contains('/') {
            std::path::PathBuf::from(config.resolve(&template))
        } else {
            template_directory.join(template)
        };
        let output = if output.contains('/') {
            std::path::PathBuf::from(config.resolve(&output))
        } else {
            std::path::PathBuf::from(config.cache_dir()).join(output)
        };
        policy = policy.read(input);
        if let Some(parent) = output.parent() {
            policy = policy.write(parent);
        }
    }
    policy
}

fn command_deadline(command: &Command) -> std::time::Duration {
    let seconds = match command {
        Command::Preview { .. }
        | Command::Stream { .. }
        | Command::StreamPersist
        | Command::Theme { .. }
        | Command::Ansi16 { .. }
        | Command::Tone { .. }
        | Command::Semantic { .. }
        | Command::ThemePreview { .. }
        | Command::SceneProbe { .. } => 2 * 60,
        Command::RemoteThumb { .. } => 10 * 60,
        Command::Recolor
        | Command::Paths { .. }
        | Command::SceneAudit { .. }
        | Command::FullScan { .. } => 30 * 60,
        Command::Version => 1,
    };
    std::time::Duration::from_secs(seconds)
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
