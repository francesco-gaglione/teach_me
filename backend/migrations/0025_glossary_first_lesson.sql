-- The lesson that first added a glossary term (issue 17, lessons.md v1.5).
--
-- `lesson_id` moves to the latest reviser; `first_lesson_id` never does.
-- Nullable and additive, so the previous binary still reads the table.

ALTER TABLE glossary_terms ADD COLUMN first_lesson_id TEXT
    REFERENCES lessons (id) ON DELETE SET NULL;

-- A term never revised still points at the lesson that created it. The origin
-- of the revised ones was overwritten, so it stays unknown (NULL).
UPDATE glossary_terms SET first_lesson_id = lesson_id WHERE created_at = updated_at;
