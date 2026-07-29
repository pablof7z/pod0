-- Issue #190: durable speaker identity.
--
-- A user-visible speaker name cannot live in pod0_transcript_speakers:
-- display_name is hashed into the artifact integrity digest, so mutating it
-- would invalidate every subsequent read of the sealed artifact. Identity
-- therefore lives in a mutable, artifact-external entity plus a revisable
-- assignment link, mirroring the pod0_categories prior art.
CREATE TABLE pod0_speakers(
    speaker_entity_id BLOB PRIMARY KEY NOT NULL CHECK(length(speaker_entity_id)=16),
    speaker_entity_revision INTEGER NOT NULL CHECK(speaker_entity_revision>=1),
    display_name TEXT NOT NULL
        CHECK(length(CAST(display_name AS BLOB)) BETWEEN 1 AND 1024),
    created_at_ms INTEGER NOT NULL CHECK(created_at_ms>=0),
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms),
    deleted INTEGER NOT NULL CHECK(deleted IN (0,1)),
    created_command_id BLOB
        CHECK(created_command_id IS NULL OR length(created_command_id)=16)
) STRICT;

CREATE INDEX pod0_speakers_active_name_v1
    ON pod0_speakers(deleted,display_name,speaker_entity_id);

-- Assignments are keyed (artifact_id,speaker_id). Speaker ids derive from
-- (episode, source revision, provider label), so a same-provider
-- re-transcription mints identical ids inside a new artifact and
-- completion-time carry-forward reseeds the link with origin=inferred;
-- diarization indices can permute within a provider across runs, so a carried
-- assignment never claims user authority. origin_code 1|2|3 encodes
-- user|inferred|feed_metadata. Deliberately no foreign key to
-- pod0_transcript_speakers: user naming must never be deleted by artifact
-- pruning (CASCADE) nor block it (RESTRICT), matching pod0_category_members.
CREATE TABLE pod0_speaker_assignments(
    artifact_id BLOB NOT NULL CHECK(length(artifact_id)=16),
    speaker_id BLOB NOT NULL CHECK(length(speaker_id)=16),
    speaker_entity_id BLOB NOT NULL
        REFERENCES pod0_speakers(speaker_entity_id) ON DELETE CASCADE
        CHECK(length(speaker_entity_id)=16),
    confidence REAL
        CHECK(confidence IS NULL OR (confidence>=0.0 AND confidence<=1.0)),
    origin_code INTEGER NOT NULL CHECK(origin_code IN (1,2,3)),
    decided_at_ms INTEGER NOT NULL CHECK(decided_at_ms>=0),
    decided_command_id BLOB
        CHECK(decided_command_id IS NULL OR length(decided_command_id)=16),
    PRIMARY KEY(artifact_id,speaker_id)
) STRICT;

CREATE INDEX pod0_speaker_assignments_entity_v1
    ON pod0_speaker_assignments(speaker_entity_id,artifact_id,speaker_id);
