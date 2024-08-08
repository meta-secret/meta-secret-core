CREATE TABLE db_commit_log (
    id       INTEGER NOT NULL,
    key_id   TEXT NOT NULL,
    event    TEXT NOT NULL,
    PRIMARY KEY (id)
)