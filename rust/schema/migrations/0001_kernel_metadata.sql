CREATE TABLE pod0_schema_versions(
    component TEXT PRIMARY KEY NOT NULL,
    version INTEGER NOT NULL CHECK(version >= 0),
    updated_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE pod0_store_metadata(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton = 1),
    store_id BLOB NOT NULL UNIQUE CHECK(length(store_id) = 16)
) STRICT;
