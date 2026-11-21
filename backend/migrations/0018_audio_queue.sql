-- The internal queue of blocks to speak (ADR 0013 §11; not a row of `jobs`).
--
-- An entry exists while the clip of (lesson, voice, block) is missing; when the
-- clip is made the entry is deleted, so the clip cache stays the only truth
-- about what is done. `id` is the order of the requests. `bump` puts the lesson
-- opened last ahead of the rest: the worker takes the highest `bump` first,
-- then the lowest `id`. `attempts` counts failed tries (an engine that is down
-- does not count); at 3 the entry becomes `failed` and stays so until the
-- lesson is opened again. `running` entries found at startup are queued again.

CREATE TABLE audio_queue (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    lesson_id TEXT NOT NULL REFERENCES lessons (id) ON DELETE CASCADE,
    voice     TEXT NOT NULL,
    block_id  TEXT NOT NULL,
    text      TEXT NOT NULL,
    state     TEXT NOT NULL CHECK (state IN ('queued', 'running', 'failed')),
    attempts  INTEGER NOT NULL DEFAULT 0,
    bump      INTEGER NOT NULL DEFAULT 0,
    UNIQUE (lesson_id, voice, block_id)
) STRICT;

CREATE INDEX audio_queue_next ON audio_queue (state, bump DESC, id);
