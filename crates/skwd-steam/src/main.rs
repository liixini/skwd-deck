#![deny(unsafe_code)]

mod app;
mod callback;
mod download;
mod event_output;
mod policy;
mod search;
mod search_params;

fn version_requested(arguments: &[String]) -> bool {
    arguments.first().is_some_and(|argument| argument == "--version" || argument == "-V")
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if version_requested(&args) {
        println!("skwd-steam {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    std::process::exit(app::run(&args).await);
}

#[cfg(test)]
mod main_tests {
    use super::version_requested;

    #[test]
    fn version_path_is_explicit_and_precedes_steam_startup() {
        assert!(version_requested(&["--version".into()]));
        assert!(version_requested(&["-V".into()]));
        assert!(!version_requested(&["431960".into(), "--version".into()]));
    }
}
