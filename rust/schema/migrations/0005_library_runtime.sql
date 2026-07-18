CREATE TABLE pod0_episode_feed_metadata(
    episode_id BLOB PRIMARY KEY NOT NULL REFERENCES pod0_episodes(episode_id) ON DELETE CASCADE,
    publisher_transcript_url TEXT,
    publisher_transcript_media_type TEXT,
    publisher_transcript_format_code INTEGER,
    publisher_transcript_format_wire_code INTEGER,
    chapters_url TEXT,
    persons_json TEXT NOT NULL,
    sound_bites_json TEXT NOT NULL,
    CHECK((publisher_transcript_url IS NULL) = (publisher_transcript_format_code IS NULL)),
    CHECK((publisher_transcript_format_code = 255) =
          (publisher_transcript_format_wire_code IS NOT NULL))
) STRICT;

CREATE TABLE pod0_library_commands(
    command_id BLOB PRIMARY KEY NOT NULL CHECK(length(command_id) = 16),
    command_fingerprint TEXT NOT NULL CHECK(length(command_fingerprint) = 64),
    applied_revision INTEGER NOT NULL CHECK(applied_revision >= 1),
    completed_at_ms INTEGER NOT NULL
) STRICT;
