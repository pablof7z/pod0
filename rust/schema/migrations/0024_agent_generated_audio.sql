CREATE TABLE pod0_agent_generated_audio_artifacts(
    artifact_id BLOB PRIMARY KEY NOT NULL CHECK(length(artifact_id)=16),
    episode_id BLOB UNIQUE NOT NULL
        REFERENCES pod0_episodes(episode_id) ON DELETE CASCADE
        CHECK(length(episode_id)=16),
    podcast_id BLOB NOT NULL
        REFERENCES pod0_podcasts(podcast_id) ON DELETE CASCADE
        CHECK(length(podcast_id)=16),
    conversation_id BLOB NOT NULL CHECK(length(conversation_id)=16),
    turn_id BLOB UNIQUE NOT NULL CHECK(length(turn_id)=16),
    proposal_id BLOB UNIQUE NOT NULL CHECK(length(proposal_id)=16),
    commit_id BLOB UNIQUE NOT NULL CHECK(length(commit_id)=16),
    media_url TEXT NOT NULL
        CHECK(length(CAST(media_url AS BLOB)) BETWEEN 8 AND 8192),
    media_type TEXT NOT NULL CHECK(media_type='audio/mpeg'),
    media_byte_count INTEGER NOT NULL CHECK(media_byte_count BETWEEN 1 AND 134217728),
    media_content_digest BLOB NOT NULL CHECK(length(media_content_digest)=32),
    script_content_digest BLOB NOT NULL CHECK(length(script_content_digest)=32),
    voice_id TEXT CHECK(length(CAST(voice_id AS BLOB)) BETWEEN 1 AND 256),
    model_reference TEXT NOT NULL
        CHECK(length(CAST(model_reference AS BLOB)) BETWEEN 1 AND 256),
    committed_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX pod0_agent_generated_audio_episode_v1
    ON pod0_agent_generated_audio_artifacts(episode_id,artifact_id);
