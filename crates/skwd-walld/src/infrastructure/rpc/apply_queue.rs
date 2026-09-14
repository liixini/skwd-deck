use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Instant;

use super::connection::ResponseSink;
use wall_proto::{Request, Response};

use crate::composition::context::Ctx;

const MAX_PENDING_APPLIES: usize = 32;

#[derive(Default)]
pub(crate) struct ApplyQueue {
    state: Mutex<QueueState>,
}

#[derive(Default)]
struct QueueState {
    running: bool,
    pending: VecDeque<QueuedApply>,
}

struct QueuedApply {
    request: Request,
    response: ResponseSink,
    received: Instant,
}

impl QueuedApply {
    fn reply(&self, response: &Response) {
        self.response.send(super::connection::response_payload(response, self.request.id));
    }

    fn supersedes(&self, earlier: &Self) -> bool {
        self.request.str_param("output", "*") == earlier.request.str_param("output", "*")
            && self.request.bool_param("override_locks", false)
                == earlier.request.bool_param("override_locks", false)
            && self.request.str_param("source", "user")
                == earlier.request.str_param("source", "user")
    }
}

impl QueueState {
    fn push(&mut self, job: QueuedApply) -> Option<QueuedApply> {
        if !self.running {
            self.running = true;
            return Some(job);
        }
        self.pending.retain(|earlier| {
            if !job.supersedes(earlier) {
                return true;
            }
            earlier.reply(&Response::ok(
                earlier.request.id,
                serde_json::json!({"applied": "", "superseded": true}),
            ));
            false
        });
        if self.pending.len() == MAX_PENDING_APPLIES {
            job.reply(&Response::err(job.request.id, -32000, "pending apply limit reached"));
        } else {
            self.pending.push_back(job);
        }
        None
    }

    fn next(&mut self) -> Option<QueuedApply> {
        let job = self.pending.pop_front();
        self.running = job.is_some();
        job
    }
}

impl ApplyQueue {
    pub(super) fn submit(&self, ctx: &Ctx, request: Request, response: ResponseSink) {
        let job = QueuedApply { request, response, received: Instant::now() };
        if let Some(error) = super::wallpaper::validate_apply_request(&job.request) {
            ctx.stats.rpc(&job.request.method);
            super::wallpaper::publish_apply_rejection(ctx, &job.request, &error);
            job.reply(&error);
            return;
        }
        let first = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(job);
        if let Some(first) = first {
            let ctx = ctx.clone();
            tokio::task::spawn_blocking(move || run(&ctx, first));
        }
    }
}

fn run(ctx: &Ctx, mut job: QueuedApply) {
    loop {
        let started = Instant::now();
        let result = super::router::dispatch(ctx, &job.request);
        log::info!(
            "apply request={} type={} output={} queue_ms={} execution_ms={} total_ms={}",
            job.request.id,
            job.request.str_param("type", "static"),
            job.request.str_param("output", "*"),
            started.duration_since(job.received).as_millis(),
            started.elapsed().as_millis(),
            job.received.elapsed().as_millis(),
        );
        job.reply(&result);
        let next =
            ctx.apply_queue.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).next();
        let Some(next) = next else { return };
        job = next;
    }
}

#[cfg(test)]
mod tests;
