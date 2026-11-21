-- Why and when a card was suspended (issue 12, reviews.md v1.3).
--
-- `suspension` is `user` (the learner's choice, reversible) or `superseded`
-- (its record was replaced, never reactivated); both are NULL while the card
-- is active. Additive, so the previous binary still reads the table.

ALTER TABLE review_cards ADD COLUMN suspension TEXT
    CHECK (suspension IN ('user', 'superseded'));
ALTER TABLE review_cards ADD COLUMN suspended_at TEXT;

-- Cards suspended before this migration: superseded when the record they come
-- from is, otherwise the learner's. The real instant was not kept, so it is
-- the one of the migration.
UPDATE review_cards
SET suspension = CASE
        WHEN record_id IS NOT NULL AND EXISTS (
            SELECT 1 FROM learning_records r
            WHERE r.id = review_cards.record_id AND r.status = 'superseded')
        THEN 'superseded'
        ELSE 'user'
    END,
    suspended_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE status = 'suspended';

-- The list of the cards suspended by hand: newest first.
CREATE INDEX review_cards_suspended_idx ON review_cards (suspended_at DESC, id)
    WHERE suspension = 'user';
