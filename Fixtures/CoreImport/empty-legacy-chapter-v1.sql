DROP TABLE IF EXISTS episodes;
DROP TABLE IF EXISTS persistence_metadata;
DROP TABLE IF EXISTS artifacts;
DROP TABLE IF EXISTS workflow_schema_versions;

CREATE TABLE episodes(
    id TEXT PRIMARY KEY,
    subscription_id TEXT NOT NULL,
    payload BLOB NOT NULL
);

CREATE TABLE persistence_metadata(
    key TEXT PRIMARY KEY,
    value BLOB NOT NULL
);

INSERT INTO persistence_metadata(key, value) VALUES('generation', '0');
