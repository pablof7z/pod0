CREATE TABLE pod0_new_episode_notification_settings(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    schema_version INTEGER NOT NULL CHECK(schema_version=1),
    enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
    revision INTEGER NOT NULL CHECK(revision>=0),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms>=0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms)
) STRICT;

INSERT INTO pod0_new_episode_notification_settings(
    singleton,schema_version,enabled,revision,created_at_ms,updated_at_ms
) VALUES(1,1,1,0,0,0);

CREATE TABLE pod0_feed_discovery_workflows(
    occurrence_id BLOB PRIMARY KEY NOT NULL CHECK(length(occurrence_id)=16)
        REFERENCES pod0_feed_discovery_occurrences(occurrence_id) ON DELETE CASCADE,
    stage TEXT NOT NULL CHECK(stage IN ('active','succeeded','expired','cancelled','failed')),
    workflow_revision INTEGER NOT NULL CHECK(workflow_revision>=1),
    expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms>=0),
    planned_at_ms INTEGER NOT NULL CHECK(planned_at_ms>=0),
    completed_at_ms INTEGER CHECK(completed_at_ms IS NULL OR completed_at_ms>=planned_at_ms),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=planned_at_ms)
) STRICT;

CREATE INDEX pod0_feed_discovery_workflows_active_v1
    ON pod0_feed_discovery_workflows(stage,updated_at_ms,occurrence_id);

CREATE TABLE pod0_feed_discovery_effects(
    occurrence_id BLOB NOT NULL CHECK(length(occurrence_id)=16)
        REFERENCES pod0_feed_discovery_occurrences(occurrence_id) ON DELETE CASCADE,
    episode_id BLOB NOT NULL CHECK(length(episode_id)=16)
        REFERENCES pod0_episodes(episode_id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK(kind IN ('download','notification')),
    stage TEXT NOT NULL CHECK(
        stage IN ('pending','requested','retry_scheduled','succeeded','obsolete','failed')
    ),
    command_id BLOB CHECK(command_id IS NULL OR length(command_id)=16),
    cancellation_id BLOB NOT NULL CHECK(length(cancellation_id)=16),
    request_id BLOB UNIQUE CHECK(request_id IS NULL OR length(request_id)=16),
    attempt INTEGER NOT NULL CHECK(attempt>=0 AND attempt<=4),
    not_before_ms INTEGER CHECK(not_before_ms IS NULL OR not_before_ms>=0),
    deadline_at_ms INTEGER CHECK(deadline_at_ms IS NULL OR deadline_at_ms>=0),
    failure_code TEXT,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms>=0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms),
    PRIMARY KEY(occurrence_id,episode_id,kind)
) STRICT;

CREATE INDEX pod0_feed_discovery_effects_pending_v1
    ON pod0_feed_discovery_effects(kind,stage,not_before_ms,updated_at_ms,occurrence_id,episode_id);
