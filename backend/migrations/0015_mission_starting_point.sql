-- Phase 12, block B (docs/contracts/mission.md v1.2): what the person already
-- knows about the topic. Additive only: the column is nullable and existing
-- rows are left as they are (NULL = saved before v1.2, "not declared"). New
-- drafts always carry a value; that rule lives in the domain, not here.
ALTER TABLE missions ADD COLUMN starting_point TEXT;
