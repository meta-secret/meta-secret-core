CREATE TABLE db_commit_log (
    id       INTEGER NOT NULL,
    store    TEXT NOT NULL,
    vault_id TEXT,
    event    TEXT NOT NULL,
    PRIMARY KEY (id)
)