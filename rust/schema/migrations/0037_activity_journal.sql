-- Immutable product activity journal (epic #204, issue #207).
-- Current-state tables remain authoritative; this table is causal history.
CREATE TABLE pod0_activity_facts(
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    activity_id BLOB NOT NULL UNIQUE CHECK(length(activity_id)=16),
    transaction_id BLOB NOT NULL CHECK(length(transaction_id)=16),
    correlation_id BLOB NOT NULL CHECK(length(correlation_id)=16),
    caused_by_activity_id BLOB
        CHECK(caused_by_activity_id IS NULL OR length(caused_by_activity_id)=16),
    command_id BLOB CHECK(command_id IS NULL OR length(command_id)=16),
    host_request_id BLOB CHECK(host_request_id IS NULL OR length(host_request_id)=16),
    authorized_effect_intent_id BLOB
        CHECK(authorized_effect_intent_id IS NULL OR length(authorized_effect_intent_id)=16),
    authorized_internal_command_id BLOB
        CHECK(authorized_internal_command_id IS NULL OR length(authorized_internal_command_id)=16),
    actor_code INTEGER NOT NULL CHECK(actor_code BETWEEN 0 AND 255),
    origin_code INTEGER NOT NULL CHECK(origin_code BETWEEN 0 AND 255),
    subject_code INTEGER NOT NULL CHECK(subject_code BETWEEN 0 AND 255),
    subject_id BLOB CHECK(subject_id IS NULL OR length(subject_id)=16),
    episode_id BLOB CHECK(episode_id IS NULL OR length(episode_id)=16),
    fact_code INTEGER NOT NULL CHECK(fact_code BETWEEN 1 AND 255),
    payload_schema_version INTEGER NOT NULL DEFAULT 1
        CHECK(payload_schema_version=1),
    payload_json TEXT NOT NULL CHECK(length(payload_json)<=16384),
    committed_at_ms INTEGER NOT NULL CHECK(committed_at_ms>=0),
    CHECK((subject_code=0)=(subject_id IS NULL)),
    CHECK(caused_by_activity_id IS NULL OR caused_by_activity_id<>activity_id),
    CHECK((fact_code=4)=(authorized_effect_intent_id IS NOT NULL)),
    CHECK((fact_code=6)=(authorized_internal_command_id IS NOT NULL)),
    UNIQUE(activity_id,correlation_id),
    UNIQUE(activity_id,fact_code),
    UNIQUE(activity_id,authorized_effect_intent_id,fact_code),
    UNIQUE(activity_id,authorized_internal_command_id,fact_code),
    FOREIGN KEY(caused_by_activity_id,correlation_id)
        REFERENCES pod0_activity_facts(activity_id,correlation_id)
) STRICT;

CREATE INDEX pod0_activity_facts_episode_sequence_v1
    ON pod0_activity_facts(episode_id,sequence);
CREATE INDEX pod0_activity_facts_correlation_sequence_v1
    ON pod0_activity_facts(correlation_id,sequence);
CREATE INDEX pod0_activity_facts_transaction_sequence_v1
    ON pod0_activity_facts(transaction_id,sequence);
CREATE INDEX pod0_activity_facts_command_sequence_v1
    ON pod0_activity_facts(command_id,sequence);
CREATE INDEX pod0_activity_facts_cause_v1
    ON pod0_activity_facts(caused_by_activity_id);

CREATE TABLE pod0_transition_receipts(
    ingress_code INTEGER NOT NULL CHECK(ingress_code BETWEEN 1 AND 6),
    ingress_id BLOB NOT NULL CHECK(length(ingress_id)=16),
    fingerprint BLOB NOT NULL CHECK(length(fingerprint)=32),
    transaction_id BLOB NOT NULL UNIQUE CHECK(length(transaction_id)=16),
    disposition_code INTEGER NOT NULL CHECK(disposition_code BETWEEN 1 AND 7),
    first_sequence INTEGER NOT NULL REFERENCES pod0_activity_facts(sequence),
    last_sequence INTEGER NOT NULL REFERENCES pod0_activity_facts(sequence),
    committed_revision INTEGER NOT NULL CHECK(committed_revision>=0),
    result_schema_version INTEGER NOT NULL DEFAULT 1 CHECK(result_schema_version=1),
    result_json TEXT NOT NULL CHECK(length(result_json)<=4096),
    committed_at_ms INTEGER NOT NULL CHECK(committed_at_ms>=0),
    PRIMARY KEY(ingress_code,ingress_id),
    CHECK(first_sequence<=last_sequence)
) STRICT;

CREATE TABLE pod0_effect_intents(
    intent_id BLOB PRIMARY KEY NOT NULL CHECK(length(intent_id)=16),
    authorizing_activity_id BLOB NOT NULL CHECK(length(authorizing_activity_id)=16),
    authorizing_fact_code INTEGER NOT NULL DEFAULT 4 CHECK(authorizing_fact_code=4),
    correlation_id BLOB NOT NULL CHECK(length(correlation_id)=16),
    effect_kind_code INTEGER NOT NULL CHECK(effect_kind_code BETWEEN 1 AND 14),
    subject_code INTEGER NOT NULL CHECK(subject_code BETWEEN 0 AND 255),
    subject_id BLOB CHECK(subject_id IS NULL OR length(subject_id)=16),
    episode_id BLOB CHECK(episode_id IS NULL OR length(episode_id)=16),
    request_schema_version INTEGER NOT NULL DEFAULT 1 CHECK(request_schema_version=1),
    request_json TEXT NOT NULL CHECK(length(request_json)<=4096),
    state_code INTEGER NOT NULL DEFAULT 1 CHECK(state_code BETWEEN 1 AND 6),
    fence INTEGER NOT NULL DEFAULT 0 CHECK(fence>=0),
    available_at_ms INTEGER NOT NULL CHECK(available_at_ms>=0),
    deadline_at_ms INTEGER CHECK(deadline_at_ms IS NULL OR deadline_at_ms>=available_at_ms),
    committed_at_ms INTEGER NOT NULL CHECK(committed_at_ms>=0),
    CHECK((subject_code=0)=(subject_id IS NULL)),
    FOREIGN KEY(authorizing_activity_id,intent_id,authorizing_fact_code)
        REFERENCES pod0_activity_facts(activity_id,authorized_effect_intent_id,fact_code)
) STRICT;

CREATE INDEX pod0_effect_intents_claim_v1
    ON pod0_effect_intents(state_code,available_at_ms,intent_id);
CREATE INDEX pod0_effect_intents_episode_v1
    ON pod0_effect_intents(episode_id,state_code);

CREATE TABLE pod0_effect_attempts(
    attempt_id BLOB PRIMARY KEY NOT NULL CHECK(length(attempt_id)=16),
    intent_id BLOB NOT NULL REFERENCES pod0_effect_intents(intent_id),
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

CREATE INDEX pod0_effect_attempts_lease_v1
    ON pod0_effect_attempts(intent_id,state_code,lease_expires_at_ms);

CREATE TABLE pod0_internal_command_intents(
    internal_command_id BLOB PRIMARY KEY NOT NULL CHECK(length(internal_command_id)=16),
    authorizing_activity_id BLOB NOT NULL CHECK(length(authorizing_activity_id)=16),
    authorizing_fact_code INTEGER NOT NULL DEFAULT 6 CHECK(authorizing_fact_code=6),
    correlation_id BLOB NOT NULL CHECK(length(correlation_id)=16),
    target_domain_code INTEGER NOT NULL CHECK(target_domain_code BETWEEN 1 AND 10),
    subject_code INTEGER NOT NULL CHECK(subject_code BETWEEN 0 AND 255),
    subject_id BLOB CHECK(subject_id IS NULL OR length(subject_id)=16),
    episode_id BLOB CHECK(episode_id IS NULL OR length(episode_id)=16),
    command_schema_version INTEGER NOT NULL DEFAULT 1 CHECK(command_schema_version=1),
    command_json TEXT NOT NULL CHECK(length(command_json)<=4096),
    state_code INTEGER NOT NULL DEFAULT 1 CHECK(state_code BETWEEN 1 AND 4),
    committed_at_ms INTEGER NOT NULL CHECK(committed_at_ms>=0),
    CHECK((subject_code=0)=(subject_id IS NULL)),
    FOREIGN KEY(authorizing_activity_id,internal_command_id,authorizing_fact_code)
        REFERENCES pod0_activity_facts(activity_id,authorized_internal_command_id,fact_code)
) STRICT;

CREATE INDEX pod0_internal_commands_pending_v1
    ON pod0_internal_command_intents(target_domain_code,state_code,internal_command_id);

CREATE TRIGGER pod0_activity_facts_no_update
BEFORE UPDATE ON pod0_activity_facts
BEGIN
    SELECT RAISE(ABORT,'pod0 activity facts are append-only');
END;

CREATE TRIGGER pod0_activity_facts_no_delete
BEFORE DELETE ON pod0_activity_facts
BEGIN
    SELECT RAISE(ABORT,'pod0 activity facts are append-only');
END;
