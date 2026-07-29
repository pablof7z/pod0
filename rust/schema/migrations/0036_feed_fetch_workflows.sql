-- Durable feed-fetch workflow family (issue #189). One row per normalized
-- feed identity: the PRIMARY KEY is the uniqueness constraint that makes
-- concurrent subscribes coalesce onto one workflow instead of racing at the
-- row level. Rows are deleted when the fetch applies; terminal failures keep
-- the row in stage 'failed' until a new command replaces it.
CREATE TABLE pod0_feed_fetch_workflows(
    feed_key_v1 TEXT PRIMARY KEY NOT NULL,
    source_url TEXT NOT NULL,
    podcast_id BLOB NOT NULL CHECK(length(podcast_id)=16)
        REFERENCES pod0_podcasts(podcast_id) ON DELETE CASCADE,
    intent TEXT NOT NULL CHECK(intent IN('subscribe','ensure','refresh','metadata')),
    stage TEXT NOT NULL CHECK(stage IN('requested','retry_scheduled','failed')),
    attempt INTEGER NOT NULL CHECK(attempt>=1 AND attempt<=65535),
    request_id BLOB NOT NULL UNIQUE CHECK(length(request_id)=16),
    command_id BLOB NOT NULL CHECK(length(command_id)=16),
    command_fingerprint TEXT NOT NULL,
    cancellation_id BLOB NOT NULL CHECK(length(cancellation_id)=16),
    issued_revision INTEGER NOT NULL CHECK(issued_revision>=0),
    deadline_at_ms INTEGER CHECK(deadline_at_ms IS NULL OR deadline_at_ms>=0),
    not_before_ms INTEGER CHECK(not_before_ms IS NULL OR not_before_ms>=0),
    entity_tag TEXT,
    last_modified TEXT,
    failure_code TEXT,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms>=0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms),
    CHECK((stage='retry_scheduled')=(not_before_ms IS NOT NULL)),
    CHECK((failure_code IS NULL) OR stage IN('retry_scheduled','failed'))
) STRICT;

CREATE INDEX pod0_feed_fetch_workflows_due_v1
    ON pod0_feed_fetch_workflows(stage,not_before_ms,feed_key_v1);
