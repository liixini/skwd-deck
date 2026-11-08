use std::io::Write;
use std::process::ChildStdin;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use paper_control::{PaperCommand, StillCommand, WidgetScene};

use super::RendererSupervisor;
use crate::infrastructure::paper::PaperClient;
use crate::lock;

pub trait WidgetSource: Send + Sync {
    fn scene(&self, shown: &[String]) -> WidgetScene;
}

impl RendererSupervisor {
    pub fn set_widget_source(&self, source: Arc<dyn WidgetSource>) {
        *lock(&self.widgets) = Some(source);
    }

    pub fn widget_editor(&self, joined: bool) -> bool {
        if joined {
            return self.widget_editors.fetch_add(1, Ordering::Relaxed) == 0;
        }
        self.widget_editors
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |editors| editors.checked_sub(1))
            .is_ok_and(|editors| editors == 1)
    }

    pub fn widget_editing(&self) -> bool {
        self.widget_editors.load(Ordering::Relaxed) > 0
    }

    pub fn committed_widgets(&self, incoming: Option<&str>) -> Option<WidgetScene> {
        let source = lock(&self.widgets).clone()?;
        let mut shown: Vec<String> = lock(&self.assignments).values().cloned().collect();
        shown.extend(incoming.map(str::to_string));
        shown.sort();
        shown.dedup();
        Some(source.scene(&shown))
    }

    pub(crate) fn widget_scene(&self, incoming: Option<&str>) -> Option<WidgetScene> {
        let mut scene = self.committed_widgets(incoming)?;
        if self.widget_editing() {
            scene.widgets.clear();
        }
        Some(scene)
    }

    pub(super) fn placed_scene(&self, incoming: Option<&str>) -> Option<WidgetScene> {
        self.widget_scene(incoming).filter(|scene| !scene.widgets.is_empty())
    }

    pub(crate) fn with_widgets(&self, mut command: StillCommand) -> StillCommand {
        if !command.path.is_empty() {
            command.widgets = self.placed_scene(Some(&command.path));
        }
        command
    }

    pub(crate) fn greet_still(&self, output: Option<&str>, path: &str) {
        let greet = |stdin: &mut ChildStdin| {
            self.placed_scene(Some(path)).map_or(Ok(()), |scene| send(stdin, &scene_line(scene)))
        };
        let sent = match output {
            None => lock(&self.still_stdin).as_mut().map(greet),
            Some(output) => lock(&self.output_stills)
                .get_mut(output)
                .and_then(|(_, stdin)| stdin.as_mut())
                .map(greet),
        };
        if let Some(Err(error)) = sent {
            log::warn!("widgets: still did not take the scene: {error}");
        }
    }

    pub(crate) fn greet_dynamic(&self, output: Option<&str>) {
        let Some(scene) = self.placed_scene(None) else { return };
        let line = video_scene_line(scene);
        let sent = match output {
            None => lock(&self.paper_stdin).as_mut().map(|stdin| send(stdin, &line)),
            Some(output) => lock(&self.video_papers)
                .get_mut(output)
                .and_then(|(_, stdin)| stdin.as_mut())
                .map(|stdin| send(stdin, &line)),
        };
        if let Some(Err(error)) = sent {
            log::warn!("widgets: dynamic renderer did not take the scene: {error}");
        }
    }

    pub fn refresh_widgets(&self) -> anyhow::Result<()> {
        let mut last = lock(&self.widget_sync);
        let Some(scene) = self.widget_scene(None) else {
            return Ok(());
        };
        let bare = last.as_ref().is_some_and(|sent| sent.widgets.is_empty());
        if last.as_ref() == Some(&scene) || (bare && scene.widgets.is_empty()) {
            return Ok(());
        }
        crate::plasma::channel::publish_widgets(scene.clone());
        let mut failure = None;
        match PaperClient::existing() {
            Ok(Some(client)) => failure = client.set_widgets(scene.clone()).err(),
            Ok(None) => {}
            Err(error) => log::debug!("widgets: paper socket unreadable: {error:#}"),
        }
        let line = scene_line(scene.clone());
        let video_line = video_scene_line(scene.clone());
        if let Some(stdin) = lock(&self.paper_stdin).as_mut()
            && let Err(error) = send(stdin, &video_line)
        {
            failure = Some(error.into());
        }
        if let Some(stdin) = lock(&self.still_stdin).as_mut()
            && let Err(error) = send(stdin, &line)
        {
            failure = Some(error.into());
        }
        for stdin in lock(&self.output_stills).values_mut().filter_map(|(_, stdin)| stdin.as_mut())
        {
            if let Err(error) = send(stdin, &line) {
                failure = Some(error.into());
            }
        }
        for stdin in lock(&self.video_papers).values_mut().filter_map(|(_, stdin)| stdin.as_mut()) {
            if let Err(error) = send(stdin, &video_line) {
                failure = Some(error.into());
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        *last = Some(scene);
        Ok(())
    }
}

pub(super) fn paper_widgets(client: &PaperClient, scene: WidgetScene) {
    if let Err(error) = client.set_widgets(scene) {
        log::debug!("widgets: paper did not take the scene: {error:#}");
    }
}

fn scene_line(scene: WidgetScene) -> String {
    let mut command = StillCommand::new("");
    command.widgets = Some(scene);
    command.line()
}

fn video_scene_line(scene: WidgetScene) -> String {
    let mut command = PaperCommand::audio(None, None);
    command.widgets = Some(Box::new(scene));
    command.line()
}

fn send(stdin: &mut ChildStdin, line: &str) -> std::io::Result<()> {
    stdin.write_all(line.as_bytes())?;
    stdin.flush()
}

#[cfg(test)]
#[path = "widgets_tests.rs"]
mod tests;
