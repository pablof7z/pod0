CREATE TABLE pod0_memory_state(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    collection_revision INTEGER NOT NULL CHECK(collection_revision>=0),
    authority_active INTEGER NOT NULL CHECK(authority_active IN (0,1)),
    source_generation INTEGER CHECK(source_generation IS NULL OR source_generation>0)
) STRICT;

INSERT INTO pod0_memory_state(
    singleton,collection_revision,authority_active,source_generation
) VALUES(1,0,0,NULL);

CREATE TABLE pod0_memories(
    memory_id BLOB PRIMARY KEY NOT NULL CHECK(length(memory_id)=16),
    memory_revision INTEGER NOT NULL CHECK(memory_revision>=1),
    content TEXT NOT NULL
        CHECK(length(CAST(content AS BLOB)) BETWEEN 1 AND 65536),
    source_code INTEGER NOT NULL CHECK(source_code IN (1,2)),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms>=0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms),
    deleted INTEGER NOT NULL CHECK(deleted IN (0,1)),
    created_command_id BLOB
        CHECK(created_command_id IS NULL OR length(created_command_id)=16)
) STRICT;

CREATE INDEX pod0_memories_active_created_v1
    ON pod0_memories(deleted,created_at_ms DESC,memory_id);

CREATE TABLE pod0_compiled_memory(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    text TEXT NOT NULL CHECK(length(CAST(text AS BLOB))<=65536),
    compiled_at_ms INTEGER NOT NULL CHECK(compiled_at_ms>=0)
) STRICT;

CREATE TABLE pod0_compiled_memory_sources(
    singleton INTEGER NOT NULL
        REFERENCES pod0_compiled_memory(singleton) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL CHECK(sort_order>=0),
    memory_id BLOB NOT NULL
        REFERENCES pod0_memories(memory_id) ON DELETE CASCADE
        CHECK(length(memory_id)=16),
    PRIMARY KEY(singleton,sort_order),
    UNIQUE(singleton,memory_id)
) STRICT;

CREATE TABLE pod0_memory_cutover_evidence(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    state TEXT NOT NULL CHECK(state IN ('staged','verified','authoritative')),
    source_generation INTEGER UNIQUE NOT NULL CHECK(source_generation>0),
    source_fingerprint BLOB NOT NULL CHECK(length(source_fingerprint)=32),
    backup_digest BLOB NOT NULL CHECK(length(backup_digest)=32),
    backup_byte_count INTEGER NOT NULL CHECK(backup_byte_count>0),
    memory_count INTEGER NOT NULL CHECK(memory_count>=0),
    deleted_count INTEGER NOT NULL CHECK(deleted_count>=0),
    compiled_present INTEGER NOT NULL CHECK(compiled_present IN (0,1)),
    staged_at_ms INTEGER NOT NULL CHECK(staged_at_ms>=0),
    verified_at_ms INTEGER CHECK(verified_at_ms>=staged_at_ms),
    committed_at_ms INTEGER CHECK(committed_at_ms>=staged_at_ms)
) STRICT;
