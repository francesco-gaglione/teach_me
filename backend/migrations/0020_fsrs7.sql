-- FSRS-7 scheduling (issue 11, ADR 0014). Additive: the FSRS-5 columns stay,
-- so the previous binary still reads the table.
--
-- `stability_fast` is the second memory trace of FSRS-7 (0 until a card is
-- scheduled by it). `learning_step` is the index of the next short step of a
-- new, learning or relearning card. `model` names the algorithm that produced
-- the memory values: existing cards stay `fsrs5` until the startup migration
-- replays their votes with FSRS-7; cards created from now on are born `fsrs7`.

ALTER TABLE review_cards ADD COLUMN stability_fast REAL NOT NULL DEFAULT 0;
ALTER TABLE review_cards ADD COLUMN learning_step INTEGER NOT NULL DEFAULT 0
    CHECK (learning_step >= 0);
ALTER TABLE review_cards ADD COLUMN model TEXT NOT NULL DEFAULT 'fsrs5';
