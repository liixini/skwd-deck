use std::collections::{HashMap, HashSet};

use skwd_wall_core::we::SteamTracking;

use super::{
    SteamConfirmation, SteamInventory, include_missing, manifest_ids, reconcile_inventory,
};

#[test]
fn installed_manifest_scope() {
    let root = tempfile::tempdir().unwrap();
    let manifest = root.path().join("appworkshop_431960.acf");
    std::fs::write(
        &manifest,
        "\"AppWorkshop\"\n{\n\"WorkshopItemsInstalled\"\n{\n\"10\"\n{\n}\n}\n\"WorkshopItemDetails\"\n{\n\"20\"\n{\n}\n}\n}",
    )
    .unwrap();
    let installed = manifest_ids(&manifest);
    assert!(installed.contains("10"));
    assert!(!installed.contains("20"));
    assert!(!installed.contains("30"));
}

#[test]
fn confirmation_checks_native_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("steam");
    let content = root.join(skwd_wall_core::paths::STEAM_WORKSHOP_CONTENT);
    let native = content.join("10");
    let unlisted = content.join("20");
    let manual = dir.path().join("manual/10");
    for item in [&native, &unlisted, &manual] {
        std::fs::create_dir_all(item).unwrap();
    }
    let manifest = root.join(skwd_wall_core::paths::STEAM_WORKSHOP_MANIFEST);
    std::fs::write(
        &manifest,
        "\"AppWorkshop\"\n{\n\"WorkshopItemsInstalled\"\n{\n\"10\"\n{\n}\n}\n\"WorkshopItemDetails\"\n{\n\"20\"\n{\n}\n}\n}",
    )
    .unwrap();
    let mut classifier = SteamConfirmation {
        source: dir.path().join("curated"),
        install_root: root,
        tracking: SteamTracking::default(),
        installed: HashMap::new(),
    };
    assert!(classifier.required("10", &native));
    std::fs::remove_file(&manifest).unwrap();
    assert!(classifier.required("10", &native));
    assert!(!classifier.required("20", &unlisted));
    assert!(!classifier.required("10", &manual));
    assert_eq!(classifier.installed.len(), 1);
}

#[test]
fn prunes_native_only() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("workshop");
    let native = root.path().join("native");
    let manual = root.path().join("manual");
    std::fs::create_dir_all(&native).unwrap();
    std::fs::create_dir_all(&manual).unwrap();
    let items = HashMap::from([("10".into(), native.clone()), ("20".into(), manual.clone())]);
    let inventory = SteamInventory {
        account: "account".into(),
        subscribed: HashSet::new(),
        installed: HashMap::from([("10".into(), native)]),
    };
    let mut tracking = SteamTracking::default();
    let removed = reconcile_inventory(&source, &items, &inventory, &mut tracking);
    assert_eq!(removed, ["10"]);
    assert!(tracking.hidden.contains_key("10"));
    assert!(!tracking.hidden.contains_key("20"));
}

#[test]
fn remembers_departed_subscription() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("workshop");
    let native = root.path().join("native");
    std::fs::create_dir_all(&native).unwrap();
    let items = HashMap::from([("10".into(), native.clone())]);
    let inventory = SteamInventory {
        account: "account".into(),
        subscribed: HashSet::new(),
        installed: HashMap::new(),
    };
    let mut tracking = SteamTracking {
        account: "account".into(),
        source: source.clone(),
        seen: HashMap::from([("10".into(), native)]),
        ..SteamTracking::default()
    };
    assert_eq!(reconcile_inventory(&source, &items, &inventory, &mut tracking), ["10"]);
    assert!(tracking.hidden.contains_key("10"));
    let another = SteamInventory { account: "another account".into(), ..inventory };
    assert!(reconcile_inventory(&source, &items, &another, &mut tracking).is_empty());
    assert!(tracking.hidden.is_empty());
    assert_eq!(tracking.owned["account"].get("10"), items.get("10"));
}

#[test]
fn account_switch_keeps_native() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("workshop");
    let native = root.path().join("native");
    std::fs::create_dir_all(&native).unwrap();
    let items = HashMap::from([("10".into(), native.clone())]);
    let old = SteamInventory {
        account: "old".into(),
        subscribed: HashSet::from(["10".into()]),
        installed: HashMap::from([("10".into(), native.clone())]),
    };
    let new = SteamInventory {
        account: "new".into(),
        subscribed: HashSet::new(),
        installed: old.installed.clone(),
    };
    let mut tracking = SteamTracking::default();
    assert!(reconcile_inventory(&source, &items, &old, &mut tracking).is_empty());
    assert!(reconcile_inventory(&source, &items, &new, &mut tracking).is_empty());
    assert!(reconcile_inventory(&source, &items, &new, &mut tracking).is_empty());
    let third = SteamInventory { account: "third".into(), ..new };
    assert!(reconcile_inventory(&source, &items, &third, &mut tracking).is_empty());
    assert_eq!(tracking.owned["old"].get("10"), Some(&native));
    let old_unsubscribed = SteamInventory {
        account: "old".into(),
        subscribed: HashSet::new(),
        installed: HashMap::new(),
    };
    assert_eq!(reconcile_inventory(&source, &items, &old_unsubscribed, &mut tracking), ["10"]);
}

#[test]
fn both_accounts_depart() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("workshop");
    let native = root.path().join("native");
    std::fs::create_dir_all(&native).unwrap();
    let items = HashMap::from([("10".into(), native.clone())]);
    let subscribed = HashSet::from(["10".into()]);
    let installed = HashMap::from([("10".into(), native)]);
    let mut tracking = SteamTracking::default();
    for account in ["a", "b"] {
        let snapshot = SteamInventory {
            account: account.into(),
            subscribed: subscribed.clone(),
            installed: installed.clone(),
        };
        assert!(reconcile_inventory(&source, &items, &snapshot, &mut tracking).is_empty());
    }
    let a_departed = SteamInventory {
        account: "a".into(),
        subscribed: HashSet::new(),
        installed: installed.clone(),
    };
    assert!(reconcile_inventory(&source, &items, &a_departed, &mut tracking).is_empty());
    assert!(tracking.seen.is_empty());
    let b_departed = SteamInventory { account: "b".into(), ..a_departed };
    assert_eq!(reconcile_inventory(&source, &items, &b_departed, &mut tracking), ["10"]);
}

#[test]
fn restores_resubscribed_item() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("workshop");
    let native = root.path().join("native");
    std::fs::create_dir_all(&native).unwrap();
    let items = HashMap::from([("10".into(), native.clone())]);
    let inventory = SteamInventory {
        account: "account".into(),
        subscribed: HashSet::from(["10".into()]),
        installed: HashMap::from([("10".into(), native.clone())]),
    };
    let mut tracking = SteamTracking {
        account: "account".into(),
        source: source.clone(),
        hidden: HashMap::from([("10".into(), native)]),
        ..SteamTracking::default()
    };
    assert!(reconcile_inventory(&source, &items, &inventory, &mut tracking).is_empty());
    assert!(tracking.hidden.is_empty());
    assert!(tracking.seen.contains_key("10"));
}

#[test]
fn missing_native_replacement() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("workshop");
    let native = root.path().join("native/10");
    let manual = root.path().join("manual/10");
    std::fs::create_dir_all(&native).unwrap();
    std::fs::create_dir_all(&manual).unwrap();
    let tracked = HashMap::from([("10".into(), native.clone())]);
    let mut items = HashMap::from([("10".into(), manual.clone())]);
    include_missing(&source, &mut items, &tracked);
    assert_eq!(items.get("10"), Some(&manual));
    items.clear();
    std::fs::create_dir_all(&source).unwrap();
    std::os::unix::fs::symlink(&manual, source.join("10")).unwrap();
    include_missing(&source, &mut items, &tracked);
    assert!(items.is_empty());
    std::fs::remove_file(source.join("10")).unwrap();
    std::os::unix::fs::symlink(&native, source.join("10")).unwrap();
    include_missing(&source, &mut items, &tracked);
    assert_eq!(items.get("10"), Some(&native));
    items.clear();
    std::fs::remove_file(source.join("10")).unwrap();
    include_missing(&source, &mut items, &tracked);
    assert_eq!(items.get("10"), Some(&native));
}
