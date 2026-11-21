-- no-transaction
-- Multi-user (phase 11, ADR 0008): users and sessions; `folders`, `topics` and
-- `preferences` get an owner.
--
-- SQLite cannot add a NOT NULL column that references another table, so the
-- three tables are rebuilt (the procedure of https://sqlite.org/lang_altertable.html
-- section 7). Other tables point at `folders` and `topics` with ON DELETE
-- CASCADE: dropping those parents with foreign keys on would run the cascade
-- and delete every lesson, message and card. That is why this file opts out
-- of sqlx's transaction (the first line) and switches the enforcement off
-- around its own: the PRAGMA has no effect inside a transaction. The pool
-- hands the whole migration run one connection, so the PRAGMAs and the
-- transaction below are on the same one.
--
-- The rebuilt tables keep every id, so the children's references stay valid;
-- `foreign_key_check` at the end reports any that would not.

PRAGMA foreign_keys = OFF;

BEGIN;

-- `username` is stored in lowercase by the application. `password_hash` is
-- NULL for an account that cannot log in yet (the initial `owner`). `llm_key`
-- is the user's LLM API key sealed with the server's key (AEAD); it is never
-- written in the clear.
CREATE TABLE users (
    id            TEXT PRIMARY KEY NOT NULL,
    username      TEXT NOT NULL,
    display_name  TEXT NOT NULL,
    password_hash TEXT,
    llm_key       BLOB,
    disabled_at   TEXT,
    last_login_at TEXT,
    created_at    TEXT NOT NULL
) STRICT;

CREATE UNIQUE INDEX users_username_uq ON users (username);

-- Opaque tokens are stored as their SHA-256 digest only.
CREATE TABLE sessions (
    id           TEXT PRIMARY KEY NOT NULL,
    user_id      TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_digest BLOB NOT NULL,
    device_name  TEXT,
    created_at   TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    expires_at   TEXT NOT NULL
) STRICT;

CREATE UNIQUE INDEX sessions_token_digest_uq ON sessions (token_digest);
CREATE INDEX sessions_user_ix ON sessions (user_id);
CREATE INDEX sessions_expires_ix ON sessions (expires_at);

-- The id is the constant `UserId::OWNER` of the domain crate.
INSERT INTO users (id, username, display_name, created_at)
VALUES ('00000000-0000-7000-8000-000000000001', 'owner', 'owner',
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

-- Folders: names are unique per user, positions are per user.
CREATE TABLE folders_new (
    id         TEXT PRIMARY KEY NOT NULL,
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    name_key   TEXT NOT NULL,
    position   INTEGER NOT NULL CHECK (position >= 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (id, user_id)
) STRICT;

INSERT INTO folders_new (id, user_id, name, name_key, position, created_at, updated_at)
SELECT id, '00000000-0000-7000-8000-000000000001', name, name_key, position, created_at,
       updated_at
FROM folders;

-- Topics: `user_id` is a copy of the folder's, and the composite foreign key
-- makes the database refuse a topic whose owner differs from its folder's.
CREATE TABLE topics_new (
    id         TEXT PRIMARY KEY NOT NULL,
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    folder_id  TEXT NOT NULL,
    title      TEXT NOT NULL,
    title_key  TEXT NOT NULL,
    state      TEXT NOT NULL DEFAULT 'new'
               CHECK (state IN ('new', 'interviewing', 'mission_proposed', 'researching', 'ready')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (id, user_id),
    FOREIGN KEY (folder_id, user_id) REFERENCES folders (id, user_id) ON DELETE CASCADE
) STRICT;

INSERT INTO topics_new (id, user_id, folder_id, title, title_key, state, created_at, updated_at)
SELECT id, '00000000-0000-7000-8000-000000000001', folder_id, title, title_key, state,
       created_at, updated_at
FROM topics;

-- Preferences: a global one belongs to the user; a topic one must be in a
-- topic of the same user (the composite key is not enforced while `topic_id`
-- is NULL).
CREATE TABLE preferences_new (
    id         TEXT PRIMARY KEY NOT NULL,
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    scope      TEXT NOT NULL CHECK (scope IN ('global', 'topic')),
    topic_id   TEXT,
    text       TEXT NOT NULL,
    text_key   TEXT NOT NULL,
    created_at TEXT NOT NULL,
    CHECK ((scope = 'topic') = (topic_id IS NOT NULL)),
    FOREIGN KEY (topic_id, user_id) REFERENCES topics (id, user_id) ON DELETE CASCADE
) STRICT;

INSERT INTO preferences_new (id, user_id, scope, topic_id, text, text_key, created_at)
SELECT id, '00000000-0000-7000-8000-000000000001', scope, topic_id, text, text_key, created_at
FROM preferences;

DROP TABLE preferences;
DROP TABLE topics;
DROP TABLE folders;

ALTER TABLE folders_new RENAME TO folders;
ALTER TABLE topics_new RENAME TO topics;
ALTER TABLE preferences_new RENAME TO preferences;

CREATE UNIQUE INDEX folders_user_name_key_uq ON folders (user_id, name_key);
CREATE INDEX folders_user_order_ix ON folders (user_id, position, id);

CREATE UNIQUE INDEX topics_folder_title_key_uq ON topics (folder_id, title_key);
CREATE INDEX topics_folder_order_ix ON topics (folder_id, created_at, id);
CREATE INDEX topics_user_ix ON topics (user_id);

CREATE UNIQUE INDEX preferences_dedupe_uq
    ON preferences (user_id, COALESCE(topic_id, ''), text_key);
CREATE INDEX preferences_order_ix ON preferences (user_id, created_at, id);

-- Reports (as rows) any reference left dangling; the test of this migration
-- asserts there are none.
PRAGMA foreign_key_check;

COMMIT;

PRAGMA foreign_keys = ON;
