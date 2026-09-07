-- Permanently remove obsolete external identity and publication state.
-- This migration is local-only: it performs no host calls or network work.

CREATE TEMP TABLE pod0_removed_publication_activities(
    activity_id BLOB PRIMARY KEY NOT NULL,
    transaction_id BLOB NOT NULL
) WITHOUT ROWID;

WITH RECURSIVE retired(activity_id, transaction_id) AS (
    SELECT activity_id, transaction_id
    FROM pod0_activity_facts
    WHERE subject_code = 7
       OR authorized_effect_intent_id IN (
            SELECT intent_id FROM pod0_effect_intents WHERE effect_kind_code = 14
       )
    UNION
    SELECT child.activity_id, child.transaction_id
    FROM pod0_activity_facts child
    JOIN retired parent ON child.caused_by_activity_id = parent.activity_id
)
INSERT OR IGNORE INTO pod0_removed_publication_activities(activity_id, transaction_id)
SELECT activity_id, transaction_id FROM retired;

DELETE FROM pod0_transition_receipts
WHERE transaction_id IN (
    SELECT DISTINCT transaction_id FROM pod0_removed_publication_activities
);

DELETE FROM pod0_effect_attempts
WHERE intent_id IN (
    SELECT intent_id FROM pod0_effect_intents
    WHERE effect_kind_code = 14
       OR authorizing_activity_id IN (
            SELECT activity_id FROM pod0_removed_publication_activities
       )
);

DELETE FROM pod0_effect_intents
WHERE effect_kind_code = 14
   OR authorizing_activity_id IN (
        SELECT activity_id FROM pod0_removed_publication_activities
   );

DELETE FROM pod0_internal_command_intents
WHERE authorizing_activity_id IN (
    SELECT activity_id FROM pod0_removed_publication_activities
);

DROP TRIGGER pod0_activity_facts_no_delete;
DELETE FROM pod0_activity_facts
WHERE activity_id IN (SELECT activity_id FROM pod0_removed_publication_activities);
CREATE TRIGGER pod0_activity_facts_no_delete
BEFORE DELETE ON pod0_activity_facts
BEGIN
    SELECT RAISE(ABORT,'pod0 activity facts are append-only');
END;

DROP TABLE pod0_removed_publication_activities;
DROP TABLE pod0_publication_commands;
DROP TABLE pod0_publication_facts;
DROP TABLE pod0_publications;
DROP TABLE pod0_signer_state;

-- Native memory import was retired before this schema shipped. Rust owns the
-- empty or already-populated memory store from this point forward.
UPDATE pod0_memory_state
SET authority_active = 1,
    source_generation = COALESCE(source_generation, 1)
WHERE singleton = 1 AND authority_active = 0;
