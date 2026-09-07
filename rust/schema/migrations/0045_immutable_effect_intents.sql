CREATE TRIGGER pod0_effect_intents_request_immutable
BEFORE UPDATE OF
    intent_id,
    authorizing_activity_id,
    authorizing_fact_code,
    correlation_id,
    effect_kind_code,
    subject_code,
    subject_id,
    episode_id,
    request_schema_version,
    request_json,
    available_at_ms,
    deadline_at_ms,
    committed_at_ms
ON pod0_effect_intents
BEGIN
    SELECT RAISE(ABORT,'pod0 effect requests are immutable');
END;

CREATE TRIGGER pod0_effect_intents_no_delete
BEFORE DELETE ON pod0_effect_intents
BEGIN
    SELECT RAISE(ABORT,'pod0 effect requests are immutable');
END;
