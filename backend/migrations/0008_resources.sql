-- Sources found by the Researcher (phase 4, docs/contracts/resources.md) and
-- the result of a job. Timestamps follow 0002.
--
-- A topic's sources are replaced as a whole by each successful research, in
-- one transaction (see the repository), so there is no per-row lifecycle.
-- `position` is the order of importance chosen by the Researcher.
-- `domain` is not stored: it is derived from `url`.

ALTER TABLE jobs ADD COLUMN result TEXT;

CREATE TABLE resources (
    id          TEXT PRIMARY KEY,
    topic_id    TEXT NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    position    INTEGER NOT NULL CHECK (position >= 0),
    kind        TEXT NOT NULL CHECK (kind IN ('knowledge', 'community')),
    source_type TEXT NOT NULL,
    title       TEXT NOT NULL,
    url         TEXT,
    language    TEXT NOT NULL,
    annotation  TEXT NOT NULL,
    use_for     TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    UNIQUE (topic_id, position)
) STRICT;

CREATE TABLE research_gaps (
    topic_id TEXT NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    area     TEXT NOT NULL,
    note     TEXT NOT NULL,
    PRIMARY KEY (topic_id, position)
) STRICT;

-- The last successful research of the topic, if any.
CREATE TABLE topic_research (
    topic_id            TEXT PRIMARY KEY REFERENCES topics (id) ON DELETE CASCADE,
    job_id              TEXT NOT NULL,
    mission_version     INTEGER NOT NULL CHECK (mission_version >= 1),
    finished_at         TEXT NOT NULL,
    communities_skipped INTEGER NOT NULL CHECK (communities_skipped IN (0, 1))
) STRICT;
