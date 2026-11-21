-- Personal scheduling weights (issue 11, ADR 0014 points 5, 11, 12).
--
-- `user_review_parameters`: the weights a user's cards are scheduled with, one
-- row per user. No row means the model's default weights. `weights` is a JSON
-- array of numbers; `model` names the algorithm they are for.
CREATE TABLE user_review_parameters (
    user_id    TEXT PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    model      TEXT NOT NULL,
    weights    TEXT NOT NULL,
    adopted_at TEXT NOT NULL
);

-- Every try at personal weights, adopted or not. The log losses are of the
-- weights in use and of the candidates on the votes kept out of the training;
-- they are NULL when the attempt failed before computing them.
CREATE TABLE review_optimization_attempts (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id            TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    at                 TEXT NOT NULL,
    useful_votes       INTEGER NOT NULL CHECK (useful_votes >= 0),
    train_votes        INTEGER NOT NULL CHECK (train_votes >= 0),
    test_votes         INTEGER NOT NULL CHECK (test_votes >= 0),
    current_log_loss   REAL,
    candidate_log_loss REAL,
    outcome            TEXT NOT NULL CHECK (outcome IN ('adopted', 'rejected', 'failed')),
    error              TEXT
);

CREATE INDEX idx_review_optimization_attempts_user ON review_optimization_attempts (user_id, at);
