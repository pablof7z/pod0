CREATE TABLE pod0_agent_turns(
    turn_id BLOB PRIMARY KEY NOT NULL CHECK(length(turn_id)=16),
    conversation_id BLOB NOT NULL CHECK(length(conversation_id)=16),
    state_revision INTEGER NOT NULL CHECK(state_revision>=1),
    stage TEXT NOT NULL CHECK(stage IN(
        'awaiting_model','approval_required','authorized','executing','commit_pending',
        'committed','completed','denied','cancelled','blocked','outcome_ambiguous','failed'
    )),
    state_schema_version INTEGER NOT NULL CHECK(state_schema_version=1),
    state_json BLOB NOT NULL CHECK(length(state_json) BETWEEN 2 AND 1048576),
    state_digest BLOB NOT NULL CHECK(length(state_digest)=32),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms)
) STRICT;

CREATE INDEX pod0_agent_turns_conversation_v1
    ON pod0_agent_turns(conversation_id,updated_at_ms DESC,turn_id);

CREATE TABLE pod0_agent_audit(
    turn_id BLOB NOT NULL REFERENCES pod0_agent_turns(turn_id),
    turn_revision INTEGER NOT NULL CHECK(turn_revision>=1),
    event_kind TEXT NOT NULL CHECK(event_kind IN(
        'started','model_observed','authorization_observed','execution_started',
        'action_observed','cancelled','recovered'
    )),
    state_digest BLOB NOT NULL CHECK(length(state_digest)=32),
    observed_at_ms INTEGER NOT NULL,
    PRIMARY KEY(turn_id,turn_revision)
) STRICT;

CREATE TABLE pod0_agent_command_receipts(
    command_id BLOB PRIMARY KEY NOT NULL CHECK(length(command_id)=16),
    command_fingerprint BLOB NOT NULL CHECK(length(command_fingerprint)=32),
    turn_id BLOB NOT NULL REFERENCES pod0_agent_turns(turn_id),
    applied_revision INTEGER NOT NULL CHECK(applied_revision>=1),
    completed_at_ms INTEGER NOT NULL
) STRICT;
