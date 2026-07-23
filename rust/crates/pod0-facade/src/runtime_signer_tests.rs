use std::sync::Arc;

use pod0_application::SignerProjection;
use pod0_domain::{SignerAccountId, SignerStage};

use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[derive(Clone, Copy)]
struct FixedClock(i64);

impl pod0_application::Clock for FixedClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        UnixTimestampMilliseconds::new(self.0)
    }
}

fn dispatch(facade: &Pod0Facade, command_id: u64, command: ApplicationCommand) -> CommandEnvelope {
    let envelope = CommandEnvelope {
        command_id: CommandId::from_parts(91, command_id),
        cancellation_id: CancellationId::from_parts(92, command_id),
        expected_revision: None,
        command,
    };
    facade.dispatch(envelope.clone());
    envelope
}

fn signer_request(facade: &Pod0Facade) -> HostRequestEnvelope {
    facade
        .next_host_requests(64)
        .into_iter()
        .find(|request| {
            matches!(
                request.request,
                HostRequest::ProvisionNostrSignerCredential
                    | HostRequest::RestoreNostrSignerCredential { .. }
                    | HostRequest::DeleteNostrSignerCredential { .. }
            )
        })
        .expect("signer host request")
}

fn observe(request: &HostRequestEnvelope, observation: HostObservation) -> HostObservationEnvelope {
    HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number: 0,
        observed_at: UnixTimestampMilliseconds::new(1_800_000_000_100),
        observation,
    }
}

fn signer(facade: &Pod0Facade) -> SignerProjection {
    let Projection::NostrSigner { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::NostrSigner,
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("signer projection")
    };
    value
}

#[test]
fn provision_ready_sign_out_has_one_durable_owner() {
    let fixture = PlaybackFixture::new();
    let facade = fixture.facade;
    let ensure = dispatch(&facade, 1, ApplicationCommand::EnsureNostrSigner);
    let provision = signer_request(&facade);
    assert!(matches!(
        provision.request,
        HostRequest::ProvisionNostrSignerCredential
    ));
    let account_id = SignerAccountId::from_parts(3, 4);
    let public_key = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    assert!(matches!(
        facade.record_host_observation(observe(
            &provision,
            HostObservation::NostrSignerCredentialReady {
                account_id,
                public_key_hex: public_key.into(),
            },
        )),
        HostObservationReceipt::AcceptedTransient { .. }
    ));
    let ready = signer(&facade);
    assert_eq!(
        ready.account.as_ref().and_then(|value| value.account_id),
        Some(account_id)
    );
    assert_eq!(
        ready.account.as_ref().map(|value| value.stage),
        Some(SignerStage::Ready)
    );
    assert!(ready.operations.iter().any(|operation| {
        operation.command_id == ensure.command_id
            && operation.stage == OperationStage::Succeeded
            && operation.result == Some(OperationResult::NostrSignerReady { account_id })
    }));

    let sign_out = dispatch(
        &facade,
        2,
        ApplicationCommand::SignOutNostrSigner {
            expected_account_id: account_id,
        },
    );
    let deletion = signer_request(&facade);
    assert!(matches!(
        deletion.request,
        HostRequest::DeleteNostrSignerCredential {
            account_id: value
        } if value == account_id
    ));
    let _ = facade.record_host_observation(observe(
        &deletion,
        HostObservation::NostrSignerCredentialDeleted { account_id },
    ));
    let signed_out = signer(&facade);
    assert!(signed_out.account.is_none());
    assert!(signed_out.operations.iter().any(|operation| {
        operation.command_id == sign_out.command_id
            && operation.result == Some(OperationResult::NostrSignerSignedOut { account_id })
    }));
}

#[test]
fn process_restart_restores_only_metadata_and_requests_keychain_capability() {
    let fixture = PlaybackFixture::new();
    let PlaybackFixture {
        _directory,
        target,
        facade,
        ..
    } = fixture;
    dispatch(&facade, 3, ApplicationCommand::EnsureNostrSigner);
    let provision = signer_request(&facade);
    let account_id = SignerAccountId::from_parts(5, 6);
    let public_key = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    let _ = facade.record_host_observation(observe(
        &provision,
        HostObservation::NostrSignerCredentialReady {
            account_id,
            public_key_hex: public_key.into(),
        },
    ));
    drop(facade);

    let reopened = Pod0Facade::open_with_clock(
        target.to_string_lossy().into_owned(),
        Arc::new(FixedClock(1_800_000_001_000)),
    );
    let restore = signer_request(&reopened);
    assert!(matches!(
        restore.request,
        HostRequest::RestoreNostrSignerCredential {
            account_id: value,
            expected_author_hex: ref author,
        } if value == account_id && author == public_key
    ));
    let restoring = signer(&reopened);
    assert_eq!(
        restoring.account.as_ref().map(|value| value.stage),
        Some(SignerStage::Restoring)
    );
    let _ = reopened.record_host_observation(observe(
        &restore,
        HostObservation::NostrSignerCredentialReady {
            account_id,
            public_key_hex: public_key.into(),
        },
    ));
    assert_eq!(
        signer(&reopened).account.as_ref().map(|value| value.stage),
        Some(SignerStage::Ready)
    );
    drop(reopened);
    drop(_directory);
}

#[test]
fn restore_failure_keeps_identity_metadata_and_marks_capability_unavailable() {
    let fixture = PlaybackFixture::new();
    let facade = fixture.facade;
    dispatch(&facade, 4, ApplicationCommand::EnsureNostrSigner);
    let provision = signer_request(&facade);
    let account_id = SignerAccountId::from_parts(7, 8);
    let public_key = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    let _ = facade.record_host_observation(observe(
        &provision,
        HostObservation::NostrSignerCredentialReady {
            account_id,
            public_key_hex: public_key.into(),
        },
    ));

    dispatch(&facade, 5, ApplicationCommand::EnsureNostrSigner);
    assert!(facade.next_host_requests(64).is_empty());
    {
        let mut state = facade.state();
        let store = state.signer_store.clone().unwrap();
        store
            .begin_restoring(
                account_id,
                public_key,
                UnixTimestampMilliseconds::new(1_800_000_002_000),
            )
            .unwrap();
        state.signer_account = store.account().unwrap();
        state.rehydrate_nostr_signer().unwrap();
    }
    let restore = signer_request(&facade);
    let _ = facade.record_host_observation(observe(
        &restore,
        HostObservation::Failed {
            code: HostFailureCode::PlatformFailure,
            safe_detail: Some("Keychain item unavailable".into()),
        },
    ));
    let unavailable = signer(&facade).account.unwrap();
    assert_eq!(unavailable.account_id, Some(account_id));
    assert_eq!(unavailable.expected_author_hex.as_deref(), Some(public_key));
    assert_eq!(unavailable.stage, SignerStage::Unavailable);
}
