-- Issue #14: non-blocking quality warnings of the saved document, as a JSON
-- array of {code, block_id, message}. Older lessons read as none. Additive.

ALTER TABLE lessons ADD COLUMN quality_warnings TEXT NOT NULL DEFAULT '[]';
