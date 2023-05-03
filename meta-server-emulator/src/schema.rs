// @generated automatically by Diesel CLI.

diesel::table! {
    db_commit_log (id) {
        id -> Integer,
        key_id -> Text,
        store -> Text,
        vault_id -> Nullable<Text>,
        event -> Text,
    }
}
