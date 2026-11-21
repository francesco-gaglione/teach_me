-- FSRS scheduling of review cards (phase 8, docs/contracts/reviews.md).
-- Timestamps follow 0002 (fixed-width UTC text, so they compare as strings).
--
-- `stability`/`difficulty` are the FSRS memory values (0 until the first
-- vote). `review_count` counts applied votes, undone ones excluded; it is the
-- version of the card for the idempotent vote. A card is reviewable when it
-- is `active` and `due` <= now.

ALTER TABLE review_cards ADD COLUMN state TEXT NOT NULL DEFAULT 'new'
    CHECK (state IN ('new', 'learning', 'review', 'relearning'));
ALTER TABLE review_cards ADD COLUMN stability REAL NOT NULL DEFAULT 0;
ALTER TABLE review_cards ADD COLUMN difficulty REAL NOT NULL DEFAULT 0;
-- A constant is required for the new column; the backfill below sets the real one.
ALTER TABLE review_cards ADD COLUMN due TEXT NOT NULL DEFAULT '';
ALTER TABLE review_cards ADD COLUMN last_reviewed_at TEXT;
ALTER TABLE review_cards ADD COLUMN review_count INTEGER NOT NULL DEFAULT 0
    CHECK (review_count >= 0);
ALTER TABLE review_cards ADD COLUMN lapses INTEGER NOT NULL DEFAULT 0
    CHECK (lapses >= 0);

-- Cards made before scheduling existed are new: due a day after creation.
UPDATE review_cards SET due = strftime('%Y-%m-%dT%H:%M:%fZ', created_at, '+24 hours');

CREATE INDEX review_cards_due_idx ON review_cards (due, id) WHERE status = 'active';

-- One row per applied vote. `state_before` and `state_after` are JSON snapshots of the
-- card's schedule: `state_before` restores the card on undo, both feed statistics.
CREATE TABLE review_logs (
    id          TEXT PRIMARY KEY,
    card_id     TEXT NOT NULL REFERENCES review_cards (id) ON DELETE CASCADE,
    rating      TEXT NOT NULL CHECK (rating IN ('again', 'hard', 'good', 'easy')),
    reviewed_at TEXT NOT NULL,
    state_before TEXT NOT NULL,
    state_after  TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    undone_at   TEXT
) STRICT;

CREATE INDEX review_logs_card_idx ON review_logs (card_id);
