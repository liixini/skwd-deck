use std::sync::Arc;

use skwd_wall_core::db;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::UnixStream;
use tokio::net::unix::OwnedWriteHalf;
use tokio::sync::mpsc::{Receiver, Sender};
use wall_proto::{Request, Response, rpc};

use super::preview_queue::PreviewQueue;
use super::router::dispatch;
use super::source::{DETAILS_DEADLINE, LIST_DEADLINE, ListLifecycle, list_call, run_list};
use crate::composition::context::Ctx;
use crate::infrastructure::events::SUBSCRIBER_CHANNEL_CAPACITY;
use crate::infrastructure::stats::Stats;

const WRITER_DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_SOURCE_TASKS_PER_CONNECTION: usize = 8;
const INLINE_METHODS: [&str; 5] = [
    rpc::PAPER_READY,
    rpc::PAPER_FAILED,
    rpc::SUBSCRIBE,
    rpc::PICKER_SESSION_BEGIN,
    rpc::PICKER_SESSION_END,
];

#[derive(Clone)]
pub(super) struct ResponseSink {
    pub(super) sender: Sender<String>,
    pub(super) close: tokio::sync::watch::Sender<bool>,
}

impl ResponseSink {
    pub(super) fn send(&self, payload: String) {
        if matches!(
            self.sender.try_send(payload),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_))
        ) {
            self.close.send_replace(true);
        }
    }
}

fn next_conn_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub(super) fn peer_uid_allowed(peer_uid: Option<u32>, self_uid: u32) -> bool {
    peer_uid == Some(self_uid)
}

fn peer_cred(stream: &UnixStream) -> Option<(u32, i32)> {
    use std::os::unix::io::AsRawFd;
    let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            std::ptr::addr_of_mut!(credentials).cast(),
            &mut length,
        )
    };
    (result == 0).then_some((credentials.uid, credentials.pid))
}

pub(crate) async fn handle_conn(stream: UnixStream, ctx: &Ctx) {
    let self_uid = unsafe { libc::geteuid() };
    let peer = peer_cred(&stream);
    if !peer_uid_allowed(peer.map(|(uid, _)| uid), self_uid) {
        log::warn!(
            "rejecting connection: peer uid {:?} (pid {:?}) != {self_uid}",
            peer.map(|(uid, _)| uid),
            peer.map(|(_, pid)| pid),
        );
        return;
    }
    ctx.stats.conn_open();
    let conn_id = next_conn_id();
    let mut subscribed = false;
    let mut editing = false;
    let source_lifecycle = Arc::new(ListLifecycle::default());
    let mut source_tasks = tokio::task::JoinSet::new();
    let preview_queue = PreviewQueue::default();
    let (sender, receiver) = tokio::sync::mpsc::channel::<String>(SUBSCRIBER_CHANNEL_CAPACITY);
    let (read_half, write_half) = stream.into_split();
    let (close, closing) = tokio::sync::watch::channel(false);
    let (drain, draining) = tokio::sync::watch::channel(false);
    let replies = ResponseSink { sender: sender.clone(), close };
    tokio::spawn(async move {
        tokio::select! {
            () = writer_task(write_half, receiver, draining) => {},
            () = wait_for_close(closing) => {},
        }
    });

    let mut reader = BufReader::new(read_half);
    let mut line = Vec::with_capacity(4096);
    loop {
        match read_bounded_line(&mut reader, &mut line).await {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                ctx.stats.error();
                if let Ok(text) = serde_json::to_string(&Response::err(
                    0,
                    -32600,
                    format!("request exceeds {MAX_REQUEST_BYTES} byte limit"),
                )) {
                    replies.send(text);
                }
                drain.send_replace(true);
                break;
            }
            Err(_) => break,
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let request = match parse_request(&line, &ctx.stats) {
            Ok(request) => request,
            Err(payload) => {
                if let Some(text) = payload {
                    replies.send(text);
                }
                continue;
            }
        };
        if request.method == rpc::WALL_APPLY {
            if super::wallpaper::validate_apply_request(&request).is_none() {
                preview_queue.cancel_pending(ctx);
            }
            ctx.apply_queue.submit(ctx, request, replies.clone());
            continue;
        }
        if let Some(active) = crate::infrastructure::widgets::edit_request(&request)
            && active != editing
        {
            editing = active;
            let state = Arc::clone(&ctx.state);
            let _ = tokio::task::spawn_blocking(move || {
                crate::infrastructure::widgets::set_editing(&state, active);
            })
            .await;
        }
        if request.method == rpc::SUBSCRIBE && !subscribed {
            subscribed = true;
            ctx.events.subscribe(conn_id, sender.clone());
        }
        if matches!(
            request.method.as_str(),
            rpc::THEME_PREVIEW | rpc::WALL_SHELL_PREVIEW | rpc::WALL_SHELL_PREVIEW_END
        ) {
            preview_queue.submit(ctx, request, replies.clone());
            continue;
        }
        while source_tasks.try_join_next().is_some() {}
        match list_call(&request) {
            Err(response) => {
                ctx.stats.rpc(&request.method);
                ctx.stats.error();
                replies.send(response_payload(&response, request.id));
                continue;
            }
            Ok(Some(call)) => {
                let request_id = request.id;
                let detached = call.generation.is_some() || request.method == rpc::SOURCE_DETAILS;
                if let Some(generation) = call.generation {
                    source_lifecycle.observe(call.provider, generation);
                }
                if detached && source_tasks.len() >= MAX_SOURCE_TASKS_PER_CONNECTION {
                    let _ = source_tasks.join_next().await;
                }
                let task_ctx = ctx.clone();
                let task_request = request.clone();
                let task_stats = Arc::clone(&ctx.stats);
                let task_lifecycle = Arc::clone(&source_lifecycle);
                let response = run_list(
                    task_lifecycle,
                    call,
                    request_id,
                    if request.method == rpc::SOURCE_DETAILS {
                        DETAILS_DEADLINE
                    } else {
                        LIST_DEADLINE
                    },
                    task_stats,
                    move || dispatch(&task_ctx, &task_request),
                );
                if detached {
                    let task_replies = replies.clone();
                    source_tasks.spawn(async move {
                        let response = response.await;
                        task_replies.send(response_payload(&response, request_id));
                    });
                    continue;
                }
                let response = response.await;
                replies.send(response_payload(&response, request_id));
                continue;
            }
            Ok(None) => {}
        }
        let heavy = request.method == rpc::WALL_LIST;
        let payload = if INLINE_METHODS.contains(&request.method.as_str()) {
            payload_for(ctx, &request, heavy)
        } else {
            let task_ctx = ctx.clone();
            let Ok(payload) =
                tokio::task::spawn_blocking(move || payload_for(&task_ctx, &request, heavy)).await
            else {
                break;
            };
            payload
        };
        replies.send(payload);
        if heavy {
            let _ =
                tokio::task::spawn_blocking(crate::infrastructure::platform::trim_allocator).await;
        }
    }
    source_tasks.abort_all();
    while source_tasks.join_next().await.is_some() {}
    let shell_cleanup = preview_queue.close(ctx);
    if subscribed {
        ctx.events.unsubscribe(conn_id);
    }
    if let Some(generation) = shell_cleanup {
        log::warn!("client disconnected mid-preview, restoring the shell colour scheme");
        let state = Arc::clone(&ctx.state);
        let _ = tokio::task::spawn_blocking(move || {
            for sink in &skwd_wall_core::theme_sink::SINKS {
                (sink.preview_end)(&state, Some(generation));
            }
        })
        .await;
    }
    if editing {
        log::warn!("widgets: editor disconnected mid-edit, showing widgets again");
        let state = Arc::clone(&ctx.state);
        let _ = tokio::task::spawn_blocking(move || {
            crate::infrastructure::widgets::set_editing(&state, false);
        })
        .await;
    }
}

async fn read_bounded_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    line: &mut Vec<u8>,
) -> std::io::Result<usize> {
    line.clear();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok(line.len());
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |position| position + 1);
        if line.len().saturating_add(take) > MAX_REQUEST_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "IPC request too large",
            ));
        }
        line.extend_from_slice(&available[..take]);
        reader.consume(take);
        if newline.is_some() {
            return Ok(line.len());
        }
    }
}

async fn wait_for_close(mut closing: tokio::sync::watch::Receiver<bool>) {
    while !*closing.borrow_and_update() {
        if closing.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

async fn writer_task(
    write_half: OwnedWriteHalf,
    receiver: Receiver<String>,
    draining: tokio::sync::watch::Receiver<bool>,
) {
    let writer = write_replies(write_half, receiver, draining.clone());
    tokio::pin!(writer);
    tokio::select! {
        () = &mut writer => {},
        () = wait_for_close(draining) => {
            let _ = tokio::time::timeout(WRITER_DRAIN_TIMEOUT, writer).await;
        },
    }
}

async fn write_replies(
    write_half: OwnedWriteHalf,
    mut receiver: Receiver<String>,
    draining: tokio::sync::watch::Receiver<bool>,
) {
    let mut writer = BufWriter::new(write_half);
    let draining = wait_for_close(draining);
    tokio::pin!(draining);
    loop {
        let message = tokio::select! {
            biased;
            () = &mut draining, if !receiver.is_closed() => {
                receiver.close();
                continue;
            },
            message = receiver.recv() => message,
        };
        let Some(message) = message else { break };

        if writer.write_all(message.as_bytes()).await.is_err()
            || writer.write_all(b"\n").await.is_err()
            || writer.flush().await.is_err()
        {
            break;
        }
    }
}

fn parse_request(line: &[u8], stats: &Stats) -> Result<Request, Option<String>> {
    match serde_json::from_slice(line) {
        Ok(request) => Ok(request),
        Err(error) => {
            stats.error();
            Err(serde_json::to_string(&Response::err(0, -32700, error.to_string())).ok())
        }
    }
}

#[cfg(test)]
mod tests;

fn payload_for(ctx: &Ctx, request: &Request, heavy: bool) -> String {
    let Ctx { database, stats, .. } = ctx;
    if heavy {
        let config = ctx.config.read();
        return list_payload(database, request, stats, &config);
    }
    let response = dispatch(ctx, request);
    response_payload(&response, request.id)
}

pub(super) fn response_payload(response: &Response, request_id: u64) -> String {
    serde_json::to_string(response).unwrap_or_else(|_| {
        format!(
            r#"{{"id":{request_id},"error":{{"code":-32603,"message":"response serialize failed"}}}}"#
        )
    })
}

fn list_payload(
    database: &skwd_wall_core::infrastructure::database::Database,
    request: &Request,
    stats: &Stats,
    config: &skwd_wall_core::config::Config,
) -> String {
    stats.rpc(&request.method);
    let favourites = request.bool_param("favourites", false);
    let mut steam = crate::infrastructure::steam_subscriptions::SteamConfirmation::new(config);
    match database.with_connection(|connection| {
        db::list_wallpapers_json(
            connection,
            favourites,
            &config.wallpaper_dir(),
            &config.we_dir(),
            |id, item| steam.required(id, item),
        )
    }) {
        Ok((mut rows, mut count)) => {
            if config.weather_source() == wall_proto::WeatherSource::Outdoors && !favourites {
                let builtins = crate::infrastructure::outdoors::items(config);
                if let Ok(mut items) = serde_json::from_str::<Vec<serde_json::Value>>(&rows) {
                    items.extend(
                        builtins.into_iter().filter_map(|item| serde_json::to_value(item).ok()),
                    );
                    count = items.len();
                    rows = serde_json::to_string(&items).expect("outdoors list serialization");
                }
            }
            format!(r#"{{"id":{},"result":{{"count":{count},"wallpapers":{rows}}}}}"#, request.id)
        }
        Err(error) => {
            stats.error();
            serde_json::to_string(&Response::err(request.id, -1, error.to_string())).unwrap_or_else(
                |_| {
                    format!(
                        r#"{{"id":{},"error":{{"code":-32603,"message":"list failed"}}}}"#,
                        request.id
                    )
                },
            )
        }
    }
}
