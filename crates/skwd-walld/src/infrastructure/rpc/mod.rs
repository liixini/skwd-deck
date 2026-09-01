mod common;
mod connection;
mod handlers;
mod response;
mod router;
mod source;
mod source_steam;
mod source_wallhaven;
mod source_youtube;
mod tags;
mod wallpaper;

pub(crate) use connection::handle_conn;
pub(crate) use response::fail_msg;

#[cfg(test)]
pub(crate) use router::dispatch;
