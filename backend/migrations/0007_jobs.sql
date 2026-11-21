-- Background jobs (phase 4, docs/mvp.md §7): research now, lesson generation
-- later. Timestamps follow 0002.
--
-- `kind` has no CHECK on purpose: adding a kind must not need a table rebuild;
-- the adapter rejects unknown values on read. `progress` is a JSON object
-- {step, done, total}. The partial unique index is the one-active-job-per-
-- (topic, kind) rule.

CREATE TABLE jobs (
    id            TEXT PRIMARY KEY,
    topic_id      TEXT NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    kind          TEXT NOT NULL,
    status        TEXT NOT NULL CHECK (status IN ('queued', 'running', 'succeeded', 'failed')),
    progress      TEXT,
    error_code    TEXT,
    error_message TEXT,
    attempt       INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    created_at    TEXT NOT NULL,
    started_at    TEXT,
    finished_at   TEXT,
    CHECK ((status = 'failed') = (error_code IS NOT NULL))
) STRICT;

CREATE UNIQUE INDEX jobs_one_active_uq ON jobs (topic_id, kind) WHERE status IN ('queued', 'running');
CREATE INDEX jobs_topic_idx ON jobs (topic_id, created_at DESC, id DESC);
CREATE INDEX jobs_queue_idx ON jobs (created_at, id) WHERE status = 'queued';
