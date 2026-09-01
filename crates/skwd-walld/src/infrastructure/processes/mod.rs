mod scanner;
mod supervisor;
mod video_optimizer;

pub(crate) use scanner::spawn_scan;
pub(crate) use supervisor::ProcessSupervisor;

#[cfg(test)]
use scanner::scanner_args;

#[cfg(test)]
mod tests;
