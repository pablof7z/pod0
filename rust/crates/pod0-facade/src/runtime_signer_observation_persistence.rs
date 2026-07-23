impl FacadeState {
    fn persist_signer_observation(
        &mut self,
        observation: HostObservationEnvelope,
    ) -> SignerObservationResult {
        let request_id = observation.request_id;
        let Some(pending) = self.pending_signers.get(&request_id).copied() else {
            return SignerObservationResult {
                changed: false,
                receipt: accepted(request_id),
                action: SignerRuntimeAction::None,
            };
        };
        match self.apply_signer_observation(pending, &observation) {
            Ok(action) => {
                self.pending_signers.remove(&request_id);
                self.pending_signer_observations.remove(&request_id);
                self.advance_revision();
                SignerObservationResult {
                    changed: true,
                    receipt: accepted(request_id),
                    action,
                }
            }
            Err(()) => SignerObservationResult {
                changed: false,
                receipt: retain(request_id),
                action: SignerRuntimeAction::None,
            },
        }
    }

    fn apply_signer_observation(
        &mut self,
        pending: PendingSignerRequest,
        envelope: &HostObservationEnvelope,
    ) -> Result<SignerRuntimeAction, ()> {
        match (pending, &envelope.observation) {
            (
                PendingSignerRequest::Provision | PendingSignerRequest::Restore { .. },
                HostObservation::NostrSignerCredentialReady {
                    account_id,
                    public_key_hex,
                },
            ) => {
                let record = self
                    .signer_store
                    .as_ref()
                    .ok_or(())?
                    .mark_ready(*account_id, public_key_hex, envelope.observed_at)
                    .map_err(|_| ())?;
                self.signer_account = Some(record);
                Ok(SignerRuntimeAction::Install {
                    account_id: *account_id,
                    public_key_hex: public_key_hex.clone(),
                })
            }
            (PendingSignerRequest::Sign, HostObservation::NostrEventSigned { value }) => {
                Ok(SignerRuntimeAction::Complete {
                    request_id: envelope.request_id,
                    observation: value.clone(),
                })
            }
            (
                PendingSignerRequest::Delete { account_id },
                HostObservation::NostrSignerCredentialDeleted { .. },
            ) => {
                self.signer_store
                    .as_ref()
                    .ok_or(())?
                    .clear(account_id, envelope.observed_at)
                    .map_err(|_| ())?;
                self.signer_account = None;
                self.finish_sign_out_waiters(account_id);
                Ok(SignerRuntimeAction::Remove { account_id })
            }
            (PendingSignerRequest::Sign, HostObservation::Failed { code, safe_detail }) => {
                Ok(SignerRuntimeAction::Fail {
                    request_id: envelope.request_id,
                    failure: native_signer_failure(*code, safe_detail.as_deref()),
                })
            }
            (PendingSignerRequest::Sign, HostObservation::Cancelled) => {
                Ok(SignerRuntimeAction::Fail {
                    request_id: envelope.request_id,
                    failure: NativeSignerFailure::Disconnected,
                })
            }
            (PendingSignerRequest::Provision, HostObservation::Failed { .. })
            | (PendingSignerRequest::Provision, HostObservation::Cancelled) => {
                self.signer_store
                    .as_ref()
                    .ok_or(())?
                    .reset_provisioning(envelope.observed_at)
                    .map_err(|_| ())?;
                self.signer_account = None;
                self.fail_signer_waiters(
                    SignerWaiterKind::Ensure,
                    host_core_failure(&envelope.observation),
                );
                Ok(SignerRuntimeAction::None)
            }
            (
                PendingSignerRequest::Restore { account_id },
                HostObservation::Failed {
                    safe_detail, code, ..
                },
            ) => self.mark_signer_unavailable(
                account_id,
                envelope.observed_at,
                safe_detail.as_deref(),
                host_core_failure_code(*code),
                SignerWaiterKind::Ensure,
            ),
            (PendingSignerRequest::Restore { account_id }, HostObservation::Cancelled) => self
                .mark_signer_unavailable(
                    account_id,
                    envelope.observed_at,
                    Some("native signer restoration was cancelled"),
                    CoreFailureCode::Cancelled,
                    SignerWaiterKind::Ensure,
                ),
            (
                PendingSignerRequest::Delete { account_id },
                HostObservation::Failed {
                    safe_detail, code, ..
                },
            ) => self.mark_signer_unavailable(
                account_id,
                envelope.observed_at,
                safe_detail.as_deref(),
                host_core_failure_code(*code),
                SignerWaiterKind::SignOut,
            ),
            (PendingSignerRequest::Delete { account_id }, HostObservation::Cancelled) => self
                .mark_signer_unavailable(
                    account_id,
                    envelope.observed_at,
                    Some("native signer deletion was cancelled"),
                    CoreFailureCode::Cancelled,
                    SignerWaiterKind::SignOut,
                ),
            _ => Err(()),
        }
    }

    fn mark_signer_unavailable(
        &mut self,
        account_id: SignerAccountId,
        observed_at: UnixTimestampMilliseconds,
        detail: Option<&str>,
        failure_code: CoreFailureCode,
        waiter_kind: SignerWaiterKind,
    ) -> Result<SignerRuntimeAction, ()> {
        let author = self
            .signer_account
            .as_ref()
            .and_then(|account| account.expected_author_hex.as_deref())
            .ok_or(())?
            .to_owned();
        let record = self
            .signer_store
            .as_ref()
            .ok_or(())?
            .mark_unavailable(account_id, &author, observed_at, detail)
            .map_err(|_| ())?;
        self.signer_account = Some(record);
        self.fail_signer_waiters(waiter_kind, failure_code);
        Ok(SignerRuntimeAction::Remove { account_id })
    }

    fn finish_ensure_waiters(&mut self, account_id: SignerAccountId) {
        let command_ids = self
            .signer_waiters
            .iter()
            .filter_map(|(command_id, waiter)| {
                matches!(waiter, SignerWaiter::Ensure).then_some(*command_id)
            })
            .collect::<Vec<_>>();
        for command_id in command_ids {
            self.signer_waiters.remove(&command_id);
            self.succeed(
                command_id,
                Some(OperationResult::NostrSignerReady { account_id }),
            );
        }
    }

    fn finish_sign_out_waiters(&mut self, account_id: SignerAccountId) {
        let command_ids = self
            .signer_waiters
            .iter()
            .filter_map(|(command_id, waiter)| {
                matches!(
                    waiter,
                    SignerWaiter::SignOut {
                        account_id: expected
                    } if *expected == account_id
                )
                .then_some(*command_id)
            })
            .collect::<Vec<_>>();
        for command_id in command_ids {
            self.signer_waiters.remove(&command_id);
            self.succeed(
                command_id,
                Some(OperationResult::NostrSignerSignedOut { account_id }),
            );
        }
    }
}
