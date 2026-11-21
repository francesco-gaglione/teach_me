-- Uploaded PDFs (phase 10, docs/contracts/documents.md). Timestamps follow 0002.
--
-- The original file is not stored: only the text of each page and the sections
-- built from it. `status` and the error are what the import last wrote; the
-- adapter reports `importing`/`failed` from the import job when the two
-- differ (a crash between the job and the document), so the job is the truth.
-- `resource_id` has no foreign key: the resource row points back at the
-- document, and both are written in the same transaction.

CREATE TABLE documents (
    id            TEXT PRIMARY KEY,
    topic_id      TEXT NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    resource_id   TEXT,
    filename      TEXT NOT NULL,
    title         TEXT NOT NULL,
    status        TEXT NOT NULL CHECK (status IN ('importing', 'ready', 'failed')),
    size_bytes    INTEGER NOT NULL CHECK (size_bytes >= 0),
    page_count    INTEGER CHECK (page_count IS NULL OR page_count >= 0),
    language      TEXT,
    error_code    TEXT,
    error_message TEXT,
    created_at    TEXT NOT NULL,
    CHECK ((error_code IS NULL) = (error_message IS NULL))
) STRICT;

CREATE INDEX documents_topic_idx ON documents (topic_id, created_at DESC, id DESC);

CREATE TABLE document_pages (
    document_id TEXT NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
    page        INTEGER NOT NULL CHECK (page >= 1),
    text        TEXT NOT NULL,
    PRIMARY KEY (document_id, page)
) STRICT;

CREATE TABLE document_sections (
    document_id TEXT NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
    position    INTEGER NOT NULL CHECK (position >= 0),
    section_id  TEXT NOT NULL,
    title       TEXT NOT NULL,
    level       INTEGER NOT NULL CHECK (level BETWEEN 1 AND 3),
    page_start  INTEGER NOT NULL CHECK (page_start >= 1),
    page_end    INTEGER NOT NULL CHECK (page_end >= page_start),
    PRIMARY KEY (document_id, position),
    UNIQUE (document_id, section_id)
) STRICT;

-- `document_import` jobs belong to a document. Their one-active-job rule is
-- the (topic, kind) index of 0009, which already covers jobs without a lesson.
ALTER TABLE jobs ADD COLUMN document_id TEXT REFERENCES documents (id) ON DELETE CASCADE;
CREATE UNIQUE INDEX jobs_document_uq ON jobs (document_id) WHERE document_id IS NOT NULL;

-- `resources.kind` gets a third value, and a CHECK cannot be altered: the
-- table is rebuilt. Nothing references `resources`. Web sources keep their
-- `position` (order chosen by the Researcher); a document has no position of
-- its own: documents are listed first, newest first.
CREATE TABLE resources_new (
    id          TEXT PRIMARY KEY,
    topic_id    TEXT NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    position    INTEGER NOT NULL CHECK (position >= 0),
    kind        TEXT NOT NULL CHECK (kind IN ('knowledge', 'community', 'document')),
    source_type TEXT NOT NULL,
    title       TEXT NOT NULL,
    url         TEXT,
    language    TEXT NOT NULL,
    annotation  TEXT NOT NULL,
    use_for     TEXT NOT NULL,
    document_id TEXT REFERENCES documents (id) ON DELETE CASCADE,
    created_at  TEXT NOT NULL,
    CHECK ((kind = 'document') = (document_id IS NOT NULL))
) STRICT;

INSERT INTO resources_new (id, topic_id, position, kind, source_type, title, url, language,
                           annotation, use_for, document_id, created_at)
SELECT id, topic_id, position, kind, source_type, title, url, language, annotation, use_for,
       NULL, created_at
FROM resources;

DROP TABLE resources;
ALTER TABLE resources_new RENAME TO resources;

CREATE UNIQUE INDEX resources_web_position_uq ON resources (topic_id, position)
    WHERE kind <> 'document';
CREATE UNIQUE INDEX resources_document_uq ON resources (document_id)
    WHERE document_id IS NOT NULL;
