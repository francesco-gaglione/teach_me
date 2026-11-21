-- Read-aloud, stage 2b (ADR 0013, docs/contracts/audio.md §4.2, §5).
--
-- `users.tts_voice` is the voice the user chose, as saved: it can name a voice
-- the server no longer lists, in which case the default applies (worked out
-- by the application, not stored). NULL means "never chose".
--
-- `audio_clip_grants` records which clips a user was shown in a manifest: the
-- permission to read the file. A voice sample needs no row (anybody can read
-- it). Rows go with the user and with the clip.

ALTER TABLE users ADD COLUMN tts_voice TEXT;

CREATE TABLE audio_clip_grants (
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    clip_id    TEXT NOT NULL REFERENCES audio_clips (id) ON DELETE CASCADE,
    granted_at TEXT NOT NULL,
    PRIMARY KEY (user_id, clip_id)
) STRICT, WITHOUT ROWID;
