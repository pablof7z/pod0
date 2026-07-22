CREATE TABLE pod0_download_environment(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    network_code INTEGER NOT NULL CHECK(network_code IN(1,2,3,4,255)),
    network_wire_code INTEGER,
    available_capacity_bytes INTEGER CHECK(available_capacity_bytes IS NULL OR available_capacity_bytes>=0),
    observed_at_ms INTEGER NOT NULL CHECK(observed_at_ms>=0),
    CHECK((network_code=255)=(network_wire_code IS NOT NULL))
) STRICT;

INSERT INTO pod0_download_environment(
    singleton,network_code,network_wire_code,available_capacity_bytes,observed_at_ms
) VALUES(1,1,NULL,NULL,0);

CREATE TABLE pod0_download_workflows(
    episode_id BLOB PRIMARY KEY NOT NULL CHECK(length(episode_id)=16)
        REFERENCES pod0_episodes(episode_id) ON DELETE CASCADE,
    intent_id BLOB NOT NULL CHECK(length(intent_id)=16),
    input_version TEXT NOT NULL CHECK(length(input_version)=64),
    origin_code INTEGER NOT NULL CHECK(origin_code IN(1,2,3,255)),
    origin_wire_code INTEGER,
    desired_state TEXT NOT NULL CHECK(desired_state IN('present','absent')),
    stage TEXT NOT NULL CHECK(stage IN(
        'waiting','requested','host_accepted','transferring','staged','retry_scheduled',
        'removing','cancelled','failed','succeeded'
    )),
    workflow_revision INTEGER NOT NULL CHECK(workflow_revision>=1),
    attempt INTEGER NOT NULL CHECK(attempt>=0 AND attempt<=65535),
    attempt_id BLOB CHECK(attempt_id IS NULL OR length(attempt_id)=16),
    request_id BLOB CHECK(request_id IS NULL OR length(request_id)=16),
    command_id BLOB NOT NULL CHECK(length(command_id)=16),
    cancellation_id BLOB NOT NULL CHECK(length(cancellation_id)=16),
    issued_revision INTEGER NOT NULL CHECK(issued_revision>=0),
    deadline_at_ms INTEGER,
    not_before_ms INTEGER,
    enclosure_url TEXT NOT NULL,
    resume_key TEXT,
    external_task_key TEXT,
    artifact_key TEXT,
    artifact_byte_count INTEGER CHECK(artifact_byte_count IS NULL OR artifact_byte_count>0),
    artifact_digest BLOB CHECK(artifact_digest IS NULL OR length(artifact_digest)=32),
    failure_code TEXT,
    failure_detail TEXT,
    failure_retryable INTEGER NOT NULL CHECK(failure_retryable IN(0,1)),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms>=0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms),
    CHECK((origin_code=255)=(origin_wire_code IS NOT NULL)),
    CHECK((attempt=0)=(attempt_id IS NULL)),
    CHECK((stage IN('requested','host_accepted','transferring','staged','retry_scheduled','removing'))=(request_id IS NOT NULL)),
    CHECK((stage='retry_scheduled')=(not_before_ms IS NOT NULL)),
    CHECK(stage NOT IN('succeeded','removing') OR artifact_key IS NOT NULL),
    CHECK(artifact_key IS NULL OR stage IN('succeeded','removing','failed')),
    CHECK((artifact_key IS NULL)=(artifact_byte_count IS NULL)),
    CHECK((artifact_key IS NULL)=(artifact_digest IS NULL)),
    CHECK((failure_code IS NULL) OR stage IN('waiting','retry_scheduled','failed'))
) STRICT;

CREATE UNIQUE INDEX pod0_download_intent_identity_v1
    ON pod0_download_workflows(intent_id);
CREATE UNIQUE INDEX pod0_download_current_attempt_identity_v1
    ON pod0_download_workflows(attempt_id) WHERE attempt_id IS NOT NULL;
CREATE INDEX pod0_download_workflow_page_v1
    ON pod0_download_workflows(updated_at_ms DESC,episode_id);
CREATE INDEX pod0_download_workflow_due_v1
    ON pod0_download_workflows(stage,not_before_ms,episode_id);

CREATE TABLE pod0_download_attempts(
    attempt_id BLOB PRIMARY KEY NOT NULL CHECK(length(attempt_id)=16),
    episode_id BLOB NOT NULL CHECK(length(episode_id)=16)
        REFERENCES pod0_download_workflows(episode_id) ON DELETE CASCADE,
    intent_id BLOB NOT NULL CHECK(length(intent_id)=16),
    attempt INTEGER NOT NULL CHECK(attempt>=1 AND attempt<=65535),
    state TEXT NOT NULL CHECK(state IN(
        'requested','host_accepted','transferring','staged','cancelled','failed','succeeded'
    )),
    request_id BLOB NOT NULL CHECK(length(request_id)=16),
    external_task_key TEXT,
    resume_key TEXT,
    staged_path TEXT,
    staged_byte_count INTEGER CHECK(staged_byte_count IS NULL OR staged_byte_count>0),
    staged_digest BLOB CHECK(staged_digest IS NULL OR length(staged_digest)=32),
    failure_code TEXT,
    failure_detail TEXT,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms>=0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms),
    UNIQUE(episode_id,intent_id,attempt),
    UNIQUE(request_id),
    CHECK((staged_path IS NULL)=(staged_byte_count IS NULL)),
    CHECK((staged_path IS NULL)=(staged_digest IS NULL)),
    CHECK((state='staged')=(staged_path IS NOT NULL))
) STRICT;

CREATE TABLE pod0_download_host_requests(
    request_id BLOB PRIMARY KEY NOT NULL CHECK(length(request_id)=16),
    episode_id BLOB NOT NULL CHECK(length(episode_id)=16)
        REFERENCES pod0_download_workflows(episode_id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK(kind IN('start','cancel','remove')),
    state TEXT NOT NULL CHECK(state IN('pending','completed','retired')),
    command_id BLOB NOT NULL CHECK(length(command_id)=16),
    cancellation_id BLOB NOT NULL CHECK(length(cancellation_id)=16),
    issued_revision INTEGER NOT NULL CHECK(issued_revision>=0),
    deadline_at_ms INTEGER,
    intent_id BLOB CHECK(intent_id IS NULL OR length(intent_id)=16),
    attempt_id BLOB CHECK(attempt_id IS NULL OR length(attempt_id)=16),
    input_version TEXT,
    enclosure_url TEXT,
    resume_key TEXT,
    external_task_key TEXT,
    artifact_key TEXT,
    last_sequence_number INTEGER CHECK(last_sequence_number IS NULL OR last_sequence_number>=0),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms>=0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms),
    CHECK(
        (kind='start' AND intent_id IS NOT NULL AND attempt_id IS NOT NULL
            AND input_version IS NOT NULL AND enclosure_url IS NOT NULL AND artifact_key IS NULL)
        OR (kind='cancel' AND intent_id IS NOT NULL AND attempt_id IS NOT NULL
            AND input_version IS NULL AND enclosure_url IS NULL AND artifact_key IS NULL)
        OR (kind='remove' AND intent_id IS NULL AND attempt_id IS NULL
            AND input_version IS NULL AND enclosure_url IS NULL AND artifact_key IS NOT NULL)
    )
) STRICT;

CREATE INDEX pod0_download_pending_host_requests_v1
    ON pod0_download_host_requests(state,created_at_ms,request_id);
