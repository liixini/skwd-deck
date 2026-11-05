use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::Value;
use wall_proto::{Request, Response, rpc};

use super::connection::{ResponseSink, response_payload};
use super::router::dispatch;
use crate::composition::context::Ctx;

const MAX_PENDING_PREVIEWS: usize = 8;

#[derive(Serialize)]
struct SkippedTheme {
    backend: &'static str,
    colors: [String; 0],
    palette: Option<Value>,
    superseded: bool,
}

#[derive(Serialize)]
struct SkippedShell {
    queued: bool,
    superseded: bool,
}

#[derive(Serialize)]
struct SkippedEnd {
    ok: bool,
    superseded: bool,
}

pub(super) fn superseded(request: &Request) -> Response {
    let result = match request.method.as_str() {
        rpc::THEME_PREVIEW => serde_json::to_value(SkippedTheme {
            backend: "",
            colors: [],
            palette: None,
            superseded: true,
        }),
        rpc::WALL_SHELL_PREVIEW => {
            serde_json::to_value(SkippedShell { queued: false, superseded: true })
        }
        rpc::WALL_SHELL_PREVIEW_END => {
            serde_json::to_value(SkippedEnd { ok: true, superseded: true })
        }
        _ => unreachable!(),
    };
    Response::ok(request.id, result.expect("preview response"))
}

#[derive(Default)]
pub(super) struct PreviewQueue {
    state: Arc<Mutex<QueueState>>,
}

#[derive(Default)]
struct QueueState {
    running: bool,
    closed: bool,
    epoch: u64,
    restore: Option<u64>,
    shell_generation: Option<u64>,
    pending: VecDeque<PreviewJob>,
}

struct PreviewJob {
    request: Request,
    replies: ResponseSink,
    epoch: u64,
    generation: u64,
}

impl PreviewJob {
    fn reply(&self, response: &Response) {
        self.replies.send(response_payload(response, self.request.id));
    }

    fn superseded(self) {
        self.reply(&superseded(&self.request));
    }
}

impl QueueState {
    fn cancel(&mut self, ctx: &Ctx) {
        self.shell_generation = self
            .shell_generation
            .and_then(|generation| ctx.state.theme().cancel_shell_preview(generation));
    }

    fn discard_matching(&mut self, method: &str) {
        let boundary = if method == rpc::WALL_SHELL_PREVIEW {
            self.pending
                .iter()
                .rposition(|job| job.request.method == rpc::WALL_SHELL_PREVIEW_END)
                .map_or(0, |index| index + 1)
        } else {
            0
        };
        let mut index = boundary;
        while index < self.pending.len() {
            if self.pending[index].request.method == method {
                self.pending.remove(index).expect("preview index").superseded();
            } else {
                index += 1;
            }
        }
    }

    fn push(&mut self, job: PreviewJob) -> Option<PreviewJob> {
        if !self.running {
            self.running = true;
            return Some(job);
        }
        match job.request.method.as_str() {
            rpc::THEME_PREVIEW | rpc::WALL_SHELL_PREVIEW => {
                self.discard_matching(&job.request.method);
            }
            rpc::WALL_SHELL_PREVIEW_END => {
                self.discard_matching(rpc::WALL_SHELL_PREVIEW);
            }
            _ => unreachable!(),
        }
        if self.pending.len() == MAX_PENDING_PREVIEWS {
            let index = self
                .pending
                .iter()
                .position(|earlier| earlier.request.method != rpc::WALL_SHELL_PREVIEW_END)
                .unwrap_or(0);
            self.pending.remove(index).expect("pending preview").superseded();
        }
        self.pending.push_back(job);
        None
    }
}

impl PreviewQueue {
    pub(super) fn submit(&self, ctx: &Ctx, request: Request, replies: ResponseSink) {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            return;
        }
        if matches!(request.method.as_str(), rpc::WALL_SHELL_PREVIEW | rpc::WALL_SHELL_PREVIEW_END)
        {
            state.epoch = state.epoch.wrapping_add(1);
        }
        if request.method == rpc::WALL_SHELL_PREVIEW_END {
            state.cancel(ctx);
        }
        let epoch = state.epoch;
        let generation = ctx.state.theme().shell_preview_generation();
        let first = state.push(PreviewJob { request, replies, epoch, generation });
        drop(state);
        if let Some(first) = first {
            let queue = Arc::clone(&self.state);
            let ctx = ctx.clone();
            tokio::task::spawn_blocking(move || run(&ctx, &queue, Some(first)));
        }
    }

    pub(super) fn cancel_pending(&self, ctx: &Ctx) {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.epoch = state.epoch.wrapping_add(1);
        state.cancel(ctx);
        state.restore = state.shell_generation;
        let mut retained = VecDeque::new();
        for job in state.pending.drain(..) {
            if job.request.method == rpc::WALL_SHELL_PREVIEW_END {
                retained.push_back(job);
            } else {
                job.superseded();
            }
        }
        state.pending = retained;
        if !state.running && state.restore.is_some() {
            state.running = true;
            drop(state);
            let queue = Arc::clone(&self.state);
            let ctx = ctx.clone();
            tokio::task::spawn_blocking(move || run(&ctx, &queue, None));
        }
    }

    pub(super) fn close(&self, ctx: &Ctx) -> Option<u64> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.closed = true;
        state.pending.clear();
        state
            .shell_generation
            .and_then(|generation| ctx.state.theme().cancel_shell_preview(generation))
    }
}

fn run(ctx: &Ctx, queue: &Mutex<QueueState>, mut job: Option<PreviewJob>) {
    loop {
        restore(ctx, queue);
        if let Some(active) = job.take() {
            let response = match active.request.method.as_str() {
                rpc::WALL_SHELL_PREVIEW => {
                    ctx.stats.rpc(&active.request.method);
                    super::wallpaper::shell_preview(&ctx.state, &active.request, &ctx.stats, || {
                        let mut state =
                            queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                        if state.closed || state.epoch != active.epoch {
                            return None;
                        }
                        let current = ctx.state.theme().shell_preview_generation();
                        if current != active.generation && state.shell_generation != Some(current) {
                            return None;
                        }
                        let generation = ctx.state.theme().cancel_shell_preview(current)?;
                        state.shell_generation = Some(generation);
                        Some(generation)
                    })
                }
                rpc::WALL_SHELL_PREVIEW_END => {
                    ctx.stats.rpc(&active.request.method);
                    let generation = {
                        let state = queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                        state.shell_generation.filter(|_| !state.closed)
                    };
                    generation.map_or_else(
                        || superseded(&active.request),
                        |generation| {
                            super::handlers::shell_preview_end(
                                &ctx.state,
                                &active.request,
                                Some(generation),
                            )
                        },
                    )
                }
                _ => dispatch(ctx, &active.request),
            };
            if active.request.method == rpc::WALL_SHELL_PREVIEW_END {
                restore(ctx, queue);
            }
            active.reply(&response);
        }
        let mut state = queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            state.running = false;
            return;
        }
        job = state.pending.pop_front();
        if job.is_none() && state.restore.is_none() {
            state.running = false;
            return;
        }
    }
}

fn restore(ctx: &Ctx, queue: &Mutex<QueueState>) {
    let restore = queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner).restore.take();
    if let Some(generation) = restore {
        for sink in &skwd_wall_core::theme_sink::SINKS {
            (sink.preview_end)(&ctx.state, Some(generation));
        }
    }
}

#[cfg(test)]
#[path = "preview_queue_tests.rs"]
mod tests;
