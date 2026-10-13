use super::*;

#[test]
fn follows_when_enabled() {
    let mut root = json!({"lockScreens":{"mode":"off","dms":false}});
    candidate(
        &mut root,
        &LockScreenSetRequest { target: LockScreenTarget::Dms, enabled: true, sddm_image: None },
    )
    .unwrap();
    assert_eq!(
        root["lockScreens"],
        json!({
            "mode":"follow","dms":true,"hyprlock":false,"swaylock":false,
            "noctalia":false,"sddmImage":""
        })
    );
}

#[test]
fn clears_dormant_targets() {
    let mut root = json!({"lockScreens":{
        "mode":"off","hyprlock":true,"dms":true,"sddmImage":"/old.png"
    }});
    candidate(
        &mut root,
        &LockScreenSetRequest {
            target: LockScreenTarget::Swaylock,
            enabled: true,
            sddm_image: None,
        },
    )
    .unwrap();
    assert_eq!(root["lockScreens"]["mode"], "follow");
    assert_eq!(root["lockScreens"]["swaylock"], true);
    assert_eq!(root["lockScreens"]["hyprlock"], false);
    assert_eq!(root["lockScreens"]["dms"], false);
    assert_eq!(root["lockScreens"]["sddmImage"], "");
}

#[test]
fn keeps_static_selection() {
    let mut root = json!({"lockScreens":{"mode":"static","image":"/wall.png","dms":true}});
    candidate(
        &mut root,
        &LockScreenSetRequest { target: LockScreenTarget::Dms, enabled: false, sddm_image: None },
    )
    .unwrap();
    assert_eq!(root["lockScreens"], json!({"mode":"static","image":"/wall.png","dms":false}));
}

#[test]
fn plasma_mode_is_separate() {
    let mut root = json!({"plasma":{"lockScreen":{"mode":"off"}},"lockScreens":{"mode":"static"}});
    candidate(
        &mut root,
        &LockScreenSetRequest { target: LockScreenTarget::Plasma, enabled: true, sddm_image: None },
    )
    .unwrap();
    assert_eq!(root["plasma"]["lockScreen"]["mode"], "follow");
    assert_eq!(root["lockScreens"]["mode"], "static");
}

#[test]
fn rejects_unowned_sddm_path() {
    let mut root = json!({});
    assert!(
        candidate(
            &mut root,
            &LockScreenSetRequest {
                target: LockScreenTarget::Sddm,
                enabled: true,
                sddm_image: Some("/home/user/wallpaper.png".into()),
            }
        )
        .is_err()
    );
}

#[test]
fn accepts_fixed_sddm_path() {
    let image = skwd_wall_core::infrastructure::lock_screens::sddm::image_for_user(unsafe {
        libc::geteuid()
    });
    let mut root = json!({"lockScreens":{"mode":"static","image":"/wall.png"}});
    candidate(
        &mut root,
        &LockScreenSetRequest {
            target: LockScreenTarget::Sddm,
            enabled: true,
            sddm_image: Some(image.to_string_lossy().into_owned()),
        },
    )
    .unwrap();
    assert_eq!(root["lockScreens"]["mode"], "static");
    assert_eq!(root["lockScreens"]["sddmImage"], image.to_string_lossy().as_ref());
}
