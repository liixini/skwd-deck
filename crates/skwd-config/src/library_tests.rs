use super::*;
use serde_json::json;

#[test]
fn folder_boundaries() {
    let policy = LibraryPolicy::new(
        &json!({"paths":{"wallpaper":"/walls"}, "library":{"excludedFolders":["Sus/", "/other/private", "", 42]}}),
    );
    for path in
        ["/walls/Sus", "/walls/Sus/deep/a.png", "/other/private/a.mp4", "/walls/a/../Sus/b.png"]
    {
        assert!(policy.excludes(Path::new(path)), "{path}");
    }
    for path in [
        "/walls/Sushi/a.png",
        "/walls/sus/a.png",
        "/other/private-ish/a.png",
        "/walls/public/a.png",
    ] {
        assert!(!policy.excludes(Path::new(path)), "{path}");
    }
}

#[test]
fn home_and_missing() {
    let policy = LibraryPolicy::new(&json!({"library":{"excludedFolders":["~/missing/Sus"]}}));
    assert!(policy.excludes(&PathBuf::from(crate::home()).join("missing/Sus/a.png")));
    assert!(!LibraryPolicy::new(&json!({})).excludes(Path::new("/walls/a.png")));
}

#[test]
fn exclusion_array_contract() {
    let root = json!({"library":{"excludedFolders":["/walls/Sus", "~/private"]}});
    assert_eq!(excluded_folders(&root), ["/walls/Sus", "~/private"]);
    assert_eq!(crate::schema::setting::library::EXCLUDED_FOLDERS.path(), "library.excludedFolders");
}

#[test]
fn source_array_contract() {
    let mut root = json!({"paths":{"wallpaper":"/walls","videoWallpaper":"/videos"}});
    assert_eq!(library_folders(&root), ["/walls", "/videos"]);
    root["library"] = json!({"folders":[]});
    assert!(library_folders(&root).is_empty());
    root["library"]["folders"] = json!(["/new", "/new/", "", 42, "~/art"]);
    assert_eq!(library_folders(&root), ["/new".to_string(), format!("{}/art", crate::home())]);
    assert_eq!(crate::schema::setting::library::FOLDERS.path(), "library.folders");
    assert_eq!(crate::schema::value_kind("library.folders"), Some(crate::schema::ValueKind::Array));
}

#[test]
fn many_source_folders() {
    let folders: Vec<String> = (0..300).map(|index| format!("/library/{index}")).collect();
    let root = json!({"library":{"folders":folders,"excludedFolders":["private"]}});
    assert_eq!(library_folders(&root), folders);
    let policy = LibraryPolicy::new(&root);
    assert!(policy.contains(Path::new("/library/299/a.png")));
    assert!(!policy.contains(Path::new("/library/300/a.png")));
    assert!(policy.excludes(Path::new("/library/299/private/a.png")));
    assert!(!policy.excludes(Path::new("/library/299/private-art/a.png")));
}

#[test]
fn legacy_relative_folders() {
    let directory = std::env::current_dir().unwrap();
    let root = json!({"paths":{"wallpaper":"unused/../walls","videoWallpaper":"videos"}});
    assert_eq!(
        library_folders(&root),
        [
            directory.join("walls").display().to_string(),
            directory.join("videos").display().to_string()
        ]
    );
    let policy = LibraryPolicy::new(&root);
    assert!(policy.contains(Path::new("walls/a.png")));
    assert!(policy.contains(&directory.join("unused/../walls/a.png")));
    assert_eq!(crate::wallpaper_dir(&root), "unused/../walls");
}
