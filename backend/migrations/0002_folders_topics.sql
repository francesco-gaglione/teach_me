-- Folders and topics (phase 1).
--
-- Ids are canonical lowercase UUID text. Timestamps are RFC 3339 UTC text with
-- fixed millisecond precision ("2026-11-08T14:36:01.129Z"), so lexicographic
-- order equals chronological order and keyset pagination can compare them.
-- `*_key` columns hold the Unicode-lowercased name/title computed by the
-- domain: SQLite's lower() is ASCII-only, so case-insensitive uniqueness is
-- enforced on this precomputed key instead.

CREATE TABLE folders (
    id         TEXT PRIMARY KEY NOT NULL,
    name       TEXT NOT NULL,
    name_key   TEXT NOT NULL,
    position   INTEGER NOT NULL CHECK (position >= 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE UNIQUE INDEX folders_name_key_uq ON folders (name_key);
CREATE INDEX folders_order_ix ON folders (position, id);

CREATE TABLE topics (
    id         TEXT PRIMARY KEY NOT NULL,
    folder_id  TEXT NOT NULL REFERENCES folders (id) ON DELETE CASCADE,
    title      TEXT NOT NULL,
    title_key  TEXT NOT NULL,
    state      TEXT NOT NULL DEFAULT 'new'
               CHECK (state IN ('new', 'interviewing', 'mission_proposed', 'researching', 'ready')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE UNIQUE INDEX topics_folder_title_key_uq ON topics (folder_id, title_key);
CREATE INDEX topics_folder_order_ix ON topics (folder_id, created_at, id);
