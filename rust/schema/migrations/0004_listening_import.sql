CREATE TABLE pod0_listening_imports(
    import_id BLOB PRIMARY KEY NOT NULL CHECK(length(import_id) = 16),
    source_kind INTEGER NOT NULL CHECK(source_kind IN (1, 2)),
    source_hash TEXT NOT NULL CHECK(length(source_hash) = 64),
    source_generation INTEGER NOT NULL CHECK(source_generation >= 0),
    podcast_count INTEGER NOT NULL CHECK(podcast_count >= 0),
    subscription_count INTEGER NOT NULL CHECK(subscription_count >= 0),
    episode_count INTEGER NOT NULL CHECK(episode_count >= 0),
    backup_byte_count INTEGER NOT NULL CHECK(backup_byte_count > 0),
    target_revision INTEGER NOT NULL CHECK(target_revision >= 1),
    state TEXT NOT NULL CHECK(state = 'verified'),
    verified_at_ms INTEGER NOT NULL,
    UNIQUE(source_kind, source_hash, source_generation)
) STRICT;

CREATE TABLE pod0_podcasts(
    podcast_id BLOB PRIMARY KEY NOT NULL CHECK(length(podcast_id) = 16),
    kind_code INTEGER NOT NULL CHECK(kind_code IN (1, 2, 255)),
    kind_wire_code INTEGER,
    feed_url TEXT,
    feed_key_v1 TEXT,
    title TEXT NOT NULL,
    author TEXT NOT NULL,
    image_url TEXT,
    description TEXT NOT NULL,
    language TEXT,
    categories_json TEXT NOT NULL,
    discovered_at_ms INTEGER NOT NULL,
    title_is_placeholder INTEGER NOT NULL CHECK(title_is_placeholder IN (0, 1)),
    last_refreshed_at_ms INTEGER,
    etag TEXT,
    last_modified TEXT,
    source_import_id BLOB NOT NULL REFERENCES pod0_listening_imports(import_id),
    CHECK((kind_code = 255) = (kind_wire_code IS NOT NULL)),
    CHECK((feed_url IS NULL) = (feed_key_v1 IS NULL))
) STRICT;

CREATE UNIQUE INDEX pod0_podcasts_feed_v1_idx
ON pod0_podcasts(feed_key_v1) WHERE feed_key_v1 IS NOT NULL;

CREATE TABLE pod0_subscriptions(
    podcast_id BLOB PRIMARY KEY NOT NULL REFERENCES pod0_podcasts(podcast_id),
    subscribed_at_ms INTEGER NOT NULL,
    auto_download_code INTEGER NOT NULL CHECK(auto_download_code IN (1, 2, 3, 255)),
    auto_download_wire_code INTEGER,
    auto_download_latest_count INTEGER,
    wifi_only INTEGER NOT NULL CHECK(wifi_only IN (0, 1)),
    notifications_enabled INTEGER NOT NULL CHECK(notifications_enabled IN (0, 1)),
    default_playback_rate_permille INTEGER,
    source_import_id BLOB NOT NULL REFERENCES pod0_listening_imports(import_id),
    CHECK((auto_download_code = 2) = (auto_download_latest_count IS NOT NULL)),
    CHECK((auto_download_code = 255) = (auto_download_wire_code IS NOT NULL))
) STRICT;

CREATE TABLE pod0_episodes(
    episode_id BLOB PRIMARY KEY NOT NULL CHECK(length(episode_id) = 16),
    podcast_id BLOB NOT NULL REFERENCES pod0_podcasts(podcast_id),
    publisher_guid TEXT NOT NULL CHECK(length(publisher_guid) > 0),
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    published_at_ms INTEGER NOT NULL,
    duration_ms INTEGER CHECK(duration_ms >= 0),
    enclosure_url TEXT NOT NULL,
    enclosure_mime_type TEXT,
    image_url TEXT,
    resume_position_ms INTEGER NOT NULL CHECK(resume_position_ms >= 0),
    completion_code INTEGER NOT NULL CHECK(completion_code IN (1, 2, 255)),
    completion_cause_code INTEGER,
    completion_cause_wire_code INTEGER,
    is_starred INTEGER NOT NULL CHECK(is_starred IN (0, 1)),
    download_code INTEGER NOT NULL CHECK(download_code IN (1, 2, 255)),
    download_wire_code INTEGER,
    download_ref_version INTEGER,
    download_ref_key TEXT,
    download_byte_count INTEGER,
    transcript_code INTEGER NOT NULL CHECK(transcript_code IN (1, 2, 255)),
    transcript_wire_code INTEGER,
    transcript_ref_version INTEGER,
    transcript_ref_key TEXT,
    transcript_source_code INTEGER,
    transcript_source_wire_code INTEGER,
    legacy_payload BLOB NOT NULL,
    source_import_id BLOB NOT NULL REFERENCES pod0_listening_imports(import_id),
    UNIQUE(podcast_id, publisher_guid),
    CHECK((download_code = 2) = (download_ref_key IS NOT NULL)),
    CHECK((transcript_code = 2) = (transcript_ref_key IS NOT NULL))
) STRICT;

CREATE INDEX pod0_episodes_podcast_published_idx
ON pod0_episodes(podcast_id, published_at_ms DESC);

CREATE TABLE pod0_playback_state(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton = 1),
    active_episode_id BLOB REFERENCES pod0_episodes(episode_id),
    playback_rate_permille INTEGER NOT NULL CHECK(playback_rate_permille BETWEEN 500 AND 3000),
    sleep_mode_code INTEGER NOT NULL CHECK(sleep_mode_code IN (1, 2, 3, 255)),
    sleep_duration_ms INTEGER,
    sleep_wire_code INTEGER,
    auto_mark_played_at_natural_end INTEGER NOT NULL CHECK(auto_mark_played_at_natural_end IN (0, 1)),
    auto_play_next INTEGER NOT NULL CHECK(auto_play_next IN (0, 1)),
    state_revision INTEGER NOT NULL CHECK(state_revision >= 1),
    source_import_id BLOB NOT NULL REFERENCES pod0_listening_imports(import_id),
    CHECK((sleep_mode_code = 2) = (sleep_duration_ms IS NOT NULL)),
    CHECK((sleep_mode_code = 255) = (sleep_wire_code IS NOT NULL))
) STRICT;

CREATE TABLE pod0_queue_entries(
    queue_entry_id BLOB PRIMARY KEY NOT NULL CHECK(length(queue_entry_id) = 16),
    sort_order INTEGER NOT NULL UNIQUE CHECK(sort_order >= 0),
    episode_id BLOB NOT NULL REFERENCES pod0_episodes(episode_id),
    segment_start_ms INTEGER,
    segment_end_ms INTEGER,
    label TEXT,
    source_import_id BLOB NOT NULL REFERENCES pod0_listening_imports(import_id),
    CHECK(segment_end_ms IS NULL OR segment_end_ms > COALESCE(segment_start_ms, 0))
) STRICT;
