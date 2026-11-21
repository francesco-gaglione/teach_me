-- Id of the last SSE event whose content is in the stored message (chat.md §1).
-- Nullable: rows from before this column keep NULL.
ALTER TABLE chat_messages ADD COLUMN last_event_id INTEGER;
