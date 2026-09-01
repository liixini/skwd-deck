use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use wall_proto::{Request, Response};

use crate::infrastructure::stats::Stats;

pub(super) const LIST_CANCELLED: i32 = -32001;
pub(super) const LIST_TIMEOUT: i32 = -32002;
const LIST_WORKER_FAILED: i32 = -32003;
pub(in crate::infrastructure::rpc) const LIST_DEADLINE: Duration = Duration::from_secs(45);
static LIST_WORKERS: tokio::sync::Semaphore =
    tokio::sync::Semaphore::const_new(crate::infrastructure::platform::MAX_IPC_CONNECTIONS);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::infrastructure::rpc) struct ListCall {
    pub provider: wall_proto::sources::Provider,
    pub generation: Option<u64>,
}

#[derive(Default)]
pub(in crate::infrastructure::rpc) struct ListLifecycle {
    generation: Mutex<Option<(wall_proto::sources::Provider, u64)>>,
    changed: tokio::sync::Notify,
}

struct ListLease(Arc<AtomicBool>);

impl Drop for ListLease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl ListLifecycle {
    pub(in crate::infrastructure::rpc) fn observe(
        &self,
        provider: wall_proto::sources::Provider,
        generation: u64,
    ) {
        let mut current = self.generation.lock().unwrap_or_else(PoisonError::into_inner);
        if current.is_none_or(|(_, value)| value <= generation) {
            let changed = *current != Some((provider, generation));
            *current = Some((provider, generation));
            drop(current);
            if changed {
                self.changed.notify_waiters();
            }
        }
    }

    fn current(&self, provider: wall_proto::sources::Provider, generation: u64) -> bool {
        self.generation
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_none_or(|current| current == (provider, generation))
    }

    async fn wait_cancelled(&self, call: ListCall) {
        let Some(generation) = call.generation else {
            std::future::pending::<()>().await;
            return;
        };
        loop {
            let changed = self.changed.notified();
            if !self.current(call.provider, generation) {
                return;
            }
            changed.await;
        }
    }
}

pub(in crate::infrastructure::rpc) fn list_call(
    req: &Request,
) -> Result<Option<ListCall>, Response> {
    let provider = match req.method.as_str() {
        wall_proto::rpc::SOURCE_LIST => {
            let key = req.str_param("source", "");
            let Some(provider) = wall_proto::sources::Provider::from_key(key) else {
                return Err(Response::err(req.id, -32602, format!("unknown source '{key}'")));
            };
            provider
        }
        wall_proto::rpc::WALLHAVEN_SEARCH => wall_proto::sources::Provider::Wallhaven,
        wall_proto::rpc::STEAM_SEARCH => wall_proto::sources::Provider::Steam,
        _ => return Ok(None),
    };
    let generation = match req.params.get("generation") {
        None => None,
        Some(value) => value.as_u64().map(Some).ok_or_else(|| {
            Response::err(req.id, -32602, "generation must be an unsigned integer")
        })?,
    };
    Ok(Some(ListCall { provider, generation }))
}

fn lifecycle_error(stats: &Stats, id: u64, code: i32, message: &str) -> Response {
    stats.error();
    Response::err(id, code, message)
}

pub(in crate::infrastructure::rpc) async fn run_list<F>(
    lifecycle: Arc<ListLifecycle>,
    call: ListCall,
    request_id: u64,
    deadline: Duration,
    stats: Arc<Stats>,
    work: F,
) -> Response
where
    F: FnOnce() -> Response + Send + 'static,
{
    if let Some(generation) = call.generation {
        lifecycle.observe(call.provider, generation);
    }
    let expires = tokio::time::Instant::now() + deadline;
    let permit = tokio::select! {
        permit = LIST_WORKERS.acquire() => match permit {
            Ok(permit) => permit,
            Err(_) => {
                return lifecycle_error(
                    &stats,
                    request_id,
                    LIST_WORKER_FAILED,
                    "source list worker unavailable",
                );
            }
        },
        () = lifecycle.wait_cancelled(call) => {
            return lifecycle_error(
                &stats,
                request_id,
                LIST_CANCELLED,
                "source list request superseded",
            );
        },
        () = tokio::time::sleep_until(expires) => {
            return lifecycle_error(
                &stats,
                request_id,
                LIST_TIMEOUT,
                "source list request timed out",
            );
        },
    };
    let lease = ListLease(Arc::new(AtomicBool::new(true)));
    let worker_active = Arc::clone(&lease.0);
    let worker_lifecycle = Arc::clone(&lifecycle);
    let worker = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if !worker_active.load(Ordering::Acquire)
            || call
                .generation
                .is_some_and(|generation| !worker_lifecycle.current(call.provider, generation))
        {
            return None;
        }
        Some(work())
    });
    let response = tokio::select! {
        result = worker => match result {
            Ok(Some(response)) => response,
            Ok(None) => {
                return lifecycle_error(
                    &stats,
                    request_id,
                    LIST_CANCELLED,
                    "source list request superseded",
                );
            }
            Err(_) => {
                return lifecycle_error(
                    &stats,
                    request_id,
                    LIST_WORKER_FAILED,
                    "source list worker failed",
                );
            }
        },
        () = lifecycle.wait_cancelled(call) => {
            return lifecycle_error(
                &stats,
                request_id,
                LIST_CANCELLED,
                "source list request superseded",
            );
        },
        () = tokio::time::sleep_until(expires) => {
            return lifecycle_error(
                &stats,
                request_id,
                LIST_TIMEOUT,
                "source list request timed out",
            );
        },
    };
    if call.generation.is_some_and(|generation| !lifecycle.current(call.provider, generation)) {
        return lifecycle_error(
            &stats,
            request_id,
            LIST_CANCELLED,
            "source list request superseded",
        );
    }
    response
}
