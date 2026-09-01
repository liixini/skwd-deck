use serde_json::Value;

pub(crate) enum Command {
    List,
    Render(RenderRequest),
    Version,
    Invalid(String),
}

pub(crate) struct RenderRequest {
    pub(crate) input: String,
    pub(crate) effects: Vec<Value>,
    pub(crate) output: String,
    pub(crate) max_dimension: u32,
    pub(crate) preview: bool,
}

pub(crate) fn parse(args: &[String]) -> Command {
    match args.get(1).map_or("", String::as_str) {
        "--version" | "-V" => Command::Version,
        "list" => Command::List,
        "render" => {
            let effect = flag(args, "--effect").unwrap_or_default();
            let params = flag(args, "--params")
                .and_then(|text| serde_json::from_str(text).ok())
                .unwrap_or_else(|| serde_json::json!({}));
            let effects = flag(args, "--effects")
                .and_then(|text| serde_json::from_str::<Value>(text).ok())
                .and_then(|value| value.as_array().cloned())
                .filter(|effects| !effects.is_empty())
                .unwrap_or_else(|| {
                    vec![serde_json::json!({
                        "effect": effect,
                        "params": params,
                    })]
                });
            Command::Render(RenderRequest {
                input: flag(args, "--input").unwrap_or_default().to_string(),
                effects,
                output: flag(args, "--output").unwrap_or_default().to_string(),
                max_dimension: flag(args, "--max-dim")
                    .and_then(|text| text.parse().ok())
                    .unwrap_or(0),
                preview: args.iter().any(|argument| argument == "--preview"),
            })
        }
        other => Command::Invalid(other.to_string()),
    }
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

mod tests;
