use pod0_application::{
    CoreFailureCode, HostFailureCode, HostObservation, HostObservationEnvelope,
    HostObservationReceipt, ObservationAcceptance, OperationResult,
};
use pod0_domain::{SignerAccountId, UnixTimestampMilliseconds};
use pod0_nmp::NativeSignerFailure;

use crate::runtime_observation_mapping::{accepted, rejected, retain};
use crate::runtime_signer::{
    PendingSignerRequest, SignerObservationResult, SignerRuntimeAction, SignerWaiter,
};
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn record_signer_observation(
        &mut self,
        observation: HostObservationEnvelope,
    ) -> Option<SignerObservationResult> {
        let request_id = observation.request_id;
        if let Some(retained) = self.pending_signer_observations.get(&request_id) {
            if retained != &observation {
                return Some(SignerObservationResult {
                    changed: false,
                    receipt: retain(request_id),
                    action: SignerRuntimeAction::None,
                });
            }
            return Some(self.persist_signer_observation(observation));
        }
        if !self.host_requests.is_signer_request(request_id) {
            return None;
        }
        let acceptance = self.host_requests.accept_observation(&observation);
        if acceptance != ObservationAcceptance::Accepted {
            return Some(SignerObservationResult {
                changed: false,
                receipt: rejected(request_id, acceptance),
                action: SignerRuntimeAction::None,
            });
        }
        let result = self.persist_signer_observation(observation.clone());
        if matches!(
            result.receipt,
            HostObservationReceipt::RetainAndRetry { .. }
        ) {
            self.pending_signer_observations
                .insert(request_id, observation);
        }
        Some(result)
    }

    pub(super) fn finalize_signer_install(
        &mut self,
        account_id: SignerAccountId,
        installed: bool,
    ) -> bool {
        if installed {
            self.finish_ensure_waiters(account_id);
            self.advance_revision();
            return true;
        }
        let Some(author) = self
            .signer_account
            .as_ref()
            .filter(|account| account.account_id == Some(account_id))
            .and_then(|account| account.expected_author_hex.clone())
        else {
            return false;
        };
        if let Some(store) = self.signer_store.as_ref()
            && let Ok(record) = store.mark_unavailable(
                account_id,
                &author,
                self.now(),
                Some("NMP signer registration unavailable"),
            )
        {
            self.signer_account = Some(record);
        }
        self.fail_signer_waiters(SignerWaiterKind::Ensure, CoreFailureCode::HostUnavailable);
        self.advance_revision();
        true
    }

    pub(super) fn fail_signer_waiters(&mut self, kind: SignerWaiterKind, code: CoreFailureCode) {
        let command_ids = self
            .signer_waiters
            .iter()
            .filter_map(|(command_id, waiter)| (kind.matches(*waiter)).then_some(*command_id))
            .collect::<Vec<_>>();
        for command_id in command_ids {
            self.signer_waiters.remove(&command_id);
            self.fail(command_id, code);
        }
    }
}

include!("runtime_signer_observation_persistence.rs");

#[derive(Clone, Copy)]
pub(super) enum SignerWaiterKind {
    Ensure,
    SignOut,
}

impl SignerWaiterKind {
    const fn matches(self, waiter: SignerWaiter) -> bool {
        matches!(
            (self, waiter),
            (Self::Ensure, SignerWaiter::Ensure) | (Self::SignOut, SignerWaiter::SignOut { .. })
        )
    }
}

fn native_signer_failure(code: HostFailureCode, detail: Option<&str>) -> NativeSignerFailure {
    let detail = detail
        .unwrap_or("native signer refused the request")
        .to_owned();
    match code {
        HostFailureCode::PermissionDenied | HostFailureCode::Unauthorized => {
            NativeSignerFailure::Rejected(detail)
        }
        HostFailureCode::InvalidResponse | HostFailureCode::ResponseTooLarge => {
            NativeSignerFailure::InvalidResponse(detail)
        }
        HostFailureCode::TimedOut => NativeSignerFailure::TimedOut,
        _ => NativeSignerFailure::Unavailable,
    }
}

fn host_core_failure(observation: &HostObservation) -> CoreFailureCode {
    match observation {
        HostObservation::Failed { code, .. } => host_core_failure_code(*code),
        HostObservation::Cancelled => CoreFailureCode::Cancelled,
        _ => CoreFailureCode::HostUnavailable,
    }
}

const fn host_core_failure_code(code: HostFailureCode) -> CoreFailureCode {
    match code {
        HostFailureCode::PermissionDenied => CoreFailureCode::HostRejected,
        HostFailureCode::Unauthorized => CoreFailureCode::Unauthorized,
        HostFailureCode::InvalidResponse | HostFailureCode::ResponseTooLarge => {
            CoreFailureCode::InvalidCommand
        }
        _ => CoreFailureCode::HostUnavailable,
    }
}
