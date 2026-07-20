CREATE TABLE pod0_model_chapter_workflows(
    episode_id BLOB PRIMARY KEY NOT NULL
        REFERENCES pod0_episodes(episode_id) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK(state IN(
        'awaiting_transcript','awaiting_publisher','preserved','requested',
        'submission_authorized','provider_accepted','ambiguous',
        'completion_observed','retry_scheduled','blocked','failed',
        'cancelled','succeeded'
    )),
    desired_configured_model TEXT NOT NULL,
    active_configured_model TEXT,
    replan_pending INTEGER NOT NULL DEFAULT 0 CHECK(replan_pending IN(0,1)),
    mode TEXT CHECK(mode IS NULL OR mode IN('generate','enrich')),
    source_version TEXT,
    request_fingerprint BLOB
        CHECK(request_fingerprint IS NULL OR length(request_fingerprint)=32),
    generation INTEGER NOT NULL CHECK(generation >= 0),
    workflow_revision INTEGER NOT NULL CHECK(workflow_revision >= 1),
    attempt INTEGER NOT NULL CHECK(attempt >= 0),
    max_attempts INTEGER NOT NULL CHECK(max_attempts >= 1),
    command_id BLOB NOT NULL CHECK(length(command_id)=16),
    cancellation_id BLOB NOT NULL CHECK(length(cancellation_id)=16),
    request_id BLOB CHECK(request_id IS NULL OR length(request_id)=16),
    submission_fence_id BLOB
        CHECK(submission_fence_id IS NULL OR length(submission_fence_id)=16),
    issued_revision INTEGER NOT NULL CHECK(issued_revision >= 0),
    deadline_at_ms INTEGER,
    not_before_ms INTEGER,
    submission_authorized_at_ms INTEGER,
    requested_transcript_version_id BLOB
        CHECK(requested_transcript_version_id IS NULL
            OR length(requested_transcript_version_id)=16),
    requested_transcript_digest BLOB
        CHECK(requested_transcript_digest IS NULL
            OR length(requested_transcript_digest)=32),
    selected_transcript_version_id BLOB
        CHECK(selected_transcript_version_id IS NULL
            OR length(selected_transcript_version_id)=16),
    selected_transcript_digest BLOB
        CHECK(selected_transcript_digest IS NULL
            OR length(selected_transcript_digest)=32),
    expected_selection_revision INTEGER
        CHECK(expected_selection_revision IS NULL OR expected_selection_revision >= 0),
    base_artifact_id BLOB
        REFERENCES pod0_chapter_artifacts(artifact_id),
    base_integrity_digest BLOB
        CHECK(base_integrity_digest IS NULL OR length(base_integrity_digest)=32),
    format_version INTEGER CHECK(format_version IS NULL OR format_version >= 1),
    policy_version INTEGER CHECK(policy_version IS NULL OR policy_version >= 1),
    provider TEXT,
    model TEXT,
    response_format_code INTEGER,
    maximum_completion_bytes INTEGER
        CHECK(maximum_completion_bytes IS NULL OR maximum_completion_bytes >= 1),
    duration_ms INTEGER CHECK(duration_ms IS NULL OR duration_ms >= 0),
    expected_artifact_source_code INTEGER,
    system_prompt TEXT,
    user_prompt TEXT,
    provider_operation_id TEXT,
    provider_status TEXT,
    selected_artifact_id BLOB
        REFERENCES pod0_chapter_artifacts(artifact_id),
    failure_code TEXT,
    failure_detail TEXT,
    may_have_submitted INTEGER NOT NULL DEFAULT 0
        CHECK(may_have_submitted IN(0,1)),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= created_at_ms),
    CHECK(
        mode!='enrich'
        OR (base_artifact_id IS NOT NULL AND base_integrity_digest IS NOT NULL)
    ),
    CHECK(
        state NOT IN(
            'requested','submission_authorized','provider_accepted','ambiguous',
            'completion_observed','retry_scheduled','succeeded'
        )
        OR (
            active_configured_model IS NOT NULL
            AND mode IS NOT NULL
            AND source_version IS NOT NULL
            AND request_fingerprint IS NOT NULL
            AND generation >= 1
            AND attempt >= 1
            AND request_id IS NOT NULL
            AND submission_fence_id IS NOT NULL
            AND requested_transcript_version_id IS NOT NULL
            AND requested_transcript_digest IS NOT NULL
            AND selected_transcript_version_id IS NOT NULL
            AND selected_transcript_digest IS NOT NULL
            AND expected_selection_revision IS NOT NULL
            AND format_version IS NOT NULL
            AND policy_version IS NOT NULL
            AND provider IS NOT NULL
            AND model IS NOT NULL
            AND response_format_code IS NOT NULL
            AND maximum_completion_bytes IS NOT NULL
            AND expected_artifact_source_code IS NOT NULL
            AND system_prompt IS NOT NULL
            AND user_prompt IS NOT NULL
        )
    ),
    CHECK(
        state NOT IN('requested','retry_scheduled') OR deadline_at_ms IS NOT NULL
    ),
    CHECK(state!='retry_scheduled' OR not_before_ms IS NOT NULL),
    CHECK(
        state NOT IN(
            'submission_authorized','provider_accepted','ambiguous',
            'completion_observed','succeeded'
        ) OR (submission_authorized_at_ms IS NOT NULL AND may_have_submitted=1)
    ),
    CHECK(state!='provider_accepted' OR provider_operation_id IS NOT NULL),
    CHECK(state!='succeeded' OR selected_artifact_id IS NOT NULL)
) STRICT;

CREATE UNIQUE INDEX pod0_model_chapter_request_identity_v1
    ON pod0_model_chapter_workflows(request_id)
    WHERE request_id IS NOT NULL;

CREATE UNIQUE INDEX pod0_model_chapter_submission_fence_v1
    ON pod0_model_chapter_workflows(submission_fence_id)
    WHERE submission_fence_id IS NOT NULL;

CREATE INDEX pod0_model_chapter_due_v1
    ON pod0_model_chapter_workflows(state,not_before_ms,episode_id);

CREATE INDEX pod0_model_chapter_updated_v1
    ON pod0_model_chapter_workflows(updated_at_ms DESC,episode_id);

CREATE UNIQUE INDEX pod0_model_chapter_workflow_request_fence_v1
    ON pod0_model_chapter_workflows(episode_id,request_id,submission_fence_id);

CREATE TABLE pod0_model_chapter_completions(
    request_id BLOB PRIMARY KEY NOT NULL CHECK(length(request_id)=16),
    episode_id BLOB NOT NULL
        REFERENCES pod0_model_chapter_workflows(episode_id) ON DELETE CASCADE,
    generation INTEGER NOT NULL CHECK(generation >= 1),
    submission_fence_id BLOB NOT NULL CHECK(length(submission_fence_id)=16),
    completion TEXT NOT NULL CHECK(length(CAST(completion AS BLOB)) <= 1048576),
    completion_digest BLOB NOT NULL CHECK(length(completion_digest)=32),
    provider TEXT NOT NULL CHECK(length(CAST(provider AS BLOB)) <= 256),
    model TEXT NOT NULL CHECK(length(CAST(model AS BLOB)) <= 256),
    prompt_tokens INTEGER CHECK(prompt_tokens IS NULL OR prompt_tokens >= 0),
    completion_tokens INTEGER CHECK(completion_tokens IS NULL OR completion_tokens >= 0),
    cached_tokens INTEGER CHECK(cached_tokens IS NULL OR cached_tokens >= 0),
    reasoning_tokens INTEGER CHECK(reasoning_tokens IS NULL OR reasoning_tokens >= 0),
    cost_microusd INTEGER CHECK(cost_microusd IS NULL OR cost_microusd >= 0),
    provider_operation_id TEXT,
    provider_status TEXT,
    generated_at_ms INTEGER NOT NULL CHECK(generated_at_ms >= 0),
    observed_at_ms INTEGER NOT NULL CHECK(observed_at_ms >= 0),
    FOREIGN KEY(episode_id,request_id,submission_fence_id)
        REFERENCES pod0_model_chapter_workflows(
            episode_id,request_id,submission_fence_id
        ) ON DELETE CASCADE
) STRICT;

CREATE INDEX pod0_model_chapter_completion_episode_v1
    ON pod0_model_chapter_completions(episode_id,observed_at_ms);
