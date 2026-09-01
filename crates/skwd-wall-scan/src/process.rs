#[cfg(target_env = "gnu")]
const MALLOC_ARENA_MAX: libc::c_int = 2;
#[cfg(target_env = "gnu")]
const MALLOC_MMAP_THRESHOLD: libc::c_int = 1024 * 1024;

#[cfg(feature = "obs-heap")]
#[global_allocator]
static GLOBAL: skwd_wall_core::countalloc::Counting<std::alloc::System> =
    skwd_wall_core::countalloc::Counting(std::alloc::System);

pub(crate) fn initialize(arguments: &[String]) {
    #[cfg(target_env = "gnu")]
    unsafe {
        libc::mallopt(libc::M_ARENA_MAX, MALLOC_ARENA_MAX);
        libc::mallopt(libc::M_MMAP_THRESHOLD, MALLOC_MMAP_THRESHOLD);
    }

    let debug = arguments.iter().any(|argument| argument == "--debug")
        || matches!(std::env::var("SKWD_WALL_DEBUG").as_deref(), Ok("1" | "true"));
    let default_level = if debug { "debug" } else { "info" };
    let filter = std::env::var("SKWD_WALL_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .unwrap_or_else(|_| default_level.to_string());
    env_logger::Builder::new().parse_filters(&filter).init();
}

pub(crate) fn arm_deadline(duration: std::time::Duration) -> std::io::Result<()> {
    std::thread::Builder::new().name("skwd-scan-deadline".to_string()).spawn(move || {
        std::thread::sleep(duration);
        unsafe { libc::_exit(124) };
    })?;
    Ok(())
}
