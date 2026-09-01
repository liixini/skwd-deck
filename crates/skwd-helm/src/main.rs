mod args;
mod commands;
mod config;
mod entry;
mod error;
mod hardware;
mod look;
mod pack;
mod rpc;
mod tests;
mod ui;
mod wallpaper;
mod watch;

fn main() {
    std::process::exit(entry::run());
}
