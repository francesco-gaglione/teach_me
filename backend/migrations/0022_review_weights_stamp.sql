-- Which weights a card's memory was computed with (issue 11, ADR 0014 point 12).
--
-- `weights_adopted_at` is the `adopted_at` of the user's row in
-- `user_review_parameters` that the memory values (`stability`, `difficulty`,
-- `stability_fast`) were computed with; NULL means the model's default
-- weights. A vote, the startup migration and a rebuild write it. A card whose
-- stamp differs from the weights in use is out of date: the daily pass
-- rebuilds it, and a vote replays its history first. This is what lets an
-- interrupted rebuild be resumed.
ALTER TABLE review_cards ADD COLUMN weights_adopted_at TEXT;
