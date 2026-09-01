use std::sync::Arc;

use skwd_wall_core::db;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::UnixStream;
use tokio::net::unix::OwnedWriteHalf;
use tokio::sync::mpsc::Receiver;
use wall_proto::{Request, Response, rpc};

use super::router::dispatch;
use super::source::{LIST_DEADLINE, ListLifecycle, list_call, run_list};
use crate::composition::context::Ctx;
use crate::infrastructure::events::SUBSCRIBER_CHANNEL_CAPACITY;
use crate::infrastructure::stats::Stats;

const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_GENERATED_LISTS_PER_CONNECTION: usize = 8;
const INLINE_METHODS: [&str; 4] =
    [rpc::PAPER_READY, rpc::SUBSCRIBE, rpc::PICKER_SESSION_BEGIN, rpc::PICKER_SESSION_END];

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
    let mut previewing = false;
    let source_lifecycle = Arc::new(ListLifecycle::default());
    let mut source_tasks = tokio::task::JoinSet::new();
    let (sender, receiver) = tokio::sync::mpsc::channel::<String>(SUBSCRIBER_CHANNEL_CAPACITY);
    let (read_half, write_half) = stream.into_split();
    tokio::spawn(writer_task(write_half, receiver));

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
                    let _ = sender.send(text).await;
                }
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
                    let _ = sender.send(text).await;
                }
                continue;
            }
        };
        if request.method == rpc::WALL_SHELL_PREVIEW {
            previewing = true;
        } else if request.method == rpc::WALL_SHELL_PREVIEW_END {
            previewing = false;
        }
        if request.method == rpc::SUBSCRIBE && !subscribed {
            subscribed = true;
            ctx.events.subscribe(conn_id, sender.clone());
        }
        while source_tasks.try_join_next().is_some() {}
        match list_call(&request) {
            Err(response) => {
                ctx.stats.rpc(&request.method);
                ctx.stats.error();
                if sender.send(response_payload(&response, request.id)).await.is_err() {
                    break;
                }
                continue;
            }
            Ok(Some(call)) => {
                let request_id = request.id;
                if let Some(generation) = call.generation {
                    source_lifecycle.observe(call.provider, generation);
                    if source_tasks.len() >= MAX_GENERATED_LISTS_PER_CONNECTION {
                        let _ = source_tasks.join_next().await;
                    }
                }
                let task_ctx = ctx.clone();
                let task_request = request.clone();
                let task_stats = Arc::clone(&ctx.stats);
                let task_lifecycle = Arc::clone(&source_lifecycle);
                let response = run_list(
                    task_lifecycle,
                    call,
                    request_id,
                    LIST_DEADLINE,
                    task_stats,
                    move || dispatch(&task_ctx, &task_request),
                );
                if call.generation.is_some() {
                    let task_sender = sender.clone();
                    source_tasks.spawn(async move {
                        let response = response.await;
                        let _ = task_sender.send(response_payload(&response, request_id)).await;
                    });
                    continue;
                }
                let response = response.await;
                if sender.send(response_payload(&response, request_id)).await.is_err() {
                    break;
                }
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
        let sent = sender.send(payload).await.is_ok();
        if heavy {
            let _ = tokio::task::spawn_blocking(trim_allocator).await;
        }
        if !sent {
            break;
        }
    }
    source_tasks.abort_all();
    while source_tasks.join_next().await.is_some() {}
    if subscribed {
        ctx.events.unsubscribe(conn_id);
    }
    if previewing {
        log::warn!("client disconnected mid-preview, restoring the shell colour scheme");
        let state = Arc::clone(&ctx.state);
        let _ = tokio::task::spawn_blocking(move || {
            state.theme().bump_shell_preview();
            for sink in &skwd_wall_core::theme_sink::SINKS {
                (sink.preview_end)(&state);
            }
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

async fn writer_task(write_half: OwnedWriteHalf, mut receiver: Receiver<String>) {
    let mut writer = BufWriter::new(write_half);
    while let Some(message) = receiver.recv().await {
        if writer.write_all(message.as_bytes()).await.is_err()
            || writer.write_all(b"\n").await.is_err()
            || writer.flush().await.is_err()
        {
            break;
        }
    }
}

fn trim_allocator() {
    #[cfg(target_env = "gnu")]
    unsafe {
        libc::malloc_trim(0)
    };
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
        return list_payload(database, request, stats);
    }
    let response = dispatch(ctx, request);
    response_payload(&response, request.id)
}

fn response_payload(response: &Response, request_id: u64) -> String {
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
) -> String {
    stats.rpc(&request.method);
    let favourites = request.bool_param("favourites", false);
    match database.with_connection(|connection| db::list_wallpapers_json(connection, favourites)) {
        Ok((rows, count)) => {
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
