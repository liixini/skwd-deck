use std::process::Child;

use crate::lock;

use super::supervisor::RendererSupervisor;

impl RendererSupervisor {
    pub fn fleet_len(&self) -> usize {
        let mut fleet = lock(&self.fleet);
        fleet.retain_mut(|child| child.try_wait().map_or(true, |status| status.is_none()));
        fleet.len()
    }

    pub fn fleet_rss_mb(&self) -> u64 {
        let fleet = lock(&self.fleet);
        fleet.iter().map(|child| crate::diag::pid_rss_kb(child.id())).sum::<u64>() / 1024
    }

    pub fn fleet_pids(&self) -> Vec<u32> {
        lock(&self.fleet).iter().map(Child::id).collect()
    }

    pub fn wallpaper_pids(&self) -> Vec<u32> {
        let mut pids = Vec::new();
        if let Some(child) = lock(&self.still_child).as_ref() {
            pids.push(child.id());
        }
        for (child, _) in lock(&self.output_stills).values() {
            pids.push(child.id());
        }
        for (child, _) in lock(&self.video_papers).values() {
            pids.push(child.id());
        }
        if let Some(child) = lock(&self.paper_child).as_ref() {
            pids.push(child.id());
        }
        pids
    }

    pub fn wallpaper_count(&self) -> usize {
        self.wallpaper_pids().len()
    }

    pub fn wallpaper_rss_mb(&self) -> u64 {
        self.wallpaper_pids().iter().map(|&pid| crate::diag::pid_rss_kb(pid)).sum::<u64>() / 1024
    }
}
