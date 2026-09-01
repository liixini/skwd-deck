pub fn is_safe_positional(argument: &str) -> bool {
    !argument.starts_with('-')
}

pub fn still_args(output: &str, path: &str, fill_mode: &str) -> Vec<String> {
    vec![output.to_string(), path.to_string(), "--fill-mode".to_string(), fill_mode.to_string()]
}

pub fn transition_args(
    from: &str,
    to: &str,
    fill_mode: &str,
    shader: &str,
    duration_ms: u64,
) -> Vec<String> {
    transition_args_for("*", from, to, fill_mode, shader, duration_ms)
}

pub fn managed_transition_args(
    from: &str,
    to: &str,
    fill_mode: &str,
    shader: &str,
    duration_ms: u64,
) -> Vec<String> {
    let mut args = transition_args(from, to, fill_mode, shader, duration_ms);
    args.push("--persist".to_string());
    args
}

pub fn transition_args_for(
    output: &str,
    from: &str,
    to: &str,
    fill_mode: &str,
    shader: &str,
    duration_ms: u64,
) -> Vec<String> {
    vec![
        output.to_string(),
        to.to_string(),
        "--transition-from".to_string(),
        from.to_string(),
        "--shader".to_string(),
        shader.to_string(),
        "--duration-ms".to_string(),
        duration_ms.to_string(),
        "--fill-mode".to_string(),
        fill_mode.to_string(),
        "--layer".to_string(),
        "bottom".to_string(),
    ]
}

pub fn vk_video_args(
    output: &str,
    path: &str,
    fill_mode: &str,
    mute: bool,
    volume: u32,
) -> Vec<String> {
    vec![
        output.to_string(),
        path.to_string(),
        "--fill-mode".to_string(),
        fill_mode.to_string(),
        "-o".to_string(),
        format!("mute={};volume={}", if mute { "yes" } else { "no" }, volume),
    ]
}

pub fn video_transition_args(
    output: &str,
    from: &str,
    to: &str,
    fill_mode: &str,
    shader: &str,
    duration_ms: u64,
    mute: bool,
    volume: u32,
) -> Vec<String> {
    vec![
        output.to_string(),
        to.to_string(),
        "--transition-from".to_string(),
        from.to_string(),
        "--shader".to_string(),
        shader.to_string(),
        "--duration-ms".to_string(),
        duration_ms.to_string(),
        "--fill-mode".to_string(),
        fill_mode.to_string(),
        "--layer".to_string(),
        "bottom".to_string(),
        "--mute".to_string(),
        if mute { "true" } else { "false" }.to_string(),
        "--volume".to_string(),
        volume.to_string(),
        "--persist".to_string(),
    ]
}

pub fn transition_reveal_delay_ms(duration_ms: u64) -> u64 {
    (duration_ms * 3 / 4).min(duration_ms.saturating_sub(80)).max(120)
}

pub fn is_video_path(path: &str) -> bool {
    paper_control::is_video_path(path)
}
