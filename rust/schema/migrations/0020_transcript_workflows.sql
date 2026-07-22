CREATE TABLE pod0_transcript_workflow_imports(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    source_generation INTEGER UNIQUE NOT NULL CHECK(source_generation>=1),
    source_fingerprint BLOB NOT NULL CHECK(length(source_fingerprint)=32),
    backup_digest BLOB NOT NULL CHECK(length(backup_digest)=32),
    backup_byte_count INTEGER NOT NULL CHECK(backup_byte_count>=0),
    row_count INTEGER NOT NULL CHECK(row_count>=0),
    state TEXT NOT NULL CHECK(state IN('staged','verified','authoritative')),
    staged_at_ms INTEGER NOT NULL CHECK(staged_at_ms>=0),
    verified_at_ms INTEGER,
    committed_at_ms INTEGER,
    CHECK((state='staged')=(verified_at_ms IS NULL)),
    CHECK((state='authoritative')=(committed_at_ms IS NOT NULL))
) STRICT;

CREATE TABLE pod0_transcript_workflow_import_rows(
    source_generation INTEGER NOT NULL,
    ordinal INTEGER NOT NULL CHECK(ordinal>=0),
    row_fingerprint BLOB NOT NULL CHECK(length(row_fingerprint)=32),
    row_bytes BLOB NOT NULL CHECK(length(row_bytes)<=1048576),
    classification TEXT NOT NULL CHECK(classification IN(
        'restart','recover_provider','ambiguous','blocked','failed','cancelled',
        'succeeded','index_pending','index_succeeded','obsolete'
    )),
    episode_id BLOB NOT NULL CHECK(length(episode_id)=16),
    PRIMARY KEY(source_generation,ordinal),
    UNIQUE(source_generation,row_fingerprint),
    FOREIGN KEY(source_generation) REFERENCES pod0_transcript_workflow_imports(source_generation)
        ON DELETE CASCADE
) STRICT;

CREATE TABLE pod0_transcript_workflows(
    episode_id BLOB PRIMARY KEY NOT NULL
        REFERENCES pod0_episodes(episode_id) ON DELETE CASCADE,
    workflow_id BLOB UNIQUE NOT NULL CHECK(length(workflow_id)=16),
    stage TEXT NOT NULL CHECK(stage IN(
        'awaiting_prerequisite','requested','publisher_requested',
        'submission_authorized','provider_accepted','completion_observed',
        'transcript_committed','evidence_requested','retry_scheduled','blocked',
        'failed','cancelled','succeeded'
    )),
    source_revision TEXT NOT NULL
        CHECK(length(CAST(source_revision AS BLOB)) BETWEEN 1 AND 256),
    origin TEXT NOT NULL CHECK(length(CAST(origin AS BLOB)) BETWEEN 1 AND 64),
    provider TEXT NOT NULL CHECK(length(CAST(provider AS BLOB)) BETWEEN 1 AND 256),
    model TEXT NOT NULL CHECK(length(CAST(model AS BLOB)) BETWEEN 1 AND 256),
    remote_audio_url TEXT NOT NULL
        CHECK(length(CAST(remote_audio_url AS BLOB)) BETWEEN 1 AND 8192),
    local_audio_url TEXT CHECK(length(CAST(local_audio_url AS BLOB)) BETWEEN 1 AND 8192),
    publisher_transcript_url TEXT
        CHECK(length(CAST(publisher_transcript_url AS BLOB)) BETWEEN 1 AND 8192),
    publisher_mime_hint TEXT
        CHECK(length(CAST(publisher_mime_hint AS BLOB)) BETWEEN 1 AND 128),
    publisher_first INTEGER NOT NULL CHECK(publisher_first IN(0,1)),
    provider_fallback_enabled INTEGER NOT NULL CHECK(provider_fallback_enabled IN(0,1)),
    workflow_revision INTEGER NOT NULL CHECK(workflow_revision>=1),
    attempt INTEGER NOT NULL CHECK(attempt>=0),
    max_attempts INTEGER NOT NULL CHECK(max_attempts BETWEEN 1 AND 65535),
    attempt_id BLOB UNIQUE CHECK(attempt_id IS NULL OR length(attempt_id)=16),
    submission_fence_id BLOB UNIQUE
        CHECK(submission_fence_id IS NULL OR length(submission_fence_id)=16),
    command_id BLOB NOT NULL CHECK(length(command_id)=16),
    cancellation_id BLOB NOT NULL CHECK(length(cancellation_id)=16),
    request_id BLOB UNIQUE CHECK(request_id IS NULL OR length(request_id)=16),
    issued_revision INTEGER NOT NULL CHECK(issued_revision>=0),
    deadline_at_ms INTEGER,
    not_before_ms INTEGER,
    submission_authorized_at_ms INTEGER,
    external_operation_id TEXT
        CHECK(length(CAST(external_operation_id AS BLOB)) BETWEEN 1 AND 1024),
    provider_status TEXT CHECK(length(CAST(provider_status AS BLOB))<=1024),
    completion_artifact_id BLOB REFERENCES pod0_transcript_artifacts(artifact_id),
    committed_artifact_id BLOB REFERENCES pod0_transcript_artifacts(artifact_id),
    committed_transcript_version_id BLOB
        CHECK(committed_transcript_version_id IS NULL OR length(committed_transcript_version_id)=16),
    committed_content_digest BLOB
        CHECK(committed_content_digest IS NULL OR length(committed_content_digest)=32),
    expected_selection_revision INTEGER NOT NULL CHECK(expected_selection_revision>=0),
    resulting_selection_revision INTEGER
        CHECK(resulting_selection_revision IS NULL OR resulting_selection_revision>=1),
    evidence_input_version TEXT
        CHECK(length(CAST(evidence_input_version AS BLOB)) BETWEEN 1 AND 256),
    failure_code TEXT CHECK(length(CAST(failure_code AS BLOB)) BETWEEN 1 AND 256),
    failure_detail TEXT CHECK(length(CAST(failure_detail AS BLOB))<=1024),
    failure_retryable INTEGER NOT NULL DEFAULT 0 CHECK(failure_retryable IN(0,1)),
    may_have_submitted INTEGER NOT NULL DEFAULT 0 CHECK(may_have_submitted IN(0,1)),
    source_generation INTEGER,
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms>=0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms),
    CHECK(attempt=0 OR (attempt_id IS NOT NULL AND submission_fence_id IS NOT NULL)),
    CHECK(stage NOT IN('requested','publisher_requested','retry_scheduled') OR request_id IS NOT NULL),
    CHECK(stage!='retry_scheduled' OR not_before_ms IS NOT NULL),
    CHECK(stage NOT IN('submission_authorized','provider_accepted')
        OR (attempt_id IS NOT NULL AND submission_fence_id IS NOT NULL
            AND submission_authorized_at_ms IS NOT NULL AND may_have_submitted=1)),
    CHECK(stage!='provider_accepted' OR external_operation_id IS NOT NULL),
    CHECK(stage!='completion_observed' OR completion_artifact_id IS NOT NULL),
    CHECK(stage NOT IN('transcript_committed','evidence_requested','succeeded') OR (
        committed_artifact_id IS NOT NULL AND committed_transcript_version_id IS NOT NULL
        AND committed_content_digest IS NOT NULL AND resulting_selection_revision IS NOT NULL
    )),
    CHECK(stage NOT IN('evidence_requested','succeeded') OR evidence_input_version IS NOT NULL)
) STRICT;

CREATE INDEX pod0_transcript_workflows_due_v1
    ON pod0_transcript_workflows(stage,not_before_ms,episode_id);
CREATE INDEX pod0_transcript_workflows_updated_v1
    ON pod0_transcript_workflows(updated_at_ms DESC,episode_id);

CREATE TABLE pod0_transcript_attempts(
    attempt_id BLOB PRIMARY KEY NOT NULL CHECK(length(attempt_id)=16),
    workflow_id BLOB NOT NULL REFERENCES pod0_transcript_workflows(workflow_id) ON DELETE CASCADE,
    episode_id BLOB NOT NULL REFERENCES pod0_episodes(episode_id) ON DELETE CASCADE,
    attempt INTEGER NOT NULL CHECK(attempt>=1),
    submission_fence_id BLOB UNIQUE NOT NULL CHECK(length(submission_fence_id)=16),
    request_id BLOB UNIQUE NOT NULL CHECK(length(request_id)=16),
    state TEXT NOT NULL CHECK(state IN(
        'prepared','authorized','provider_accepted','completion_observed',
        'retry_scheduled','ambiguous','failed','cancelled','committed'
    )),
    authorized_at_ms INTEGER,
    external_operation_id TEXT
        CHECK(length(CAST(external_operation_id AS BLOB)) BETWEEN 1 AND 1024),
    provider_status TEXT CHECK(length(CAST(provider_status AS BLOB))<=1024),
    completion_artifact_id BLOB REFERENCES pod0_transcript_artifacts(artifact_id),
    failure_code TEXT CHECK(length(CAST(failure_code AS BLOB)) BETWEEN 1 AND 256),
    failure_detail TEXT CHECK(length(CAST(failure_detail AS BLOB))<=1024),
    may_have_submitted INTEGER NOT NULL DEFAULT 0 CHECK(may_have_submitted IN(0,1)),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms>=0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms),
    UNIQUE(workflow_id,attempt),
    CHECK(state NOT IN('authorized','provider_accepted','completion_observed','ambiguous','committed')
        OR (authorized_at_ms IS NOT NULL AND may_have_submitted=1)),
    CHECK(state!='provider_accepted' OR external_operation_id IS NOT NULL),
    CHECK(state NOT IN('completion_observed','committed') OR completion_artifact_id IS NOT NULL)
) STRICT;

CREATE TABLE pod0_transcript_evidence_requests(
    workflow_id BLOB PRIMARY KEY NOT NULL
        REFERENCES pod0_transcript_workflows(workflow_id) ON DELETE CASCADE,
    episode_id BLOB NOT NULL REFERENCES pod0_episodes(episode_id) ON DELETE CASCADE,
    transcript_version_id BLOB NOT NULL CHECK(length(transcript_version_id)=16),
    content_digest BLOB NOT NULL CHECK(length(content_digest)=32),
    input_version TEXT NOT NULL
        CHECK(length(CAST(input_version AS BLOB)) BETWEEN 1 AND 256),
    state TEXT NOT NULL CHECK(state IN('requested','completed')),
    requested_at_ms INTEGER NOT NULL CHECK(requested_at_ms>=0),
    completed_at_ms INTEGER,
    CHECK((state='completed')=(completed_at_ms IS NOT NULL)),
    UNIQUE(episode_id,input_version)
) STRICT;
