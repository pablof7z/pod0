use sha2::{Digest, Sha256};

use pod0_application::{
    CommandEnvelope, CoreFailureCode, HostCancellationRequest, HostObservationReceipt, HostRequest,
    HostRequestEnvelope, NostrSignatureObservation, OperationResult, OperationStage,
    SIGNER_HOST_DEADLINE_MILLISECONDS, SignerProjection,
};
use pod0_domain::{
    CancellationId, CommandId, HostRequestId, SignerAccountId, SignerStage,
    UnixTimestampMilliseconds,
};
use pod0_nmp::{NativeSignerFailure, NativeSigningRequest};

use crate::runtime_signer_observations::SignerWaiterKind;
use crate::runtime_state::FacadeState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PendingSignerRequest {
    Provision,
    Restore { account_id: SignerAccountId },
    Sign,
    Delete { account_id: SignerAccountId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SignerWaiter {
    Ensure,
    SignOut { account_id: SignerAccountId },
}

pub(super) enum SignerRuntimeAction {
    None,
    Install {
        account_id: SignerAccountId,
        public_key_hex: String,
    },
    Remove {
        account_id: SignerAccountId,
    },
    Complete {
        request_id: HostRequestId,
        observation: NostrSignatureObservation,
    },
    Fail {
        request_id: HostRequestId,
        failure: NativeSignerFailure,
    },
}

pub(super) struct SignerObservationResult {
    pub(super) changed: bool,
    pub(super) receipt: HostObservationReceipt,
    pub(super) action: SignerRuntimeAction,
}

impl FacadeState {
    pub(super) fn ensure_nostr_signer(&mut self, envelope: &CommandEnvelope) {
        match self.signer_account.clone() {
            Some(account) if account.stage == SignerStage::Ready => {
                let Some(account_id) = account.account_id else {
                    self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
                    return;
                };
                self.succeed(
                    envelope.command_id,
                    Some(OperationResult::NostrSignerReady { account_id }),
                );
            }
            Some(account)
                if matches!(
                    account.stage,
                    SignerStage::Provisioning | SignerStage::Restoring
                ) =>
            {
                self.signer_waiters
                    .insert(envelope.command_id, SignerWaiter::Ensure);
                self.finish(envelope.command_id, OperationStage::Blocked, None, None);
            }
            Some(account)
                if matches!(
                    account.stage,
                    SignerStage::Unavailable | SignerStage::Failed
                ) =>
            {
                let (Some(account_id), Some(author)) =
                    (account.account_id, account.expected_author_hex.as_deref())
                else {
                    self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
                    return;
                };
                self.signer_waiters
                    .insert(envelope.command_id, SignerWaiter::Ensure);
                if !self.begin_signer_restore(
                    account_id,
                    author,
                    Some((envelope.command_id, envelope.cancellation_id)),
                ) {
                    self.fail_signer_waiters(
                        SignerWaiterKind::Ensure,
                        CoreFailureCode::StorageUnavailable,
                    );
                }
            }
            Some(account) if account.stage == SignerStage::SigningOut => {
                self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            }
            Some(_) => self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable),
            None => {
                self.signer_waiters
                    .insert(envelope.command_id, SignerWaiter::Ensure);
                if !self.begin_signer_provisioning(Some((
                    envelope.command_id,
                    envelope.cancellation_id,
                ))) {
                    self.fail_signer_waiters(
                        SignerWaiterKind::Ensure,
                        CoreFailureCode::StorageUnavailable,
                    );
                }
            }
        }
    }

    pub(super) fn sign_out_nostr_signer(
        &mut self,
        envelope: &CommandEnvelope,
        expected_account_id: SignerAccountId,
    ) {
        let Some(account) = self.signer_account.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::NotFound);
            return;
        };
        if account.account_id != Some(expected_account_id) {
            self.fail(envelope.command_id, CoreFailureCode::RevisionConflict);
            return;
        }
        if account.stage == SignerStage::SigningOut {
            self.signer_waiters.insert(
                envelope.command_id,
                SignerWaiter::SignOut {
                    account_id: expected_account_id,
                },
            );
            self.finish(envelope.command_id, OperationStage::Blocked, None, None);
            return;
        }
        let Some(store) = self.signer_store.as_ref() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        let Ok(record) = store.begin_sign_out(expected_account_id, self.now()) else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        self.signer_account = Some(record);
        self.advance_revision();
        self.signer_waiters.insert(
            envelope.command_id,
            SignerWaiter::SignOut {
                account_id: expected_account_id,
            },
        );
        let request_id = HostRequestId::from_bytes(envelope.command_id.into_bytes());
        self.queue_signer_request(
            request_id,
            envelope.command_id,
            envelope.cancellation_id,
            PendingSignerRequest::Delete {
                account_id: expected_account_id,
            },
            HostRequest::DeleteNostrSignerCredential {
                account_id: expected_account_id,
            },
        );
        self.finish(envelope.command_id, OperationStage::Running, None, None);
    }

    pub(super) fn rehydrate_nostr_signer(&mut self) -> Result<(), pod0_storage::StorageError> {
        let Some(account) = self.signer_account.clone() else {
            return Ok(());
        };
        match account.stage {
            SignerStage::Provisioning => {
                self.begin_signer_provisioning(None);
            }
            SignerStage::Restoring
            | SignerStage::Ready
            | SignerStage::Unavailable
            | SignerStage::Failed => {
                let account_id = account
                    .account_id
                    .ok_or(pod0_storage::StorageError::InvalidSignerState)?;
                let author = account
                    .expected_author_hex
                    .as_deref()
                    .ok_or(pod0_storage::StorageError::InvalidSignerState)?;
                self.begin_signer_restore(account_id, author, None);
            }
            SignerStage::SigningOut => {
                let account_id = account
                    .account_id
                    .ok_or(pod0_storage::StorageError::InvalidSignerState)?;
                let request_id =
                    signer_request_id(b"delete", Some(account_id), account.revision.value);
                self.queue_signer_request(
                    request_id,
                    CommandId::from_bytes(request_id.into_bytes()),
                    CancellationId::from_bytes(request_id.into_bytes()),
                    PendingSignerRequest::Delete { account_id },
                    HostRequest::DeleteNostrSignerCredential { account_id },
                );
            }
        }
        Ok(())
    }

    pub(super) fn enqueue_native_signing_request(&mut self, value: NativeSigningRequest) -> bool {
        if self.pending_signers.contains_key(&value.request_id) {
            return false;
        }
        let command_id = CommandId::from_bytes(value.request_id.into_bytes());
        let cancellation_id = CancellationId::from_bytes(value.request_id.into_bytes());
        self.queue_signer_request(
            value.request_id,
            command_id,
            cancellation_id,
            PendingSignerRequest::Sign,
            HostRequest::SignNostrEvent {
                request: value.request,
            },
        );
        self.advance_revision();
        true
    }

    pub(super) fn cancel_native_signing_request(&mut self, request_id: HostRequestId) -> bool {
        let Some(PendingSignerRequest::Sign) = self.pending_signers.remove(&request_id) else {
            return false;
        };
        let cancellation_id = CancellationId::from_bytes(request_id.into_bytes());
        self.host_queue
            .retain(|request| request.request_id != request_id);
        if self.host_requests.cancel_request(request_id) {
            self.host_cancellations.push_back(HostCancellationRequest {
                request_id,
                cancellation_id,
            });
        }
        self.advance_revision();
        true
    }

    pub(super) fn signer_projection(&self) -> SignerProjection {
        SignerProjection {
            account: self.signer_account.clone(),
            pending_request_count: u16::try_from(self.pending_signers.len()).unwrap_or(u16::MAX),
            operations: self.operations.clone(),
        }
    }
}

include!("runtime_signer_requests.rs");
