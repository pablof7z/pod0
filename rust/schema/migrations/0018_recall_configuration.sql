CREATE TABLE pod0_recall_configuration(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    schema_version INTEGER NOT NULL CHECK(schema_version=1),
    revision INTEGER NOT NULL CHECK(revision >= 1),
    origin TEXT NOT NULL CHECK(origin IN('legacy_swift','user')),
    embedding_provider TEXT NOT NULL CHECK(embedding_provider IN('openrouter','ollama')),
    embedding_model TEXT NOT NULL
        CHECK(length(CAST(embedding_model AS BLOB)) BETWEEN 1 AND 256),
    stored_embedding_model_id TEXT NOT NULL
        CHECK(length(CAST(stored_embedding_model_id AS BLOB)) BETWEEN 1 AND 256),
    embedding_dimensions INTEGER NOT NULL CHECK(embedding_dimensions=1024),
    embedding_space_digest BLOB NOT NULL CHECK(length(embedding_space_digest)=32),
    reranker_enabled INTEGER NOT NULL CHECK(reranker_enabled IN(0,1)),
    reranker_provider TEXT CHECK(reranker_provider IS NULL OR reranker_provider='openrouter'),
    reranker_model TEXT
        CHECK(reranker_model IS NULL OR length(CAST(reranker_model AS BLOB)) BETWEEN 1 AND 256),
    source_generation BLOB CHECK(source_generation IS NULL OR length(source_generation)=32),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0),
    CHECK(
        (reranker_enabled=1 AND reranker_provider IS NOT NULL AND reranker_model IS NOT NULL)
        OR (reranker_enabled=0 AND reranker_provider IS NULL AND reranker_model IS NULL)
    ),
    CHECK((origin='legacy_swift') = (source_generation IS NOT NULL))
) STRICT;
