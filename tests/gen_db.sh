#!/bin/bash
set -e

DB_PATH="data/oj.sqlite3"

mkdir -p data

sqlite3 "$DB_PATH" <<EOF
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 2000;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;

CREATE TABLE IF NOT EXISTS users (
    id            INTEGER PRIMARY KEY,
    name          TEXT    NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS jobs (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    created_time  TEXT    NOT NULL,
    updated_time  TEXT    NOT NULL,
    user_id       INTEGER NOT NULL,
    contest_id    INTEGER,
    problem_id    INTEGER NOT NULL,
    source_code   TEXT    NOT NULL,
    language      TEXT    NOT NULL,
    state         TEXT    NOT NULL,
    result        TEXT    NOT NULL,
    score         REAL,
    FOREIGN KEY (user_id)  REFERENCES users (id)
);

CREATE INDEX IF NOT EXISTS idx_jobs_created_time ON jobs(created_time);

CREATE TABLE job_case (
    job_id         INTEGER      NOT NULL,
    case_index     INTEGER      NOT NULL,
    result         TEXT         NOT NULL,
    time_us        INTEGER      NOT NULL,
    memory_kb      INTEGER      NOT NULL,
    info           TEXT         DEFAULT '',
    PRIMARY KEY (job_id, case_index),
    FOREIGN KEY (job_id)  REFERENCES jobs (id)
);

INSERT OR IGNORE INTO users (id, name) VALUES (0, 'root');
EOF

echo "Database created at $DB_PATH"