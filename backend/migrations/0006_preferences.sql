-- Preferences saved by the Tutor (phase 3, docs/contracts/mission.md).
--
-- `scope = 'topic'` rows name their topic and go away with it. `text_key` is
-- the Unicode-lowercased text computed by the domain (see 0002 for why), so
-- the same instruction cannot be stored twice for the same scope.

CREATE TABLE preferences (
    id         TEXT PRIMARY KEY NOT NULL,
    scope      TEXT NOT NULL CHECK (scope IN ('global', 'topic')),
    topic_id   TEXT REFERENCES topics (id) ON DELETE CASCADE,
    text       TEXT NOT NULL,
    text_key   TEXT NOT NULL,
    created_at TEXT NOT NULL,
    CHECK ((scope = 'topic') = (topic_id IS NOT NULL))
) STRICT;

CREATE UNIQUE INDEX preferences_dedupe_uq ON preferences (COALESCE(topic_id, ''), text_key);
CREATE INDEX preferences_order_ix ON preferences (created_at, id);
