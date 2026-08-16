CREATE TABLE pod0_recall_queries (
    query_id BLOB PRIMARY KEY NOT NULL CHECK(length(query_id)=16),
    command_id BLOB NOT NULL UNIQUE CHECK(length(command_id)=16),
    cancellation_id BLOB NOT NULL CHECK(length(cancellation_id)=16),
    revision INTEGER NOT NULL CHECK(revision>=1),
    query_json TEXT NOT NULL CHECK(json_valid(query_json)),
    stage_json TEXT NOT NULL CHECK(json_valid(stage_json)),
    evidence_json TEXT NOT NULL CHECK(json_valid(evidence_json)),
    failure_json TEXT CHECK(failure_json IS NULL OR json_valid(failure_json)),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX pod0_recall_queries_command_idx ON pod0_recall_queries(command_id);

CREATE TABLE pod0_recall_index_cutover_workflow (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    command_id BLOB NOT NULL UNIQUE CHECK(length(command_id)=16),
    cancellation_id BLOB NOT NULL CHECK(length(cancellation_id)=16),
    revision INTEGER NOT NULL CHECK(revision>=1),
    stage TEXT NOT NULL CHECK(stage IN ('awaiting_host','host_observed','committed','failed','cancelled')),
    removed_file_count INTEGER,
    updated_at_ms INTEGER NOT NULL
) STRICT;
