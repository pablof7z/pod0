CREATE TABLE pod0_scheduled_agent_authority(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    state TEXT NOT NULL CHECK(state IN('inactive','authoritative')),
    source_generation INTEGER CHECK(source_generation IS NULL OR source_generation>=1),
    core_revision INTEGER NOT NULL CHECK(core_revision>=0),
    committed_at_ms INTEGER,
    CHECK((state='inactive')=(source_generation IS NULL AND committed_at_ms IS NULL)),
    CHECK((state='authoritative')=(source_generation IS NOT NULL AND committed_at_ms IS NOT NULL))
) STRICT;

INSERT INTO pod0_scheduled_agent_authority(
    singleton,state,source_generation,core_revision,committed_at_ms
) VALUES(1,'inactive',NULL,0,NULL);

CREATE TABLE pod0_scheduled_tasks(
    task_id BLOB PRIMARY KEY NOT NULL CHECK(length(task_id)=16),
    label TEXT NOT NULL CHECK(length(CAST(label AS BLOB)) BETWEEN 1 AND 160),
    prompt TEXT NOT NULL CHECK(length(CAST(prompt AS BLOB)) BETWEEN 1 AND 32768),
    prompt_revision BLOB NOT NULL CHECK(length(prompt_revision)=32),
    model_reference TEXT NOT NULL
        CHECK(length(CAST(model_reference AS BLOB)) BETWEEN 1 AND 256),
    interval_ms INTEGER NOT NULL CHECK(interval_ms>=1),
    task_revision INTEGER NOT NULL CHECK(task_revision>=1),
    last_run_at_ms INTEGER,
    next_run_at_ms INTEGER NOT NULL,
    active INTEGER NOT NULL CHECK(active IN(0,1)),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms),
    removed_at_ms INTEGER,
    CHECK((active=1)=(removed_at_ms IS NULL))
) STRICT;

CREATE INDEX pod0_scheduled_tasks_due_v1
    ON pod0_scheduled_tasks(active,next_run_at_ms,task_id);

CREATE TABLE pod0_scheduled_occurrences(
    occurrence_id BLOB PRIMARY KEY NOT NULL CHECK(length(occurrence_id)=16),
    task_id BLOB NOT NULL CHECK(length(task_id)=16),
    scheduled_for_ms INTEGER NOT NULL,
    prompt TEXT NOT NULL CHECK(length(CAST(prompt AS BLOB)) BETWEEN 1 AND 32768),
    prompt_revision BLOB NOT NULL CHECK(length(prompt_revision)=32),
    model_reference TEXT NOT NULL
        CHECK(length(CAST(model_reference AS BLOB)) BETWEEN 1 AND 256),
    stage TEXT NOT NULL CHECK(stage IN(
        'pending','requested','host_accepted','retry_scheduled','blocked','cancelled',
        'obsolete','failed_permanent','succeeded','ambiguous'
    )),
    workflow_revision INTEGER NOT NULL CHECK(workflow_revision>=1),
    attempt INTEGER NOT NULL CHECK(attempt BETWEEN 0 AND 65535),
    attempt_id BLOB UNIQUE CHECK(attempt_id IS NULL OR length(attempt_id)=16),
    request_id BLOB UNIQUE CHECK(request_id IS NULL OR length(request_id)=16),
    provider_operation_id TEXT
        CHECK(length(CAST(provider_operation_id AS BLOB)) BETWEEN 1 AND 1024),
    not_before_ms INTEGER,
    artifact_id BLOB UNIQUE CHECK(artifact_id IS NULL OR length(artifact_id)=16),
    output_digest BLOB CHECK(output_digest IS NULL OR length(output_digest)=32),
    failure_code TEXT CHECK(length(CAST(failure_code AS BLOB)) BETWEEN 1 AND 64),
    failure_wire_code INTEGER CHECK(failure_wire_code IS NULL OR failure_wire_code>=0),
    failure_detail TEXT CHECK(length(CAST(failure_detail AS BLOB))<=1024),
    failure_retryable INTEGER NOT NULL CHECK(failure_retryable IN(0,1)),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms),
    UNIQUE(task_id,scheduled_for_ms),
    CHECK(attempt=0 OR (attempt_id IS NOT NULL AND request_id IS NOT NULL)),
    CHECK(stage!='retry_scheduled' OR not_before_ms IS NOT NULL),
    CHECK(stage!='host_accepted' OR attempt_id IS NOT NULL),
    CHECK(stage!='succeeded' OR (artifact_id IS NOT NULL AND output_digest IS NOT NULL))
) STRICT;

CREATE INDEX pod0_scheduled_occurrences_current_v1
    ON pod0_scheduled_occurrences(task_id,updated_at_ms DESC,occurrence_id);
CREATE INDEX pod0_scheduled_occurrences_recovery_v1
    ON pod0_scheduled_occurrences(stage,not_before_ms,occurrence_id);

CREATE TABLE pod0_scheduled_attempts(
    attempt_id BLOB PRIMARY KEY NOT NULL CHECK(length(attempt_id)=16),
    occurrence_id BLOB NOT NULL REFERENCES pod0_scheduled_occurrences(occurrence_id),
    attempt INTEGER NOT NULL CHECK(attempt BETWEEN 1 AND 65535),
    request_id BLOB UNIQUE NOT NULL CHECK(length(request_id)=16),
    command_id BLOB NOT NULL CHECK(length(command_id)=16),
    cancellation_id BLOB NOT NULL CHECK(length(cancellation_id)=16),
    issued_revision INTEGER NOT NULL CHECK(issued_revision>=0),
    deadline_at_ms INTEGER NOT NULL,
    state TEXT NOT NULL CHECK(state IN(
        'requested','host_accepted','retry_scheduled','blocked','failed','cancelled',
        'succeeded','ambiguous'
    )),
    provider_operation_id TEXT
        CHECK(length(CAST(provider_operation_id AS BLOB)) BETWEEN 1 AND 1024),
    last_sequence_number INTEGER CHECK(last_sequence_number IS NULL OR last_sequence_number>=0),
    last_observation_fingerprint BLOB
        CHECK(last_observation_fingerprint IS NULL OR length(last_observation_fingerprint)=32),
    failure_code TEXT CHECK(length(CAST(failure_code AS BLOB)) BETWEEN 1 AND 64),
    failure_wire_code INTEGER CHECK(failure_wire_code IS NULL OR failure_wire_code>=0),
    failure_detail TEXT CHECK(length(CAST(failure_detail AS BLOB))<=1024),
    failure_retryable INTEGER NOT NULL CHECK(failure_retryable IN(0,1)),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms),
    UNIQUE(occurrence_id,attempt),
    CHECK((last_sequence_number IS NULL)=(last_observation_fingerprint IS NULL))
) STRICT;

CREATE TABLE pod0_scheduled_completion_evidence(
    attempt_id BLOB PRIMARY KEY NOT NULL REFERENCES pod0_scheduled_attempts(attempt_id),
    occurrence_id BLOB UNIQUE NOT NULL REFERENCES pod0_scheduled_occurrences(occurrence_id),
    request_id BLOB UNIQUE NOT NULL CHECK(length(request_id)=16),
    artifact_id BLOB UNIQUE NOT NULL CHECK(length(artifact_id)=16),
    output_digest BLOB NOT NULL CHECK(length(output_digest)=32),
    output_excerpt TEXT NOT NULL
        CHECK(length(CAST(output_excerpt AS BLOB)) BETWEEN 1 AND 16384),
    sequence_number INTEGER NOT NULL CHECK(sequence_number>=0),
    observation_fingerprint BLOB NOT NULL CHECK(length(observation_fingerprint)=32),
    observed_at_ms INTEGER NOT NULL,
    state TEXT NOT NULL CHECK(state IN('observed','committed')),
    committed_at_ms INTEGER,
    CHECK((state='committed')=(committed_at_ms IS NOT NULL))
) STRICT;

CREATE TABLE pod0_generated_artifacts(
    artifact_id BLOB PRIMARY KEY NOT NULL CHECK(length(artifact_id)=16),
    occurrence_id BLOB UNIQUE NOT NULL REFERENCES pod0_scheduled_occurrences(occurrence_id),
    attempt_id BLOB UNIQUE NOT NULL REFERENCES pod0_scheduled_attempts(attempt_id),
    kind TEXT NOT NULL CHECK(kind='scheduled_agent_output'),
    content_digest BLOB NOT NULL CHECK(length(content_digest)=32),
    bounded_excerpt TEXT NOT NULL
        CHECK(length(CAST(bounded_excerpt AS BLOB)) BETWEEN 1 AND 16384),
    selected_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE pod0_scheduled_command_receipts(
    command_id BLOB PRIMARY KEY NOT NULL CHECK(length(command_id)=16),
    command_fingerprint BLOB NOT NULL CHECK(length(command_fingerprint)=32),
    task_id BLOB CHECK(task_id IS NULL OR length(task_id)=16),
    occurrence_id BLOB CHECK(occurrence_id IS NULL OR length(occurrence_id)=16),
    applied_revision INTEGER NOT NULL CHECK(applied_revision>=1),
    completed_at_ms INTEGER NOT NULL
) STRICT;
