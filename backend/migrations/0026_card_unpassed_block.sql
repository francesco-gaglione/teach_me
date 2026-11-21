-- Review cards born "to consolidate" (issue 29, reviews.md v1.4).
--
-- `unpassed_block` is the id of the lesson exercise the learner never passed
-- and the card tests; NULL for every other card, including all those that
-- exist before this migration. Whether the card is still "to consolidate" is
-- derived on read from its votes, so nothing else is stored. Additive, so the
-- previous binary still reads the table.

ALTER TABLE review_cards ADD COLUMN unpassed_block TEXT;
