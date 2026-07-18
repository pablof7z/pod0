CREATE TABLE pod0_domain_cutovers(
    domain TEXT PRIMARY KEY NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('staged', 'authoritative')),
    source_generation INTEGER NOT NULL,
    core_revision INTEGER NOT NULL,
    committed_at_ms INTEGER NOT NULL
) STRICT;
