use super::*;
use serde_json::json;

fn public() -> Visibility {
    let root = json!({"library":{"profiles":[{
        "id":"public",
        "hide":["/walls/Anime"],
        "exclude":["/videos/raw"],
        "workshop":"hide"
    }]}});
    Visibility {
        wallpaper: PathBuf::from("/walls"),
        videos: PathBuf::from("/videos"),
        profile: skwd_config::library_profile(&root, "public"),
    }
}

#[test]
fn profile_filters_kinds() {
    let visibility = public();
    for (key, name, video, shown) in [
        ("static:Anime/a.png", "Anime/a.png", "", false),
        ("static:Nature/b.png", "Nature/b.png", "", true),
        ("video:c.mp4", "c.mp4", "/videos/raw/c.mp4", false),
        ("video:d.mp4", "d.mp4", "/videos/d.mp4", true),
        ("we:1", "Scene", "", false),
        ("we:2", "Movie", "/videos/movie.mp4", false),
    ] {
        assert_eq!(visibility.shows(key, name, video), shown, "{name}");
    }
}

#[test]
fn hidden_items_skipped() {
    let visibility = Visibility {
        wallpaper: PathBuf::from("/walls"),
        videos: PathBuf::from("/videos"),
        profile: None,
    };
    let item = json!({"key":"static:a.png","name":"a.png","hidden":false});
    assert!(visibility.shows_item(&item));
    assert!(!visibility.shows_item(&json!({"key":"static:a.png","name":"a.png","hidden":true})));
}
