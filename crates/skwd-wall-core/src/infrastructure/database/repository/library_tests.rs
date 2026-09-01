#![cfg(test)]

use super::*;
use crate::db::{
    effect_tag, list_wallpapers, open_in_memory, seed, set_favourite, update_user_tags,
};

fn set_we_id(conn: &Connection, key: &str, we_id: &str) {
    conn.execute("UPDATE meta SET we_id = ?1 WHERE key = ?2", params![we_id, key]).unwrap();
}

#[test]
fn library_roundtrip() {
    let src = open_in_memory().unwrap();
    seed(&src, "static:a.png", "a.png", "static");
    seed(&src, "static:b.png", "b.png", "static");
    seed(&src, "we:12345", "scene", "we");
    set_we_id(&src, "we:12345", "12345");
    set_favourite(&src, "static:a.png", true).unwrap();
    update_user_tags(&src, "static:a.png", "forest,green").unwrap();
    update_user_tags(&src, "we:12345", "space").unwrap();
    set_effect_tag(&src, "effects/x-sepia", "sepia").unwrap();

    let jsonl = export_library(&src).unwrap();
    assert!(jsonl.contains("static:a.png"));
    assert!(jsonl.contains("forest,green"));
    assert!(jsonl.contains("sepia"));
    assert!(!jsonl.contains("static:b.png"));

    let dst = open_in_memory().unwrap();
    seed(&dst, "static:a.png", "a.png", "static");
    seed(&dst, "we:88888", "scene", "we");
    set_we_id(&dst, "we:88888", "12345");

    let stats = import_library(&dst, &jsonl).unwrap();
    assert_eq!(stats.matched, 2);
    assert_eq!(stats.missing, 0);
    assert_eq!(stats.effects, 1);

    let list = list_wallpapers(&dst, false).unwrap();
    let image = list.iter().find(|item| item["key"] == "static:a.png").unwrap();
    assert_eq!(image["favourite"], 1);
    assert_eq!(image["tags"], "forest,green");
    let we = list.iter().find(|item| item["key"] == "we:88888").unwrap();
    assert_eq!(we["tags"], "space");
    assert_eq!(effect_tag(&dst, "effects/x-sepia").as_deref(), Some("sepia"));

    let empty = open_in_memory().unwrap();
    let only_ghost =
        import_library(&empty, "{\"key\":\"static:none.png\",\"fav\":true}\n").unwrap();
    assert_eq!(only_ghost, LibraryImport { matched: 0, missing: 1, effects: 0 });
}
