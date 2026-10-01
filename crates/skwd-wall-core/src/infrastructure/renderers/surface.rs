use std::io::Write;

use paper_control::{SourceKind, SurfacePolicy};

use super::RendererSupervisor;
use crate::lock;

fn dynamic_line(surface: SurfacePolicy) -> String {
    let mut command = paper_control::PaperCommand::audio(None, None);
    command.surface = Some(Box::new(surface));
    command.line()
}

fn still_line(surface: SurfacePolicy) -> String {
    let mut command = paper_control::StillCommand::new("");
    command.surface = Some(surface);
    command.line()
}

impl RendererSupervisor {
    pub fn set_surface(&self, surface: impl Fn(SourceKind) -> SurfacePolicy) -> anyhow::Result<()> {
        for kind in [SourceKind::Static, SourceKind::Video, SourceKind::WallpaperEngine] {
            surface(kind).validate()?;
        }
        let still = still_line(surface(SourceKind::Static));
        let video = dynamic_line(surface(SourceKind::Video));
        let scene = dynamic_line(surface(SourceKind::WallpaperEngine));
        let scenes = lock(&self.scene_papers).clone();
        let mut failure = None;
        let mut write = |stdin: &mut std::process::ChildStdin, line: &str| {
            if let Err(error) = stdin.write_all(line.as_bytes()).and_then(|()| stdin.flush()) {
                failure = Some(error);
            }
        };
        if let Some(stdin) = lock(&self.paper_stdin).as_mut() {
            write(stdin, &video);
        }
        if let Some(stdin) = lock(&self.still_stdin).as_mut() {
            write(stdin, &still);
        }
        for (output, (_, stdin)) in lock(&self.video_papers).iter_mut() {
            if let Some(stdin) = stdin {
                write(stdin, if scenes.contains(output) { &scene } else { &video });
            }
        }
        for (_, stdin) in lock(&self.output_stills).values_mut() {
            if let Some(stdin) = stdin {
                write(stdin, &still);
            }
        }
        failure.map_or(Ok(()), |error| Err(error.into()))
    }
}
