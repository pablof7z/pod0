use pod0_domain::{
    CommandId, ContentDigest, PRODUCT_SETTINGS_SCHEMA_VERSION, ProductSettings,
    ProductSettingsValues, SettingsWriterVersion, StateRevision,
};

use crate::{
    RequestDisposition, RequestRejectionReason, SettingsChange, SettingsConflictWinner,
    SettingsValidationState, plan_settings_transition,
};

#[test]
fn local_change_advances_authoritative_and_writer_revisions() {
    let current = settings(7, 4, 1, "local");
    let mut values = current.values.clone();
    values.agent_display_name = "Poddy".into();
    let plan = plan_settings_transition(
        command(1),
        StateRevision::new(40),
        Some(&current),
        SettingsChange::Local {
            expected_revision: StateRevision::new(7),
            writer_id: digest(3),
            values: values.clone(),
        },
    )
    .unwrap();
    assert_eq!(plan.disposition(), RequestDisposition::Accepted);
    let (_, _, mutation, _, _, _, _) = plan.into_parts();
    let next = mutation.next.unwrap();
    assert_eq!(next.revision, StateRevision::new(41));
    assert_eq!(next.writer_version.counter, 5);
    assert_eq!(next.values, values);
}

#[test]
fn same_counter_remote_conflict_converges_on_writer_identity() {
    let low = settings(4, 9, 1, "low");
    let high = settings(4, 9, 2, "high");
    let from_low = remote_plan(&low, &high, 10);
    let from_high = remote_plan(&high, &low, 11);

    let (_, _, low_mutation, _, _, _, _) = from_low.into_parts();
    let (_, _, high_mutation, _, _, _, _) = from_high.into_parts();
    assert_eq!(low_mutation.next.unwrap().values.agent_display_name, "high");
    assert!(high_mutation.next.is_none());
    assert_eq!(
        low_mutation.conflict.unwrap().winner,
        SettingsConflictWinner::Candidate
    );
    assert_eq!(
        high_mutation.conflict.unwrap().winner,
        SettingsConflictWinner::Current
    );
}

#[test]
fn stale_local_and_invalid_remote_candidates_never_mutate() {
    let current = settings(7, 4, 1, "local");
    let stale = plan_settings_transition(
        command(4),
        StateRevision::new(40),
        Some(&current),
        SettingsChange::Local {
            expected_revision: StateRevision::new(6),
            writer_id: digest(1),
            values: ProductSettingsValues::default(),
        },
    )
    .unwrap();
    assert_eq!(
        stale.disposition(),
        RequestDisposition::Rejected {
            reason: RequestRejectionReason::RevisionConflict
        }
    );
    let invalid = ProductSettingsValues {
        default_playback_rate_milli: 4_000,
        ..ProductSettingsValues::default()
    };
    let rejected = plan_settings_transition(
        command(5),
        StateRevision::new(40),
        Some(&current),
        SettingsChange::Remote {
            schema_version: PRODUCT_SETTINGS_SCHEMA_VERSION,
            writer_version: SettingsWriterVersion {
                counter: 10,
                writer_id: digest(2),
            },
            values: invalid,
        },
    )
    .unwrap();
    let (_, _, mutation, _, _, _, disposition) = rejected.into_parts();
    assert!(matches!(disposition, RequestDisposition::Rejected { .. }));
    assert!(mutation.next.is_none());
    assert_ne!(mutation.validation, SettingsValidationState::Valid);
}

fn remote_plan(
    current: &ProductSettings,
    candidate: &ProductSettings,
    id: u64,
) -> crate::SettingsTransitionPlan {
    plan_settings_transition(
        command(id),
        StateRevision::new(40),
        Some(current),
        SettingsChange::Remote {
            schema_version: candidate.schema_version,
            writer_version: candidate.writer_version,
            values: candidate.values.clone(),
        },
    )
    .unwrap()
}

fn settings(revision: u64, counter: u64, writer: u8, name: &str) -> ProductSettings {
    let values = ProductSettingsValues {
        agent_display_name: name.into(),
        ..ProductSettingsValues::default()
    };
    ProductSettings {
        schema_version: PRODUCT_SETTINGS_SCHEMA_VERSION,
        revision: StateRevision::new(revision),
        writer_version: SettingsWriterVersion {
            counter,
            writer_id: digest(writer),
        },
        values,
    }
}

fn command(value: u64) -> CommandId {
    CommandId::from_parts(45, value)
}
fn digest(value: u8) -> ContentDigest {
    ContentDigest::from_bytes([value; 32])
}
