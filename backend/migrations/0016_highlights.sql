-- The learner's highlights on a lesson (lessons.md §10, ADR 0011).
--
-- The app sends the whole set and the backend replaces it, so a row has no id
-- of its own: `position` is its place in the set as returned (document order,
-- then start). The range is in UTF-16 code units of the block's `text` source,
-- hence `range_start`/`range_end` (`end` is an SQL keyword). Overlaps are
-- checked before storing, where the document is known. Deleting the lesson
-- (or its topic) deletes its highlights.

CREATE TABLE highlights (
    lesson_id   TEXT NOT NULL REFERENCES lessons (id) ON DELETE CASCADE,
    position    INTEGER NOT NULL CHECK (position >= 0),
    block_id    TEXT NOT NULL,
    range_start INTEGER NOT NULL CHECK (range_start >= 0),
    range_end   INTEGER NOT NULL,
    color       TEXT NOT NULL CHECK (color IN ('yellow', 'green', 'blue', 'pink')),
    PRIMARY KEY (lesson_id, position),
    CHECK (range_start < range_end)
) STRICT;
