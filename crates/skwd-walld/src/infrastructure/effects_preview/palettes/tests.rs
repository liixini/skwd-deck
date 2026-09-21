use super::*;

#[test]
fn saved_options_and_resolution_share_colours_without_shadowing_builtins() {
    let config = Config::from_root(json!({"theme": {"savedThemes": [
        {"name": "Nord", "primary": "#FF0011", "on_primary": "#ff0011", "background": "#102030"},
        {"name": "Broken", "primary": "nope"}, {"name": "", "primary": "#ffffff"}
    ]}}));
    let mut list = json!([
        {"id": "theme", "params": [{"id": "theme", "options": [{"mode": "Nord"}]}]},
        {"id": "gradientmap", "params": [{"id": "theme", "options": []}]}
    ]);
    extend(&mut list, &config);
    assert_eq!(list[0]["params"][0]["options"].as_array().unwrap().len(), 2);
    assert_eq!(list[0]["params"][0]["options"][0]["mode"], "Nord");
    assert_eq!(list[0]["params"][0]["options"][1]["mode"], "saved:Nord");
    assert_eq!(list[1]["params"][0]["options"][0]["swatch"], json!(["#102030", "#ff0011"]));
    let mut chain = json!([
        {"effect": "theme", "params": {"theme": "saved:Nord"}},
        {"effect": "gradientmap", "params": {"theme": "saved:Nord"}},
        {"effect": "theme", "params": {"theme": "Nord"}}
    ]);
    resolve(&mut chain, &config).unwrap();
    assert_eq!(chain[0]["params"]["palette"], json!(["#102030", "#ff0011"]));
    assert_eq!(chain[1]["params"]["palette"], chain[0]["params"]["palette"]);
    assert!(chain[2]["params"].get("palette").is_none());
}

#[test]
fn uses_saved_variant_and_extended_roles_without_source_seed() {
    let theme = json!({"_scheme": {"is_dark_mode": false, "colors": {
        "secondary": {"light": {"color": "#112233"}, "dark": {"color": "#445566"}},
        "source_color": {"light": {"color": "#abcdef"}},
        "bad": {"light": {"color": "#nothex"}}
    }}});
    assert_eq!(colours(&theme), ["#112233"]);
}

#[test]
fn edits_are_resolved_again_and_deleted_or_invalid_themes_fail() {
    let mut chain =
        json!([{"effect": "theme", "params": {"theme": "saved:Mine", "palette": ["#ffffff"]}}]);
    for colour in ["#123456", "#abcdef"] {
        let config = Config::from_root(
            json!({"theme": {"savedThemes": [{"name": "Mine", "primary": colour}]}}),
        );
        resolve(&mut chain, &config).unwrap();
        assert_eq!(chain[0]["params"]["palette"], json!([colour]));
    }
    assert!(
        resolve(&mut chain, &Config::from_root(json!({})))
            .unwrap_err()
            .to_string()
            .contains("no longer exists")
    );
    let config = Config::from_root(
        json!({"theme": {"savedThemes": [{"name": "Mine", "primary": "invalid"}]}}),
    );
    assert!(resolve(&mut chain, &config).unwrap_err().to_string().contains("no valid colours"));
}
