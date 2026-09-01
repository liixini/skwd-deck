mod assignments;
mod connection;
mod library;
mod maintenance;
mod playlists;
mod tags;
#[cfg(test)]
mod test_support;
mod wallpapers;
mod we_properties;

pub use assignments::{playlist_assign_clear, playlist_assign_set, playlist_assigns};
pub use connection::{open, open_in_memory};
pub use library::{LibraryImport, export_library, import_library};
pub use maintenance::{
    image_optimization_record, image_optimization_records, record_image_optimization_and_rename,
    rename_wallpaper_key, rollback_image_optimization_and_rename,
};
pub use playlists::{
    delete_member_by_key, playlist_add_member, playlist_create, playlist_delete,
    playlist_member_items, playlist_members, playlist_memberships_for_key, playlist_move_member,
    playlist_remove_member, playlist_toggle_member, playlist_update, playlists_all,
};
pub use tags::{
    backfill_effect_tags, effect_tag, merge_tag, parse_effect_tag, set_effect_tag, stem_key,
};
pub use wallpapers::{
    TINIER_CONVERT_MAX_BYTES, TINIER_CONVERT_PRESET, bump_apply_count, clear_cache, color_rows,
    delete_by_name, delete_entries, has_entry, item_count, key_for_video_file, known_keys,
    known_we_meta, list_wallpapers, list_wallpapers_json, random_pick, retire_video_converts,
    set_favourite, thumb_for_key, thumb_for_video, tinier_convert_delete, tinier_convert_entry,
    tinier_convert_record, tinier_convert_src, update_colors, update_duration, update_user_tags,
    upsert_cache_entry,
};
pub use we_properties::{
    MAX_WE_PROPERTIES, MAX_WE_PROPERTY_NAME, clear_we_properties, set_we_property,
    valid_property_name, we_properties,
};

#[cfg(test)]
use connection::{META_COLUMNS, is_corruption, migrate, open_at, quarantine};
#[cfg(test)]
use rusqlite::Connection;
#[cfg(test)]
pub(crate) use test_support::seed;

#[cfg(test)]
mod tests;
