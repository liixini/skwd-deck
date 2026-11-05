mod artifact;
mod cli;
mod gpu;
mod models;
mod plan;
mod plane;
mod prepare;
mod raster;
mod strokes;
mod subject;

use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use skwd_wall_core::infrastructure::drawing::pack::{Device, Pack};

fn execute(models: &models::Models, arguments: &cli::Arguments) -> Result<()> {
    if arguments.check {
        models.check()?;
        prepare::emit(&prepare::READY);
        return Ok(());
    }
    if let Some(map) = &arguments.depth_map {
        return prepare::depth_map(
            models,
            arguments.input.as_ref().context("--input is required")?,
            map,
        );
    }
    let (Some(input), Some(output), Some(source_hash)) =
        (&arguments.input, &arguments.output, &arguments.source_hash)
    else {
        bail!("--input, --output, and --source-sha256 are required");
    };
    prepare::run(models, &prepare::Job { input, output, source_hash })
}

fn run() -> Result<()> {
    let arguments = cli::parse(std::env::args_os().skip(1))?;
    rayon::ThreadPoolBuilder::new().num_threads(models::THREADS).build_global()?;
    let pack = Pack::read(arguments.pack.as_ref().context("--pack is required")?)?;
    let job_output = !arguments.check && arguments.depth_map.is_none();
    let output = arguments.output.as_deref().filter(|_| job_output);
    let vacant = output.is_none_or(prepare::vacant);
    let device = arguments.device.unwrap_or(pack.device);
    let models = models::Models::load(&pack, device)?;
    let Err(error) = execute(&models, &arguments) else {
        return Ok(());
    };
    if !vacant || !models::falls_back(device, models.backend(), &error) {
        return Err(error);
    }
    eprintln!("drawing: {} failed, retrying on cpu: {error:#}", models.backend().label());
    drop(models);
    if let Some(output) = output {
        prepare::clear(output)?;
    }
    execute(&models::Models::load(&pack, Device::Cpu)?, &arguments)
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("drawing preparation failed: {error:#}");
            ExitCode::FAILURE
        }
    }
}
