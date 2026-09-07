use pod0_domain::{CancellationId, CommandId, StateRevision};

use crate::{
    ApplicationCommand, CommandAdmission, CommandCompletionError, CommandDisposition,
    CommandEnvelope, CommandReceiptLedger, CommandRejectionReason, CoreFailureCode,
};

fn command(id: u64, expected_revision: Option<u64>) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(200, id),
        cancellation_id: CancellationId::from_parts(201, id),
        expected_revision: expected_revision.map(StateRevision::new),
        command: ApplicationCommand::ResetListeningData,
    }
}

#[test]
fn completed_command_replay_returns_original_typed_disposition() {
    let mut ledger = CommandReceiptLedger::default();
    let envelope = command(1, Some(7));
    assert_eq!(
        ledger.admit(envelope.clone(), StateRevision::new(7)),
        CommandAdmission::Ready
    );
    let original = ledger
        .complete(
            envelope.command_id,
            StateRevision::new(8),
            CommandDisposition::Applied,
        )
        .unwrap();
    assert!(!original.replayed);

    let CommandAdmission::Complete(replayed) = ledger.admit(envelope, StateRevision::new(99))
    else {
        panic!("exact replay must be complete")
    };
    assert_eq!(replayed.disposition, CommandDisposition::Applied);
    assert_eq!(replayed.committed_revision, StateRevision::new(8));
    assert!(replayed.replayed);
}

#[test]
fn stale_expected_revision_is_typed_and_replays_without_admission() {
    let mut ledger = CommandReceiptLedger::default();
    let envelope = command(2, Some(4));
    let CommandAdmission::Complete(stale) = ledger.admit(envelope.clone(), StateRevision::new(5))
    else {
        panic!("stale command must not be admitted")
    };
    assert_eq!(
        stale.disposition,
        CommandDisposition::Stale {
            expected_revision: StateRevision::new(4),
            actual_revision: StateRevision::new(5),
        }
    );
    let CommandAdmission::Complete(replayed) = ledger.admit(envelope, StateRevision::new(6)) else {
        panic!("stale replay must preserve the original result")
    };
    assert_eq!(replayed.disposition, stale.disposition);
    assert_eq!(replayed.committed_revision, StateRevision::new(5));
    assert!(replayed.replayed);
}

#[test]
fn command_identity_reuse_is_rejected_without_replacing_original() {
    let mut ledger = CommandReceiptLedger::default();
    let original = command(3, None);
    assert_eq!(
        ledger.admit(original.clone(), StateRevision::INITIAL),
        CommandAdmission::Ready
    );
    let mut conflict = original.clone();
    conflict.cancellation_id = CancellationId::from_parts(202, 3);
    let CommandAdmission::Complete(rejected) = ledger.admit(conflict, StateRevision::INITIAL)
    else {
        panic!("conflicting identity must be rejected")
    };
    assert_eq!(
        rejected.disposition,
        CommandDisposition::Rejected {
            reason: CommandRejectionReason::CommandIdentityConflict,
        }
    );
    assert!(ledger.receipt(original.command_id).is_none());
    assert!(
        ledger
            .complete(
                original.command_id,
                StateRevision::new(1),
                CommandDisposition::NoOp,
            )
            .is_ok()
    );
}

#[test]
fn invalid_input_rejection_authorizes_no_mutation_or_effect() {
    let mut ledger = CommandReceiptLedger::default();
    let envelope = command(4, None);
    assert_eq!(
        ledger.admit(envelope.clone(), StateRevision::new(10)),
        CommandAdmission::Ready
    );
    let state_mutations = 0;
    let effect_authorizations = 0;
    let rejected = ledger
        .complete(
            envelope.command_id,
            StateRevision::new(10),
            CommandDisposition::Rejected {
                reason: CommandRejectionReason::InvalidInput,
            },
        )
        .unwrap();
    assert_eq!(rejected.committed_revision, StateRevision::new(10));
    assert_eq!(state_mutations, 0);
    assert_eq!(effect_authorizations, 0);
}

#[test]
fn contract_round_trips_every_required_disposition() {
    let dispositions = [
        CommandDisposition::Applied,
        CommandDisposition::Rejected {
            reason: CommandRejectionReason::MissingPrerequisite,
        },
        CommandDisposition::Stale {
            expected_revision: StateRevision::new(1),
            actual_revision: StateRevision::new(2),
        },
        CommandDisposition::Duplicate,
        CommandDisposition::NotAllowed,
        CommandDisposition::AlreadyComplete,
        CommandDisposition::NoOp,
        CommandDisposition::Cancelled,
        CommandDisposition::Failed {
            code: CoreFailureCode::HostUnavailable,
        },
        CommandDisposition::OutcomeUnknown,
    ];
    for disposition in dispositions {
        let encoded = serde_json::to_vec(&disposition).unwrap();
        assert_eq!(
            serde_json::from_slice::<CommandDisposition>(&encoded).unwrap(),
            disposition
        );
    }
}

#[test]
fn completion_requires_one_known_pending_command() {
    let mut ledger = CommandReceiptLedger::default();
    let envelope = command(5, None);
    assert_eq!(
        ledger.complete(
            envelope.command_id,
            StateRevision::INITIAL,
            CommandDisposition::Applied,
        ),
        Err(CommandCompletionError::UnknownCommand)
    );
    assert_eq!(
        ledger.admit(envelope.clone(), StateRevision::INITIAL),
        CommandAdmission::Ready
    );
    ledger
        .complete(
            envelope.command_id,
            StateRevision::new(1),
            CommandDisposition::Applied,
        )
        .unwrap();
    assert_eq!(
        ledger.complete(
            envelope.command_id,
            StateRevision::new(2),
            CommandDisposition::Applied,
        ),
        Err(CommandCompletionError::AlreadyComplete)
    );
}
