pub(super) fn take_flag(args: &mut Vec<String>, names: &[&str]) -> bool {
    if let Some(position) = args.iter().position(|arg| names.contains(&arg.as_str())) {
        args.remove(position);
        true
    } else {
        false
    }
}

pub(super) fn take_option(args: &mut Vec<String>, names: &[&str]) -> Option<String> {
    let position = args.iter().position(|arg| names.contains(&arg.as_str()))?;
    args.remove(position);
    if position < args.len() { Some(args.remove(position)) } else { None }
}

pub(super) fn first_positional(args: &[String]) -> Option<String> {
    args.iter().find(|arg| !arg.starts_with('-')).cloned()
}

mod tests;
