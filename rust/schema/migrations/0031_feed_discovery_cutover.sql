CREATE TABLE pod0_feed_discovery_cutover(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    state TEXT NOT NULL CHECK(state IN ('staged','authoritative')),
    source_generation INTEGER NOT NULL CHECK(source_generation>0),
    source_fingerprint BLOB NOT NULL CHECK(length(source_fingerprint)=32),
    backup_digest BLOB NOT NULL CHECK(length(backup_digest)=32),
    backup_byte_count INTEGER NOT NULL CHECK(backup_byte_count>=0),
    notification_command_id BLOB NOT NULL CHECK(length(notification_command_id)=16),
    notifications_enabled INTEGER NOT NULL CHECK(notifications_enabled IN (0,1)),
    inspected_job_count INTEGER NOT NULL CHECK(inspected_job_count>=0),
    candidate_count INTEGER NOT NULL CHECK(candidate_count>=0),
    blocked_count INTEGER NOT NULL CHECK(blocked_count>=0),
    ambiguous_count INTEGER NOT NULL CHECK(ambiguous_count>=0),
    staged_at_ms INTEGER NOT NULL CHECK(staged_at_ms>=0),
    committed_at_ms INTEGER CHECK(
        committed_at_ms IS NULL OR committed_at_ms>=staged_at_ms
    )
) STRICT;

CREATE TABLE pod0_feed_discovery_cutover_candidates(
    occurrence_id BLOB NOT NULL CHECK(length(occurrence_id)=16),
    command_id BLOB NOT NULL CHECK(length(command_id)=16),
    podcast_id BLOB NOT NULL CHECK(length(podcast_id)=16)
        REFERENCES pod0_podcasts(podcast_id) ON DELETE RESTRICT,
    episode_id BLOB NOT NULL CHECK(length(episode_id)=16)
        REFERENCES pod0_episodes(episode_id) ON DELETE RESTRICT,
    kind TEXT NOT NULL CHECK(kind IN ('download','notification')),
    disposition TEXT NOT NULL CHECK(
        disposition IN ('pending','succeeded','obsolete','failed','ambiguous')
    ),
    attempt INTEGER NOT NULL CHECK(attempt>=0 AND attempt<=4),
    not_before_ms INTEGER CHECK(not_before_ms IS NULL OR not_before_ms>=0),
    observed_at_ms INTEGER NOT NULL CHECK(observed_at_ms>=0),
    expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms>=observed_at_ms),
    published_at_ms INTEGER NOT NULL,
    input_version TEXT NOT NULL CHECK(
        length(input_version)=64 AND input_version NOT GLOB '*[^0-9a-f]*'
    ),
    PRIMARY KEY(occurrence_id,episode_id,kind),
    UNIQUE(command_id,episode_id,kind)
) STRICT;

CREATE INDEX pod0_feed_discovery_cutover_candidates_v1
    ON pod0_feed_discovery_cutover_candidates(occurrence_id,published_at_ms DESC,episode_id,kind);
