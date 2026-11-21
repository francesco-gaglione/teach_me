-- Metadata of the spoken clips of lessons (ADR 0013, docs/contracts/audio.md).
--
-- A clip is content-addressed: `key` is the hash of spoken text, engine, model,
-- voice and parameters, so a clip never changes and a missing key just means
-- "generate it". `text_hash` is the hash of the spoken text alone: it finds the
-- same text read by another voice (the fallback while a new voice is made).
-- The audio bytes are files named after `id` in the audio directory, a
-- rebuildable cache kept out of the database and its backups.

CREATE TABLE audio_clips (
    id          TEXT PRIMARY KEY,
    key         TEXT NOT NULL UNIQUE,
    text_hash   TEXT NOT NULL,
    voice       TEXT NOT NULL,
    duration_ms INTEGER NOT NULL CHECK (duration_ms > 0),
    size_bytes  INTEGER NOT NULL CHECK (size_bytes > 0),
    created_at  TEXT NOT NULL
) STRICT;

CREATE INDEX audio_clips_by_text ON audio_clips (text_hash, created_at);
