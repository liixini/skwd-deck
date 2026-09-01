use super::rpc;

const SOURCE: &str = include_str!("rpc_catalog.rs");

fn declared() -> Vec<(String, String)> {
    SOURCE
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("pub const ")?;
            let (name, rest) = rest.split_once(": &str = \"")?;
            let value = rest.strip_suffix("\";")?;
            Some((name.to_string(), value.to_string()))
        })
        .collect()
}

#[test]
fn all_lists_declared_methods() {
    let declared = declared();
    assert_eq!(declared.len(), 81, "declared method count changed");
    assert_eq!(rpc::ALL.len(), declared.len());
    for (name, value) in &declared {
        assert!(rpc::ALL.contains(&value.as_str()), "{name} ({value})");
    }
}

#[test]
fn method_names_unique() {
    let mut seen = rpc::ALL.to_vec();
    seen.sort_unstable();
    let before = seen.len();
    seen.dedup();
    assert_eq!(seen.len(), before);
}

#[test]
fn method_names_wire_shape() {
    for name in rpc::ALL {
        assert!(!name.is_empty());
        assert!(
            name.bytes().all(|byte| byte.is_ascii_lowercase() || b"._0123456789".contains(&byte)),
            "{name} not lowercase dotted"
        );
        assert!(!name.starts_with('.') && !name.ends_with('.'), "{name} dangling dot");
        assert!(!name.contains(".."), "{name} empty segment");
    }
}

#[test]
fn pinned_wire_strings() {
    assert_eq!(rpc::WALL_APPLY, "wall.apply");
    assert_eq!(rpc::WALL_LIST, "wall.list");
    assert_eq!(rpc::PAPER_READY, "paper.ready");
    assert_eq!(rpc::STATUS, "status");
    assert_eq!(rpc::SUBSCRIBE, "subscribe");
    assert_eq!(rpc::DIAG, "diag");
    assert_eq!(rpc::PICKER_SESSION_BEGIN, "picker.session.begin");
    assert_eq!(rpc::PICKER_SESSION_END, "picker.session.end");
    assert_eq!(rpc::WALL_SHELL_PREVIEW_END, "wall.shell_preview_end");
    assert_eq!(rpc::WALL_REFRESH_OVERVIEW_BACKDROP, "wall.refresh_overview_backdrop");
}

#[test]
fn methods_not_events() {
    for method in rpc::ALL {
        assert!(!method.starts_with("skwd.wall."), "{method}");
    }
}
