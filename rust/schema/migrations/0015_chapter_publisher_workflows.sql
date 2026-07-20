CREATE TABLE pod0_publisher_chapter_workflows(
    episode_id BLOB PRIMARY KEY NOT NULL
        REFERENCES pod0_episodes(episode_id) ON DELETE CASCADE,
    source_url TEXT NOT NULL,
    source_version TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN(
        'requested','retry_scheduled','failed','cancelled','succeeded','source_absent'
    )),
    generation INTEGER NOT NULL CHECK(generation >= 1),
    workflow_revision INTEGER NOT NULL CHECK(workflow_revision >= 1),
    attempt INTEGER NOT NULL CHECK(attempt >= 1),
    max_attempts INTEGER NOT NULL CHECK(max_attempts >= 1),
    command_id BLOB NOT NULL CHECK(length(command_id)=16),
    cancellation_id BLOB NOT NULL CHECK(length(cancellation_id)=16),
    request_id BLOB CHECK(request_id IS NULL OR length(request_id)=16),
    issued_revision INTEGER NOT NULL CHECK(issued_revision >= 0),
    expected_selection_revision INTEGER NOT NULL CHECK(expected_selection_revision >= 0),
    deadline_at_ms INTEGER,
    not_before_ms INTEGER,
    selected_artifact_id BLOB
        REFERENCES pod0_chapter_artifacts(artifact_id),
    failure_code TEXT,
    failure_detail TEXT,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms),
    CHECK(
        (state IN('requested','retry_scheduled')
            AND request_id IS NOT NULL AND deadline_at_ms IS NOT NULL)
        OR state NOT IN('requested','retry_scheduled')
    ),
    CHECK(
        (state='retry_scheduled' AND not_before_ms IS NOT NULL AND failure_code IS NOT NULL)
        OR state!='retry_scheduled'
    ),
    CHECK(
        (state='succeeded' AND selected_artifact_id IS NOT NULL)
        OR state!='succeeded'
    )
) STRICT;

CREATE UNIQUE INDEX pod0_publisher_chapter_request_identity_v1
    ON pod0_publisher_chapter_workflows(request_id)
    WHERE request_id IS NOT NULL;

CREATE INDEX pod0_publisher_chapter_due_v1
    ON pod0_publisher_chapter_workflows(state,not_before_ms,episode_id);

CREATE INDEX pod0_publisher_chapter_updated_v1
    ON pod0_publisher_chapter_workflows(updated_at_ms DESC,episode_id);
