-- Chat messages with the Tutor (phase 2).
--
-- `parts` is a JSON array of typed parts ([{"type":"text","text":"..."}]) so
-- native parts can be added later without a migration. Ordering is
-- (created_at, id), same conventions as 0002.

CREATE TABLE chat_messages (
    id            TEXT PRIMARY KEY NOT NULL,
    topic_id      TEXT NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    role          TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    status        TEXT NOT NULL CHECK (status IN ('streaming', 'complete', 'failed')),
    parts         TEXT NOT NULL DEFAULT '[]',
    error_code    TEXT,
    error_message TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    CHECK ((status = 'failed') = (error_code IS NOT NULL AND error_message IS NOT NULL)),
    CHECK (role = 'assistant' OR status = 'complete')
) STRICT;

CREATE INDEX chat_messages_topic_order_ix ON chat_messages (topic_id, created_at, id);

-- At most one reply being generated per topic: the database settles races
-- between concurrent sends.
CREATE UNIQUE INDEX chat_messages_one_streaming_uq
    ON chat_messages (topic_id) WHERE status = 'streaming';
