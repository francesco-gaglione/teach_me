-- Answers to the exercises of a lesson (phase 7). Timestamps follow 0002.
--
-- (lesson, block, number) is unique: the first attempt stored wins and a
-- resent queue creates no duplicates. `answer` is the JSON of lessons.md §2.1.
-- The grade columns are set together, only for a graded `open_answer`.

CREATE TABLE attempts (
    id                  TEXT PRIMARY KEY,
    lesson_id           TEXT NOT NULL REFERENCES lessons (id) ON DELETE CASCADE,
    block_id            TEXT NOT NULL,
    kind                TEXT NOT NULL,
    attempt_number      INTEGER NOT NULL CHECK (attempt_number IN (1, 2)),
    answer              TEXT NOT NULL,
    outcome             TEXT NOT NULL CHECK (outcome IN ('correct', 'partial', 'incorrect', 'pending')),
    grade_verdict       TEXT,
    grade_feedback      TEXT,
    grade_misconception TEXT,
    answered_at         TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    UNIQUE (lesson_id, block_id, attempt_number),
    CHECK ((grade_verdict IS NULL) = (grade_feedback IS NULL))
) STRICT;

CREATE INDEX attempts_lesson_idx ON attempts (lesson_id, created_at, id);
