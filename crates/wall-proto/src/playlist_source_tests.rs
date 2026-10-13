use super::*;

#[test]
fn quoted_clauses_split() {
    let source = r#"type:image folder:"/w/Gwenni good" color:blue"#;
    assert_eq!(clauses(source), ["type:image", r#"folder:"/w/Gwenni good""#, "color:blue"]);
    assert_eq!(clauses("  all  "), ["all"]);
    assert!(clauses("   ").is_empty());
}

#[test]
fn unterminated_quote_stays_whole() {
    assert_eq!(
        clauses(r#"tag:cat folder:"/w/a type:video"#),
        ["tag:cat", r#"folder:"/w/a type:video"#]
    );
}

#[test]
fn folder_clause_roundtrip() {
    for path in ["/w/anime", "/w/Gwenni good", r#"/w/say "hi""#, r"/w/back\slash", "nature/forest"]
    {
        let clause = folder_clause(path);
        assert_eq!(clauses(&format!("tag:cat {clause} type:video")).len(), 3, "{path}");
        assert_eq!(folder_path(&clause).as_deref(), Some(path));
    }
    assert_eq!(folder_clause("/w/anime"), "folder:/w/anime");
    assert_eq!(folder_path("tag:cat"), None);
}
