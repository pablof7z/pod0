CREATE TABLE pod0_agent_history_cutover_evidence (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    state TEXT NOT NULL CHECK (state IN ('staged', 'verified', 'authoritative')),
    source_generation INTEGER NOT NULL CHECK (source_generation > 0),
    source_fingerprint BLOB NOT NULL CHECK (length(source_fingerprint) = 32),
    backup_digest BLOB NOT NULL CHECK (length(backup_digest) = 32),
    backup_byte_count INTEGER NOT NULL CHECK (backup_byte_count > 0),
    conversation_count INTEGER NOT NULL CHECK (conversation_count >= 0),
    turn_count INTEGER NOT NULL CHECK (turn_count >= 0),
    message_count INTEGER NOT NULL CHECK (message_count >= 0),
    staged_at_ms INTEGER NOT NULL CHECK (staged_at_ms >= 0),
    verified_at_ms INTEGER CHECK (verified_at_ms >= staged_at_ms),
    committed_at_ms INTEGER CHECK (committed_at_ms >= staged_at_ms)
);

CREATE TABLE pod0_agent_history_staged_conversations (
    conversation_id BLOB PRIMARY KEY CHECK (length(conversation_id) = 16),
    title TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms)
);

CREATE TABLE pod0_agent_history_staged_turns (
    turn_id BLOB PRIMARY KEY CHECK (length(turn_id) = 16),
    conversation_id BLOB NOT NULL CHECK (length(conversation_id) = 16),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms),
    state_schema_version INTEGER NOT NULL CHECK (state_schema_version = 1),
    state_json BLOB NOT NULL,
    state_digest BLOB NOT NULL CHECK (length(state_digest) = 32),
    FOREIGN KEY (conversation_id)
        REFERENCES pod0_agent_history_staged_conversations(conversation_id)
        ON DELETE CASCADE
);

CREATE INDEX pod0_agent_history_staged_turns_conversation_idx
    ON pod0_agent_history_staged_turns(conversation_id, created_at_ms, turn_id);

CREATE TABLE pod0_agent_conversation_metadata (
    conversation_id BLOB PRIMARY KEY CHECK (length(conversation_id) = 16),
    title TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('legacy_swift')),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms)
);
