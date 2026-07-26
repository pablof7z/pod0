ALTER TABLE pod0_subscriptions
ADD COLUMN transcript_start_policy_code INTEGER NOT NULL DEFAULT 1
    CHECK(transcript_start_policy_code IN (1, 2, 255));

ALTER TABLE pod0_subscriptions
ADD COLUMN transcript_start_policy_wire_code INTEGER;
