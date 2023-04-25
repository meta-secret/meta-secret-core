// @generated automatically by Diesel CLI.

diesel::table! {
    db_commit_log (id) {
        id -> Integer,
        store -> Text,
        vault_id -> Nullable<Text>,
        event -> Text,
    }
}
