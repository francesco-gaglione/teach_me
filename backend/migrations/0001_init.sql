-- Pipeline smoke test: a tiny key/value table describing the schema.
CREATE TABLE schema_info (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) STRICT;

INSERT INTO schema_info (key, value) VALUES ('created_by', 'teach-me');
