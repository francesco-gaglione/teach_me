-- Language of the content of a topic (ADR 0019). Existing topics are Italian.
-- Immutable: the API never updates it.
ALTER TABLE topics ADD COLUMN language TEXT NOT NULL DEFAULT 'it' CHECK (language IN ('it', 'en'));

-- The language the audio queue entry is spoken in (the topic's, copied so the
-- worker does not need the lesson).
ALTER TABLE audio_queue ADD COLUMN language TEXT NOT NULL DEFAULT 'it' CHECK (language IN ('it', 'en'));
