-- The learner's handwritten notes on a lesson (lessons.md §11, ADR 0015).
--
-- Like highlights, the app sends the whole set and the backend replaces it,
-- so a row has no id of its own in the database key: `position` is the
-- drawing order (the first is underneath) and must be preserved. `id` is the
-- app's UUID, kept as sent and unique in the lesson (the domain compares it
-- ignoring case, so this index never sees two spellings of the same id).
--
-- `points` is a BLOB of little-endian i32, two per point (x then y). A stroke
-- can have 1000 points and a lesson 200 000: packed, that is 4 bytes per
-- coordinate instead of ~6-7 as JSON text, and the adapter needs no JSON
-- parsing to read it back. The contract's ranges (-40000..200000) fit an i32.
-- Nothing queries inside the points, so being opaque costs nothing.
-- Deleting the lesson (or its topic) deletes its strokes.

CREATE TABLE strokes (
    lesson_id TEXT NOT NULL REFERENCES lessons (id) ON DELETE CASCADE,
    position  INTEGER NOT NULL CHECK (position >= 0),
    id        TEXT NOT NULL,
    block_id  TEXT NOT NULL,
    color     TEXT NOT NULL CHECK (color IN ('ink', 'blue', 'red', 'green')),
    width     TEXT NOT NULL CHECK (width IN ('thin', 'medium', 'thick')),
    points    BLOB NOT NULL CHECK (length(points) >= 8 AND length(points) % 8 = 0),
    PRIMARY KEY (lesson_id, position),
    UNIQUE (lesson_id, id)
) STRICT;
