#![cfg(test)]

use super::*;

fn arguments(values: &[&str]) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}

#[test]
fn version_is_a_headless_command() {
    for flag in ["--version", "-V"] {
        assert!(matches!(parse(&arguments(&["skwd-wall-effects", flag])), Command::Version));
    }
}

#[test]
fn render_args_decode() {
    let args = arguments(&[
        "skwd-wall-effects",
        "render",
        "--input",
        "in.png",
        "--effect",
        "invert",
        "--output",
        "out",
        "--params",
        r#"{"factor":2}"#,
        "--max-dim",
        "640",
        "--preview",
    ]);
    let Command::Render(request) = parse(&args) else {
        panic!("expected render request");
    };
    assert_eq!(request.input, "in.png");
    assert_eq!(request.effects[0]["effect"], "invert");
    assert_eq!(request.output, "out");
    assert_eq!(request.effects[0]["params"]["factor"], 2);
    assert_eq!(request.max_dimension, 640);
    assert!(request.preview);
}

#[test]
fn malformed_values_default() {
    let args =
        arguments(&["skwd-wall-effects", "render", "--params", "invalid", "--max-dim", "invalid"]);
    let Command::Render(request) = parse(&args) else {
        panic!("expected render request");
    };
    assert_eq!(request.effects[0]["params"], serde_json::json!({}));
    assert_eq!(request.max_dimension, 0);
}

#[test]
fn effects_flag_wins() {
    let args = arguments(&[
        "skwd-wall-effects",
        "render",
        "--effect",
        "invert",
        "--effects",
        r#"[{"effect":"sepia","params":{"intensity":0.5}},{"effect":"mirror","params":{}}]"#,
    ]);
    let Command::Render(request) = parse(&args) else {
        panic!("expected render request");
    };
    assert_eq!(request.effects.len(), 2);
    assert_eq!(request.effects[0]["effect"], "sepia");
    assert_eq!(request.effects[1]["effect"], "mirror");
}
