use super::Database;

#[test]
fn connection_access() {
    let database = Database::in_memory();

    assert_eq!(database.with_connection(crate::db::item_count).unwrap(), 0);
}
