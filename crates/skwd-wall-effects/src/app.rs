use std::path::Path;

use crate::cli::{self, Command};
use crate::native;

pub(crate) async fn run() -> i32 {
    let arguments: Vec<String> = std::env::args().collect();
    match cli::parse(&arguments) {
        Command::List => {
            println!("{}", native::list());
            0
        }
        Command::Version => {
            println!("skwd-wall-effects {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Command::Render(request) => match native::render_effects_to_file(
            &request.effects,
            Path::new(&request.input),
            Path::new(&request.output),
            request.max_dimension,
            request.preview,
        )
        .await
        {
            Ok(written) => {
                println!("{}", written.display());
                0
            }
            Err(error) => {
                eprintln!("render failed: {error}");
                1
            }
        },
        Command::Invalid(command) => {
            eprintln!("usage: skwd-wall-effects <list|render>; got {command:?}");
            2
        }
    }
}
