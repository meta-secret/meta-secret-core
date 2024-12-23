// @generated automatically by Diesel CLI.

diesel::table! {
    db_commit_log (id) {
        id -> Integer,
        key_id -> Text,
        event -> Text,
    }
}
