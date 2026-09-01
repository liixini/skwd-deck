#![deny(unsafe_code)]

mod app;
mod cli;
mod color;
mod distort;
mod imgutil;
mod native;
mod registry;
mod stylize;
mod tone;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    std::process::exit(app::run().await);
}
