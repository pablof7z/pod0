CREATE TABLE pod0_feed_discovery_occurrences(
    occurrence_id BLOB PRIMARY KEY NOT NULL CHECK(length(occurrence_id)=16),
    command_id BLOB NOT NULL UNIQUE CHECK(length(command_id)=16)
        REFERENCES pod0_library_commands(command_id) ON DELETE CASCADE,
    podcast_id BLOB NOT NULL CHECK(length(podcast_id)=16)
        REFERENCES pod0_podcasts(podcast_id) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK(state='pending'),
    workflow_schema_version INTEGER NOT NULL CHECK(workflow_schema_version=1),
    policy_version INTEGER NOT NULL CHECK(policy_version=1),
    is_initial_population INTEGER NOT NULL CHECK(is_initial_population IN (0,1)),
    item_count INTEGER NOT NULL CHECK(item_count>=1 AND item_count<=10000),
    observed_at_ms INTEGER NOT NULL CHECK(observed_at_ms>=0),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms>=0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms)
) STRICT;

CREATE INDEX pod0_feed_discovery_pending_v1
    ON pod0_feed_discovery_occurrences(state,observed_at_ms,occurrence_id);

CREATE TABLE pod0_feed_discovery_items(
    item_id BLOB PRIMARY KEY NOT NULL CHECK(length(item_id)=16),
    occurrence_id BLOB NOT NULL CHECK(length(occurrence_id)=16)
        REFERENCES pod0_feed_discovery_occurrences(occurrence_id) ON DELETE CASCADE,
    episode_id BLOB NOT NULL CHECK(length(episode_id)=16)
        REFERENCES pod0_episodes(episode_id) ON DELETE CASCADE,
    input_version TEXT NOT NULL CHECK(
        length(input_version)=64 AND input_version NOT GLOB '*[^0-9a-f]*'
    ),
    published_at_ms INTEGER NOT NULL,
    UNIQUE(occurrence_id,episode_id)
) STRICT;

CREATE INDEX pod0_feed_discovery_items_order_v1
    ON pod0_feed_discovery_items(occurrence_id,published_at_ms DESC,episode_id);

CREATE TABLE pod0_feed_apply_receipts(
    command_id BLOB PRIMARY KEY NOT NULL CHECK(length(command_id)=16)
        REFERENCES pod0_library_commands(command_id) ON DELETE CASCADE,
    podcast_id BLOB NOT NULL CHECK(length(podcast_id)=16)
        REFERENCES pod0_podcasts(podcast_id) ON DELETE CASCADE,
    inserted_episode_count INTEGER NOT NULL
        CHECK(inserted_episode_count>=0 AND inserted_episode_count<=10000),
    discovery_occurrence_id BLOB UNIQUE CHECK(
        discovery_occurrence_id IS NULL OR length(discovery_occurrence_id)=16
    ) REFERENCES pod0_feed_discovery_occurrences(occurrence_id) ON DELETE SET NULL
) STRICT;
