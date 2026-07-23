CREATE TABLE pod0_signer_state(
    singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
    account_id BLOB CHECK(account_id IS NULL OR length(account_id)=16),
    credential_kind_code TEXT,
    credential_kind_wire_code INTEGER,
    expected_author_hex TEXT
        CHECK(expected_author_hex IS NULL OR (
            length(expected_author_hex)=64
            AND expected_author_hex=lower(expected_author_hex)
        )),
    state_revision INTEGER NOT NULL CHECK(state_revision >= 0),
    stage_code TEXT NOT NULL CHECK(stage_code IN (
        'unconfigured','provisioning','restoring','ready',
        'unavailable','signing_out','failed'
    )),
    updated_at_ms INTEGER NOT NULL,
    safe_detail TEXT CHECK(
        safe_detail IS NULL OR length(CAST(safe_detail AS BLOB)) <= 512
    ),
    CHECK(
        (stage_code IN ('unconfigured','provisioning')
            AND account_id IS NULL
            AND expected_author_hex IS NULL)
        OR
        (stage_code NOT IN ('unconfigured','provisioning')
            AND account_id IS NOT NULL
            AND credential_kind_code IS NOT NULL
            AND expected_author_hex IS NOT NULL)
    )
) STRICT;

INSERT INTO pod0_signer_state(
    singleton,account_id,credential_kind_code,credential_kind_wire_code,
    expected_author_hex,state_revision,stage_code,updated_at_ms,safe_detail
) VALUES(1,NULL,NULL,NULL,NULL,0,'unconfigured',0,NULL);
