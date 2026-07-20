CREATE TABLE pod0_chapter_selections_v14(
    episode_id BLOB NOT NULL CHECK(length(episode_id) = 16),
    selection_revision INTEGER NOT NULL CHECK(selection_revision >= 1),
    artifact_id BLOB NOT NULL CHECK(length(artifact_id) = 16),
    source_import_id BLOB CHECK(
        source_import_id IS NULL OR length(source_import_id) = 16
    ),
    selected_at_ms INTEGER NOT NULL CHECK(selected_at_ms >= 0),
    PRIMARY KEY(episode_id, selection_revision),
    UNIQUE(episode_id, source_import_id),
    FOREIGN KEY(artifact_id, episode_id)
        REFERENCES pod0_chapter_artifacts(artifact_id, episode_id),
    FOREIGN KEY(source_import_id) REFERENCES pod0_chapter_imports(import_id)
) STRICT;

INSERT INTO pod0_chapter_selections_v14(
    episode_id,selection_revision,artifact_id,source_import_id,selected_at_ms
)
SELECT episode_id,selection_revision,artifact_id,source_import_id,selected_at_ms
FROM pod0_chapter_selections;

DROP TABLE pod0_chapter_selections;
ALTER TABLE pod0_chapter_selections_v14 RENAME TO pod0_chapter_selections;

CREATE INDEX pod0_chapter_selections_import_idx
ON pod0_chapter_selections(source_import_id, selection_revision);

CREATE TABLE pod0_chapter_state_v14(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton = 1),
    collection_revision INTEGER NOT NULL CHECK(collection_revision >= 0),
    authority_active INTEGER NOT NULL CHECK(authority_active IN (0, 1)),
    authority_import_id BLOB REFERENCES pod0_chapter_imports(import_id),
    CHECK(
        (authority_active = 0 AND authority_import_id IS NULL)
        OR
        (authority_active = 1 AND authority_import_id IS NOT NULL)
    )
) STRICT;

INSERT INTO pod0_chapter_state_v14(
    singleton,collection_revision,authority_active,authority_import_id
)
SELECT singleton,collection_revision,authority_active,authority_import_id
FROM pod0_chapter_state;

DROP TABLE pod0_chapter_state;
ALTER TABLE pod0_chapter_state_v14 RENAME TO pod0_chapter_state;

CREATE TABLE pod0_chapter_commands(
    command_id BLOB PRIMARY KEY NOT NULL CHECK(length(command_id) = 16),
    operation_code INTEGER NOT NULL CHECK(operation_code = 1),
    command_fingerprint BLOB NOT NULL CHECK(length(command_fingerprint) = 32),
    episode_id BLOB NOT NULL CHECK(length(episode_id) = 16),
    artifact_id BLOB NOT NULL CHECK(length(artifact_id) = 16),
    expected_selection_revision INTEGER NOT NULL
        CHECK(expected_selection_revision >= 0),
    previous_artifact_id BLOB CHECK(
        previous_artifact_id IS NULL OR length(previous_artifact_id) = 16
    ),
    resulting_selection_revision INTEGER NOT NULL
        CHECK(resulting_selection_revision >= 1),
    already_selected INTEGER NOT NULL CHECK(already_selected IN (0, 1)),
    completed_at_ms INTEGER NOT NULL CHECK(completed_at_ms >= 0),
    FOREIGN KEY(artifact_id, episode_id)
        REFERENCES pod0_chapter_artifacts(artifact_id, episode_id),
    FOREIGN KEY(previous_artifact_id) REFERENCES pod0_chapter_artifacts(artifact_id)
) STRICT;

CREATE INDEX pod0_chapter_commands_episode_idx
ON pod0_chapter_commands(episode_id, completed_at_ms DESC);
