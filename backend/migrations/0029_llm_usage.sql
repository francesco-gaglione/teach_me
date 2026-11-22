-- Tokens spent on the model per user and UTC day (input + output), for the
-- daily budget (docs/contracts/api-conventions.md section 12). One row per
-- user and day; old days are harmless and tiny, so nothing prunes them.
-- `day` is an ISO date (YYYY-MM-DD).

CREATE TABLE llm_usage (
    user_id TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    day     TEXT NOT NULL,
    tokens  INTEGER NOT NULL CHECK (tokens >= 0),
    PRIMARY KEY (user_id, day)
) STRICT;
