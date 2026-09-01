use std::io::Write;
use std::path::Path;

use serde_json::{Value, json};
use wall_proto::{ServerMessage, rpc};

use super::args::take_option;
use super::error::CliError;

pub(super) fn run(socket: &Path, mut args: Vec<String>) -> Result<(), CliError> {
    let executable = take_option(&mut args, &["--exec"]);
    let mut connection =
        wall_proto::client::Conn::connect_to(socket).map_err(|_| CliError::Unreachable)?;
    connection.send(rpc::SUBSCRIBE, &json!({})).map_err(|_| CliError::Unreachable)?;
    loop {
        let Some(raw) = connection.read_raw().map_err(|_| CliError::Unreachable)? else {
            return Ok(());
        };
        if let Ok(ServerMessage::Event(event)) = serde_json::from_str::<ServerMessage>(&raw) {
            println!("{raw}");
            let _ = std::io::stdout().flush();
            if let Some(command) = executable.as_deref() {
                run_exec(command, &event.event, &event.data);
            }
        }
    }
}

pub(super) fn shell_escape(argument: &str) -> String {
    format!("'{}'", argument.replace('\'', "'\\''"))
}

pub(super) fn expand_template(command: &str, event: &str, data: &Value) -> String {
    let get = |key: &str| data.get(key).and_then(Value::as_str).unwrap_or("");
    let mut output = String::with_capacity(command.len());
    let mut rest = command;
    while let Some(index) = rest.find('%') {
        output.push_str(&rest[..index]);
        let tail = &rest[index + 1..];
        let expanded = tail.find('%').and_then(|end| {
            let value = match &tail[..end] {
                "event" => event,
                key @ ("path" | "type" | "name" | "output") => get(key),
                _ => return None,
            };
            Some((value, end))
        });
        if let Some((value, end)) = expanded {
            output.push_str(&shell_escape(value));
            rest = &tail[end + 1..];
        } else {
            output.push('%');
            rest = tail;
        }
    }
    output.push_str(rest);
    output
}

fn run_exec(command: &str, event: &str, data: &Value) {
    let expanded = expand_template(command, event, data);
    if let Ok(mut child) =
        std::process::Command::new("setsid").args(["-f", "sh", "-c", &expanded]).spawn()
    {
        let _ = child.wait();
    }
}
