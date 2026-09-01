#![cfg(test)]

use super::*;
use crate::db::{open_in_memory, playlist_assign_clear, playlist_assign_set, playlist_assigns};

#[test]
fn playlist_crud_roundtrip() {
    let conn = open_in_memory().unwrap();
    let id = playlist_create(&conn, "Chill").unwrap();
    let id2 = playlist_create(&conn, "Dark").unwrap();
    assert_eq!(playlists_all(&conn).unwrap().len(), 2);

    assert!(playlist_toggle_member(&conn, id, "static:a.png").unwrap());
    playlist_add_member(&conn, id, "static:b.png").unwrap();
    playlist_add_member(&conn, id, "video:c.mp4").unwrap();
    playlist_add_member(&conn, id, "static:a.png").unwrap();
    assert_eq!(
        playlist_members(&conn, id).unwrap(),
        vec!["static:a.png", "static:b.png", "video:c.mp4"]
    );

    assert_eq!(playlist_memberships_for_key(&conn, "static:a.png").unwrap(), vec![id]);
    assert!(!playlist_toggle_member(&conn, id, "static:a.png").unwrap());
    assert_eq!(playlist_members(&conn, id).unwrap(), vec!["static:b.png", "video:c.mp4"]);

    playlist_move_member(&conn, id, "video:c.mp4", -1).unwrap();
    assert_eq!(playlist_members(&conn, id).unwrap(), vec!["video:c.mp4", "static:b.png"]);

    playlist_assign_set(&conn, "DP-1", Some(id)).unwrap();
    playlist_assign_set(&conn, "*", Some(id2)).unwrap();
    playlist_assign_set(&conn, "DP-1", Some(id2)).unwrap();
    let mut assigns = playlist_assigns(&conn).unwrap();
    assigns.sort();
    assert_eq!(assigns, vec![("*".to_string(), id2), ("DP-1".to_string(), id2)]);
    playlist_assign_set(&conn, "DP-1", None).unwrap();
    assert_eq!(playlist_assigns(&conn).unwrap().len(), 1);

    playlist_assign_set(&conn, "DP-1", Some(id)).unwrap();
    playlist_assign_set(&conn, "DP-2", Some(id2)).unwrap();
    assert_eq!(playlist_assign_clear(&conn, Some(id)).unwrap(), 1);
    let after: Vec<i64> =
        playlist_assigns(&conn).unwrap().into_iter().map(|(_, pid)| pid).collect();
    assert!(!after.contains(&id) && after.contains(&id2));
    playlist_assign_clear(&conn, None).unwrap();
    assert!(playlist_assigns(&conn).unwrap().is_empty());

    playlist_update(
        &conn,
        id,
        Some("Chill v2"),
        Some("smart"),
        Some("tag:calm"),
        Some("sequential"),
        Some(900),
    )
    .unwrap();
    playlist_update(&conn, id, None, None, None, None, Some(120)).unwrap();
    let all = playlists_all(&conn).unwrap();
    let playlist = all.iter().find(|entry| entry.id == id).unwrap();
    assert_eq!(playlist.name, "Chill v2");
    assert_eq!(playlist.kind, "smart");
    assert_eq!(playlist.source.as_deref(), Some("tag:calm"));
    assert_eq!(playlist.order, "sequential");
    assert_eq!(playlist.dwell, 120);
    assert_eq!(playlist.count, 2);
    assert_eq!(serde_json::to_value(playlist).unwrap()["source"], "tag:calm");

    playlist_delete(&conn, id).unwrap();
    assert_eq!(playlists_all(&conn).unwrap().len(), 1);
    assert!(playlist_members(&conn, id).unwrap().is_empty());
}
