use super::*;
use serde_json::json;

fn public() -> Value {
    json!({"library":{"profile":"public","profiles":[{
        "id":"public",
        "name":"Public",
        "hide":["/walls/Anime", "/walls/Games"],
        "show":["/walls/Games/Pokemon"],
        "exclude":["/walls/Sort/", "relative/Sus"],
        "workshop":"hide"
    }]}})
}

#[test]
fn nearest_folder_wins() {
    let profile = active_library_profile(&public()).expect("active");
    for (path, state) in [
        ("/walls/Anime/a.png", FolderState::Hide),
        ("/walls/Games/b.png", FolderState::Hide),
        ("/walls/Games/Pokemon/c.png", FolderState::Show),
        ("/walls/Sort/d.png", FolderState::Exclude),
        ("/walls/Animes/e.png", FolderState::Show),
        ("/walls/Anime/../f.png", FolderState::Show),
        ("relative/Sus/g.png", FolderState::Show),
    ] {
        assert_eq!(profile.state(LibraryItem::Folder(Path::new(path))), state, "{path}");
    }
    assert_eq!(profile.state(LibraryItem::Workshop), FolderState::Hide);
    assert_eq!(profile.state(LibraryItem::Other), FolderState::Show);
}

#[test]
fn item_kinds_classify() {
    assert_eq!(
        LibraryItem::new("static:a.png", "/walls/a.png"),
        LibraryItem::Folder(Path::new("/walls/a.png"))
    );
    assert_eq!(
        LibraryItem::new("video:b.mp4", "/videos/b.mp4"),
        LibraryItem::Folder(Path::new("/videos/b.mp4"))
    );
    assert_eq!(LibraryItem::new("we:123", "/steam/.local/movie.mp4"), LibraryItem::Workshop);
    assert_eq!(LibraryItem::new("static:c.png", ""), LibraryItem::Other);
    assert_eq!(LibraryItem::new("generative:pond", "/pond"), LibraryItem::Other);
}

#[test]
fn everything_shows_all() {
    let mut root = public();
    root["library"]["profile"] = json!(EVERYTHING_PROFILE);
    let active = active_library_profile(&root);
    assert!(active.is_none());
    let anime = LibraryItem::Folder(Path::new("/walls/Anime/a.png"));
    assert_eq!(profile_state(active.as_ref(), anime), FolderState::Show);
    assert_eq!(profile_state(None, LibraryItem::Workshop), FolderState::Show);
}

#[test]
fn invalid_entries_skipped() {
    let root = json!({"library":{"profiles":[
        {"name":"no id"},
        {"id":"everything"},
        42,
        {"id":"odd","workshop":"sideways","hide":["", "~/private"]}
    ]}});
    let profiles = library_profiles(&root);
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].state(LibraryItem::Workshop), FolderState::Show);
    let private = PathBuf::from(crate::home()).join("private/a.png");
    assert_eq!(profiles[0].state(LibraryItem::Folder(&private)), FolderState::Hide);
}

#[test]
fn state_wire_keys() {
    assert_eq!(
        [FolderState::Show, FolderState::Hide, FolderState::Exclude].map(FolderState::key),
        ["show", "hide", "exclude"]
    );
    assert_eq!(crate::keys::library::PROFILES, "library.profiles");
    assert_eq!(crate::keys::library::PROFILE, "library.profile");
    let entry = json!({"id": "a", "name": "A", "workshop": "exclude"});
    let profile = LibraryProfile::from_value(&entry).expect("entry");
    assert_eq!(profile.name, entry[crate::keys::library::profile::NAME]);
    assert_eq!(profile.id, entry[crate::keys::library::profile::ID]);
    assert_eq!(
        profile.state(LibraryItem::Workshop).key(),
        entry[crate::keys::library::profile::WORKSHOP]
    );
}
