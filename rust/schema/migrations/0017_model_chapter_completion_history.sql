ALTER TABLE pod0_model_chapter_completions
RENAME TO pod0_model_chapter_completions_v16;

CREATE TABLE pod0_model_chapter_completions(
    request_id BLOB PRIMARY KEY NOT NULL CHECK(length(request_id)=16),
    episode_id BLOB NOT NULL
        REFERENCES pod0_episodes(episode_id) ON DELETE CASCADE,
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
    UNIQUE(episode_id,generation,submission_fence_id)
) STRICT;

INSERT INTO pod0_model_chapter_completions(
    request_id,episode_id,generation,submission_fence_id,completion,
    completion_digest,provider,model,prompt_tokens,completion_tokens,
    cached_tokens,reasoning_tokens,cost_microusd,provider_operation_id,
    provider_status,generated_at_ms,observed_at_ms
)
SELECT request_id,episode_id,generation,submission_fence_id,completion,
       completion_digest,provider,model,prompt_tokens,completion_tokens,
       cached_tokens,reasoning_tokens,cost_microusd,provider_operation_id,
       provider_status,generated_at_ms,observed_at_ms
FROM pod0_model_chapter_completions_v16;

DROP TABLE pod0_model_chapter_completions_v16;

CREATE INDEX pod0_model_chapter_completion_episode_v1
    ON pod0_model_chapter_completions(episode_id,observed_at_ms);
