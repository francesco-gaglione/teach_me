-- Lessons (phase 6, docs/contracts/lessons.md). Timestamps follow 0002.
--
-- `document` is the complete SDUI document, `rubric` included: the API strips
-- it on the way out. `plan` is what the Planner decided. `generation_job_id`
-- is unique so a generation run again after a crash cannot save a second
-- lesson. `number` is assigned by the repository (max + 1 per topic) and has
-- no gaps because lessons are never deleted on their own.

CREATE TABLE lessons (
    id                TEXT PRIMARY KEY,
    topic_id          TEXT NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    number            INTEGER NOT NULL CHECK (number >= 1),
    title             TEXT NOT NULL,
    objective         TEXT NOT NULL,
    estimated_minutes INTEGER NOT NULL CHECK (estimated_minutes >= 1),
    status            TEXT NOT NULL DEFAULT 'not_started'
                      CHECK (status IN ('not_started', 'in_progress', 'completed')),
    catalog_version   INTEGER NOT NULL CHECK (catalog_version >= 1),
    mission_version   INTEGER NOT NULL CHECK (mission_version >= 1),
    document          TEXT NOT NULL,
    plan              TEXT NOT NULL,
    generation_job_id TEXT NOT NULL,
    created_at        TEXT NOT NULL,
    completed_at      TEXT,
    UNIQUE (topic_id, number),
    UNIQUE (generation_job_id),
    CHECK ((status = 'completed') = (completed_at IS NOT NULL))
) STRICT;

-- Blocks that asked for a component the catalog does not have (API: phase 9).
CREATE TABLE component_requests (
    id         TEXT PRIMARY KEY,
    topic_id   TEXT NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    lesson_id  TEXT NOT NULL REFERENCES lessons (id) ON DELETE CASCADE,
    block_id   TEXT NOT NULL,
    wanted     TEXT NOT NULL,
    purpose    TEXT NOT NULL,
    fallback   TEXT NOT NULL,
    created_at TEXT NOT NULL
) STRICT;

CREATE INDEX component_requests_lesson_idx ON component_requests (lesson_id);

-- `lesson_completion` jobs belong to a lesson: their one-active-job rule is
-- per (lesson, kind); every other kind keeps the per-(topic, kind) rule.
ALTER TABLE jobs ADD COLUMN lesson_id TEXT REFERENCES lessons (id) ON DELETE CASCADE;

DROP INDEX jobs_one_active_uq;
CREATE UNIQUE INDEX jobs_one_active_uq ON jobs (topic_id, kind)
    WHERE status IN ('queued', 'running') AND lesson_id IS NULL;
CREATE UNIQUE INDEX jobs_one_active_lesson_uq ON jobs (lesson_id, kind)
    WHERE status IN ('queued', 'running') AND lesson_id IS NOT NULL;
