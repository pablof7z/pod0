-- Decode-only pre-exact effect requests can never be executed safely: their payload was derived
-- from mutable workflow state after authorization. Fence only the exact legacy wire variant.
CREATE TABLE pod0_legacy_effect_recovery_v40(
    intent_id BLOB PRIMARY KEY NOT NULL REFERENCES pod0_effect_intents(intent_id),
    prior_intent_state_code INTEGER NOT NULL CHECK(prior_intent_state_code IN(1,2)),
    prior_fence INTEGER NOT NULL CHECK(prior_fence>=0),
    recovery_activity_id BLOB UNIQUE
        CHECK(recovery_activity_id IS NULL OR length(recovery_activity_id)=16),
    recovery_code TEXT NOT NULL CHECK(recovery_code='outcome_unknown_legacy_domain_derived')
) STRICT;

INSERT INTO pod0_legacy_effect_recovery_v40(
    intent_id,prior_intent_state_code,prior_fence,recovery_code
)
SELECT intent_id,state_code,fence,'outcome_unknown_legacy_domain_derived'
FROM pod0_effect_intents
WHERE state_code IN(1,2)
  AND json_type(request_json,'$.execution')='text'
  AND json_extract(request_json,'$.execution')='DomainDerived';

UPDATE pod0_effect_attempts
SET state_code=4
WHERE state_code IN(1,2)
  AND intent_id IN(SELECT intent_id FROM pod0_legacy_effect_recovery_v40);

UPDATE pod0_effect_intents
SET state_code=4
WHERE intent_id IN(SELECT intent_id FROM pod0_legacy_effect_recovery_v40);
