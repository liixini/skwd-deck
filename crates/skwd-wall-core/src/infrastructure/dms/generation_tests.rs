use super::*;
use serde_json::json;

#[test]
fn template_choices_follow_dms_settings_and_discovered_ids() {
    let checks =
        json!([{"id":"gtk"},{"id":"nvim"},{"id":"kitty"},{"id":"zenbrowser"},{"id":"futureapp"}]);
    assert_eq!(skip_templates(&json!({}), &checks).unwrap(), "nvim");
    assert_eq!(skip_templates(&json!({"matugenTemplateGtk":false,"matugenTemplateNeovim":true,"matugenTemplateZenBrowser":false,"matugenTemplateFutureApp":false}), &checks).unwrap(), "gtk,zenbrowser,futureapp");
    assert_eq!(
        skip_templates(&json!({"runDmsMatugenTemplates":false}), &checks).unwrap(),
        "gtk,nvim,kitty,zenbrowser,futureapp"
    );
    for invalid in [json!({}), json!([]), json!([{}]), json!([{"id":"a,b"}])] {
        assert!(skip_templates(&json!({}), &invalid).is_err());
    }
}

#[test]
fn native_generation_preserves_user_options_and_literal_paths() {
    let args = generation_args(&json!({"runUserMatugenTemplates":false,"syncModeWithPortal":true,"terminalsAlwaysDark":true,"matugenContrast":0.5,"matugenScheme":"scheme-vibrant","matugenSourceMode":"colorful","matugenSmartMode":true,"iconTheme":"Papirus Dark"}), &json!([{"id":"kitty"}]), "/wall/a ' $(literal).png", false).unwrap();
    for pair in [
        ["--value", "/wall/a ' $(literal).png"],
        ["--mode", "smart"],
        ["--matugen-type", "scheme-vibrant"],
        ["--contrast", "0.5"],
        ["--source-mode", "colorful"],
        ["--icon-theme", "Papirus Dark"],
    ] {
        assert!(args.windows(2).any(|args| args == pair));
    }
    for flag in ["--run-user-templates=false", "--sync-mode-with-portal", "--terminals-always-dark"]
    {
        assert!(args.iter().any(|arg| arg == flag));
    }
    assert!(!args.iter().any(|arg| arg == "--dry-run" || arg == "--skip-templates"));
}
