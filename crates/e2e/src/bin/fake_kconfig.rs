use std::io::Write;
use std::path::{Path, PathBuf};

struct Request {
    file: PathBuf,
    header: String,
    key: String,
    value: Option<String>,
}

fn parse(args: &[String]) -> Option<Request> {
    let (mut file, mut groups, mut key, mut value, mut delete) =
        (None, String::new(), None, None, false);
    let mut index = 0;
    while index < args.len() {
        let next = args.get(index + 1).cloned();
        match args[index].as_str() {
            "--file" => file = next,
            "--group" => {
                groups.push('[');
                groups.push_str(&next.unwrap_or_default());
                groups.push(']');
            }
            "--key" => key = next,
            "--delete" => {
                delete = true;
                index += 1;
                continue;
            }
            other => {
                value = Some(other.to_string());
                index += 1;
                continue;
            }
        }
        index += 2;
    }
    let file = PathBuf::from(file?);
    let file = if file.is_absolute() {
        file
    } else {
        PathBuf::from(std::env::var_os("XDG_CONFIG_HOME")?).join(file)
    };
    Some(Request { file, header: groups, key: key?, value: if delete { None } else { value } })
}

fn section(lines: &[String], header: &str) -> Option<(usize, usize)> {
    let start = lines.iter().position(|line| line == header)?;
    let end = lines[start + 1..]
        .iter()
        .position(|line| line.starts_with('['))
        .map_or(lines.len(), |offset| start + 1 + offset);
    Some((start, end))
}

fn write(path: &Path, request: &Request) {
    let mut lines: Vec<String> =
        std::fs::read_to_string(path).unwrap_or_default().lines().map(str::to_string).collect();
    let prefix = format!("{}=", request.key);
    let entry = request.value.as_ref().map(|value| format!("{prefix}{value}"));
    match (section(&lines, &request.header), entry) {
        (Some((start, end)), entry) => {
            let existing = (start + 1..end).find(|index| lines[*index].starts_with(&prefix));
            match (existing, entry) {
                (Some(index), Some(entry)) => lines[index] = entry,
                (Some(index), None) => {
                    lines.remove(index);
                }
                (None, Some(entry)) => lines.insert(end, entry),
                (None, None) => {}
            }
        }
        (None, Some(entry)) => {
            if lines.last().is_some_and(|line| !line.is_empty()) {
                lines.push(String::new());
            }
            lines.push(request.header.clone());
            lines.push(entry);
        }
        (None, None) => {}
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, lines.join("\n") + "\n");
}

fn main() {
    let mut args = std::env::args();
    let program = args.next().unwrap_or_default();
    let args: Vec<String> = args.collect();
    let name = Path::new(&program).file_name().and_then(|name| name.to_str()).unwrap_or_default();
    if let Some(dir) = std::env::var_os("SKWD_E2E_PLASMA").map(PathBuf::from)
        && let Ok(mut log) =
            std::fs::OpenOptions::new().create(true).append(true).open(dir.join("kconfig.log"))
    {
        let _ = writeln!(log, "{}", serde_json::json!([name, args]));
    }
    let Some(request) = parse(&args) else { std::process::exit(1) };
    if name == "kwriteconfig6" {
        write(&request.file, &request);
        return;
    }
    let text = std::fs::read_to_string(&request.file).unwrap_or_default();
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let prefix = format!("{}=", request.key);
    if let Some((start, end)) = section(&lines, &request.header)
        && let Some(value) =
            lines[start + 1..end].iter().find_map(|line| line.strip_prefix(&prefix))
    {
        println!("{value}");
    }
}
