-- What a completed lesson leaves behind (phase 7): learning records, glossary
-- terms and review cards. Timestamps follow 0002.
--
-- `number` of a record is per topic (LR-NNNN). `term_key` is the Unicode
-- lower-cased term, precomputed by the domain for the alphabetical order
-- (SQLite's lower() is ASCII only). A card belongs to exactly one record or
-- term. `lesson_completions` has one row per recorded lesson: it makes the
-- save idempotent and keeps the counts of a lesson whose Recorder produced
-- nothing. Review scheduling (phase 8) will add its own tables.

CREATE TABLE learning_records (
    id            TEXT PRIMARY KEY,
    topic_id      TEXT NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    number        INTEGER NOT NULL CHECK (number >= 1),
    kind          TEXT NOT NULL CHECK (kind IN ('demonstrated', 'misconception_corrected')),
    title         TEXT NOT NULL,
    body          TEXT NOT NULL,
    evidence      TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'superseded')),
    superseded_by TEXT REFERENCES learning_records (id) ON DELETE SET NULL,
    lesson_id     TEXT NOT NULL REFERENCES lessons (id) ON DELETE CASCADE,
    created_at    TEXT NOT NULL,
    UNIQUE (topic_id, number)
) STRICT;

CREATE INDEX learning_records_lesson_idx ON learning_records (lesson_id);

CREATE TABLE glossary_terms (
    id         TEXT PRIMARY KEY,
    topic_id   TEXT NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    key        TEXT NOT NULL,
    term       TEXT NOT NULL,
    term_key   TEXT NOT NULL,
    definition TEXT NOT NULL,
    avoid      TEXT NOT NULL,
    lesson_id  TEXT NOT NULL REFERENCES lessons (id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (topic_id, key)
) STRICT;

CREATE INDEX glossary_terms_order_idx ON glossary_terms (topic_id, term_key, key);
CREATE INDEX glossary_terms_lesson_idx ON glossary_terms (lesson_id);

CREATE TABLE review_cards (
    id         TEXT PRIMARY KEY,
    topic_id   TEXT NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    lesson_id  TEXT NOT NULL REFERENCES lessons (id) ON DELETE CASCADE,
    record_id  TEXT REFERENCES learning_records (id) ON DELETE CASCADE,
    term_id    TEXT REFERENCES glossary_terms (id) ON DELETE CASCADE,
    front      TEXT NOT NULL,
    back       TEXT NOT NULL,
    status     TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended')),
    created_at TEXT NOT NULL,
    CHECK ((record_id IS NULL) <> (term_id IS NULL))
) STRICT;

CREATE INDEX review_cards_topic_idx ON review_cards (topic_id, created_at, id);
CREATE INDEX review_cards_lesson_idx ON review_cards (lesson_id);

CREATE TABLE lesson_completions (
    lesson_id         TEXT PRIMARY KEY REFERENCES lessons (id) ON DELETE CASCADE,
    record_count      INTEGER NOT NULL CHECK (record_count >= 0),
    term_count        INTEGER NOT NULL CHECK (term_count >= 0),
    review_card_count INTEGER NOT NULL CHECK (review_card_count >= 0),
    created_at        TEXT NOT NULL
) STRICT;
