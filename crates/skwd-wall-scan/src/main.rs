mod app;
mod cli;
mod media_jobs;
mod process;
mod remote_thumbs;
mod reporter;
mod sandbox;
mod scan_jobs;
mod theme;

fn main() -> anyhow::Result<()> {
    app::run()
}

#[cfg(test)]
use remote_thumbs::{
    fetch_bytes, fetch_bytes_guarded, host_private as thumb_host_private,
    parse_job as parse_thumb_job, require_public as thumb_require_public,
    resolve_redirect as thumb_resolve_redirect, scheme_host as thumb_scheme_host,
};
#[cfg(test)]
use scan_jobs::{key_present, prune_targets, scan_done_payload};
#[cfg(test)]
use theme::render_template;

#[cfg(test)]
mod tests;
