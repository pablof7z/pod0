-- Exact durable effect requests may carry bounded transcript/evidence input.
-- Rebuild both sides of the FK so every intent, attempt, fence, and observation survives.
CREATE TABLE pod0_effect_intents_v39(
    intent_id BLOB PRIMARY KEY NOT NULL CHECK(length(intent_id)=16),
    authorizing_activity_id BLOB NOT NULL CHECK(length(authorizing_activity_id)=16),
    authorizing_fact_code INTEGER NOT NULL DEFAULT 4 CHECK(authorizing_fact_code=4),
    correlation_id BLOB NOT NULL CHECK(length(correlation_id)=16),
    effect_kind_code INTEGER NOT NULL CHECK(effect_kind_code BETWEEN 1 AND 14),
    subject_code INTEGER NOT NULL CHECK(subject_code BETWEEN 0 AND 255),
    subject_id BLOB CHECK(subject_id IS NULL OR length(subject_id)=16),
    episode_id BLOB CHECK(episode_id IS NULL OR length(episode_id)=16),
    request_schema_version INTEGER NOT NULL DEFAULT 1 CHECK(request_schema_version=1),
    request_json TEXT NOT NULL CHECK(length(request_json)<=67108864),
    state_code INTEGER NOT NULL DEFAULT 1 CHECK(state_code BETWEEN 1 AND 6),
    fence INTEGER NOT NULL DEFAULT 0 CHECK(fence>=0),
    available_at_ms INTEGER NOT NULL CHECK(available_at_ms>=0),
    deadline_at_ms INTEGER CHECK(deadline_at_ms IS NULL OR deadline_at_ms>=available_at_ms),
    committed_at_ms INTEGER NOT NULL CHECK(committed_at_ms>=0),
    CHECK((subject_code=0)=(subject_id IS NULL)),
    FOREIGN KEY(authorizing_activity_id,intent_id,authorizing_fact_code)
        REFERENCES pod0_activity_facts(activity_id,authorized_effect_intent_id,fact_code)
) STRICT;

CREATE TABLE pod0_effect_attempts_v39(
    attempt_id BLOB PRIMARY KEY NOT NULL CHECK(length(attempt_id)=16),
    intent_id BLOB NOT NULL REFERENCES pod0_effect_intents_v39(intent_id),
    lease_id BLOB NOT NULL UNIQUE CHECK(length(lease_id)=16),
    fence INTEGER NOT NULL CHECK(fence>=1),
    state_code INTEGER NOT NULL CHECK(state_code BETWEEN 1 AND 4),
    claimed_at_ms INTEGER NOT NULL CHECK(claimed_at_ms>=0),
    lease_expires_at_ms INTEGER NOT NULL CHECK(lease_expires_at_ms>claimed_at_ms),
    observed_at_ms INTEGER,
    outcome_schema_version INTEGER CHECK(outcome_schema_version IS NULL OR outcome_schema_version=1),
    outcome_json TEXT CHECK(outcome_json IS NULL OR length(outcome_json)<=4096),
    observation_schema_version INTEGER
        CHECK(observation_schema_version IS NULL OR observation_schema_version=1),
    observation_json TEXT CHECK(observation_json IS NULL OR length(observation_json)<=67108864),
    UNIQUE(intent_id,fence),
    CHECK((observed_at_ms IS NULL)=(outcome_json IS NULL)),
    CHECK((observation_schema_version IS NULL)=(observation_json IS NULL))
) STRICT;

INSERT INTO pod0_effect_intents_v39 SELECT * FROM pod0_effect_intents;
INSERT INTO pod0_effect_attempts_v39 SELECT * FROM pod0_effect_attempts;
DROP TABLE pod0_effect_attempts;
DROP TABLE pod0_effect_intents;
ALTER TABLE pod0_effect_intents_v39 RENAME TO pod0_effect_intents;
ALTER TABLE pod0_effect_attempts_v39 RENAME TO pod0_effect_attempts;

CREATE INDEX pod0_effect_intents_claim_v1
    ON pod0_effect_intents(state_code,available_at_ms,intent_id);
CREATE INDEX pod0_effect_intents_episode_v1
    ON pod0_effect_intents(episode_id,state_code);
CREATE INDEX pod0_effect_attempts_lease_v1
    ON pod0_effect_attempts(intent_id,state_code,lease_expires_at_ms);
