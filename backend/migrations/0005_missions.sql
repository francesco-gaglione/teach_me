-- Versioned missions of a topic (phase 3, docs/contracts/mission.md).
--
-- The list columns hold JSON arrays of strings. Timestamps follow 0002.
-- Two partial unique indexes carry the rules the domain relies on: at most
-- one open draft and at most one confirmed mission per topic.
-- `mission_constraints` avoids the SQL word `constraints`.

CREATE TABLE missions (
    topic_id            TEXT NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    version             INTEGER NOT NULL CHECK (version >= 1),
    status              TEXT NOT NULL CHECK (status IN ('draft', 'confirmed', 'superseded')),
    why                 TEXT NOT NULL,
    success_criteria    TEXT NOT NULL,
    mission_constraints TEXT NOT NULL,
    out_of_scope        TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    confirmed_at        TEXT,
    superseded_at       TEXT,
    PRIMARY KEY (topic_id, version),
    CHECK (status != 'draft' OR (confirmed_at IS NULL AND superseded_at IS NULL)),
    CHECK (status != 'confirmed' OR (confirmed_at IS NOT NULL AND superseded_at IS NULL)),
    CHECK (status != 'superseded' OR superseded_at IS NOT NULL)
) STRICT;

CREATE UNIQUE INDEX missions_one_draft_uq ON missions (topic_id) WHERE status = 'draft';
CREATE UNIQUE INDEX missions_one_confirmed_uq ON missions (topic_id) WHERE status = 'confirmed';
