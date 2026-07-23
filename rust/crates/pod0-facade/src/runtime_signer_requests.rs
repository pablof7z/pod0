impl FacadeState {
    fn begin_signer_provisioning(&mut self, owner: Option<(CommandId, CancellationId)>) -> bool {
        let Some(store) = self.signer_store.as_ref() else {
            return false;
        };
        let Ok(record) = store.begin_provisioning(self.now()) else {
            return false;
        };
        self.signer_account = Some(record.clone());
        self.advance_revision();
        let request_id = owner.map_or_else(
            || signer_request_id(b"provision", None, record.revision.value),
            |(command_id, _)| HostRequestId::from_bytes(command_id.into_bytes()),
        );
        let (command_id, cancellation_id) = owner.unwrap_or_else(|| {
            (
                CommandId::from_bytes(request_id.into_bytes()),
                CancellationId::from_bytes(request_id.into_bytes()),
            )
        });
        self.queue_signer_request(
            request_id,
            command_id,
            cancellation_id,
            PendingSignerRequest::Provision,
            HostRequest::ProvisionNostrSignerCredential,
        );
        true
    }

    fn begin_signer_restore(
        &mut self,
        account_id: SignerAccountId,
        author: &str,
        owner: Option<(CommandId, CancellationId)>,
    ) -> bool {
        let Some(store) = self.signer_store.as_ref() else {
            return false;
        };
        let Ok(record) = store.begin_restoring(account_id, author, self.now()) else {
            return false;
        };
        self.signer_account = Some(record.clone());
        self.advance_revision();
        let request_id = owner.map_or_else(
            || signer_request_id(b"restore", Some(account_id), record.revision.value),
            |(command_id, _)| HostRequestId::from_bytes(command_id.into_bytes()),
        );
        let (command_id, cancellation_id) = owner.unwrap_or_else(|| {
            (
                CommandId::from_bytes(request_id.into_bytes()),
                CancellationId::from_bytes(request_id.into_bytes()),
            )
        });
        self.queue_signer_request(
            request_id,
            command_id,
            cancellation_id,
            PendingSignerRequest::Restore { account_id },
            HostRequest::RestoreNostrSignerCredential {
                account_id,
                expected_author_hex: author.to_owned(),
            },
        );
        true
    }

    fn queue_signer_request(
        &mut self,
        request_id: HostRequestId,
        command_id: CommandId,
        cancellation_id: CancellationId,
        pending: PendingSignerRequest,
        request: HostRequest,
    ) {
        let envelope = HostRequestEnvelope {
            request_id,
            command_id,
            cancellation_id,
            issued_revision: self.revision,
            deadline_at: Some(UnixTimestampMilliseconds::new(
                self.now()
                    .value
                    .saturating_add(SIGNER_HOST_DEADLINE_MILLISECONDS),
            )),
            request,
        };
        if self.host_requests.register(envelope.clone()) {
            self.pending_signers.insert(request_id, pending);
            self.host_queue.push_back(envelope);
        }
    }
}

fn signer_request_id(
    phase: &[u8],
    account_id: Option<SignerAccountId>,
    revision: u64,
) -> HostRequestId {
    let mut digest = Sha256::new();
    digest.update(b"pod0-nostr-signer\0");
    digest.update(phase);
    if let Some(account_id) = account_id {
        digest.update(account_id.into_bytes());
    }
    digest.update(revision.to_be_bytes());
    let bytes: [u8; 32] = digest.finalize().into();
    HostRequestId::from_bytes(bytes[..16].try_into().expect("SHA-256 prefix"))
}
