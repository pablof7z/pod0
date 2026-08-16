use pod0_application::{
    ApplicationCommand, CommandEnvelope, CoreFailureCode, RequestDisposition,
    WorkflowCapabilitySnapshot, WorkflowCapabilitySnapshotInput, WorkflowConfigurationInput,
    WorkflowOpportunity,
};
use pod0_domain::{ContentDigest, StateRevision};

use crate::runtime_command_fingerprint::command_fingerprint_digest;
use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    pub(super) fn accept_workflow_configuration_command(
        &mut self,
        envelope: &CommandEnvelope,
        command: ApplicationCommand,
    ) {
        match command {
            ApplicationCommand::ImportLegacyWorkflowConfiguration {
                configuration,
                source_generation,
            } => self.import_legacy_workflow_configuration(
                envelope,
                configuration,
                source_generation,
            ),
            ApplicationCommand::SetWorkflowConfiguration {
                expected_configuration_revision,
                configuration,
            } => self.set_workflow_configuration(
                envelope,
                expected_configuration_revision,
                configuration,
            ),
            ApplicationCommand::ObserveWorkflowCapabilities { capabilities } => {
                self.observe_workflow_capabilities(envelope, capabilities)
            }
            ApplicationCommand::ReconcileWorkflowOpportunity { opportunity } => {
                self.reconcile_workflow_opportunity(envelope, opportunity)
            }
            _ => unreachable!("workflow configuration router received another command"),
        }
    }

    pub(super) fn import_legacy_workflow_configuration(
        &mut self,
        envelope: &CommandEnvelope,
        configuration: WorkflowConfigurationInput,
        source_generation: ContentDigest,
    ) {
        let result = self.store.as_ref().map_or(
            Err(pod0_storage::StorageError::CutoverNotAuthoritative),
            |store| {
                store.import_legacy_workflow_configuration(
                    envelope.command_id,
                    command_fingerprint_digest(&envelope.command),
                    configuration,
                    source_generation,
                    self.now().value,
                )
            },
        );
        self.finish_workflow_configuration(envelope, result.map(|value| value.receipt));
    }

    pub(super) fn set_workflow_configuration(
        &mut self,
        envelope: &CommandEnvelope,
        expected_revision: StateRevision,
        configuration: WorkflowConfigurationInput,
    ) {
        let result = self.store.as_ref().map_or(
            Err(pod0_storage::StorageError::CutoverNotAuthoritative),
            |store| {
                store.set_workflow_configuration(
                    envelope.command_id,
                    command_fingerprint_digest(&envelope.command),
                    expected_revision,
                    configuration,
                    self.now().value,
                )
            },
        );
        self.finish_workflow_configuration(envelope, result.map(|value| value.receipt));
    }

    pub(super) fn observe_workflow_capabilities(
        &mut self,
        envelope: &CommandEnvelope,
        capabilities: WorkflowCapabilitySnapshotInput,
    ) {
        let snapshot = match WorkflowCapabilitySnapshot::from_input(capabilities, self.now()) {
            Ok(value) => value,
            Err(_) => {
                self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
                return;
            }
        };
        let result = self.store.as_ref().map_or(
            Err(pod0_storage::StorageError::CutoverNotAuthoritative),
            |store| {
                store.observe_workflow_capabilities(
                    envelope.command_id,
                    command_fingerprint_digest(&envelope.command),
                    snapshot,
                )
            },
        );
        self.finish_workflow_configuration(envelope, result.map(|value| value.receipt));
    }

    pub(super) fn reconcile_workflow_opportunity(
        &mut self,
        envelope: &CommandEnvelope,
        opportunity: WorkflowOpportunity,
    ) {
        let result = self.store.as_ref().map_or(
            Err(pod0_storage::StorageError::CutoverNotAuthoritative),
            |store| {
                store.reconcile_workflow_opportunity(
                    envelope.command_id,
                    command_fingerprint_digest(&envelope.command),
                    opportunity,
                )
            },
        );
        let committed = result.is_ok();
        self.finish_workflow_configuration(envelope, result.map(|value| value.receipt));
        if committed {
            self.resume_workflow_internal_commands();
        }
    }

    fn finish_workflow_configuration(
        &mut self,
        envelope: &CommandEnvelope,
        result: Result<pod0_storage::CommitReceipt, pod0_storage::StorageError>,
    ) {
        match result {
            Ok(receipt) => {
                self.revision =
                    StateRevision::new(self.revision.value.max(receipt.committed_revision.value));
                match receipt.disposition {
                    RequestDisposition::Accepted
                    | RequestDisposition::Duplicate
                    | RequestDisposition::NoSemanticChange => {
                        self.succeed(envelope.command_id, None)
                    }
                    RequestDisposition::Rejected {
                        reason: pod0_application::RequestRejectionReason::RevisionConflict,
                    } => self.fail(envelope.command_id, CoreFailureCode::RevisionConflict),
                    RequestDisposition::Rejected { .. } => {
                        self.fail(envelope.command_id, CoreFailureCode::InvalidCommand)
                    }
                    _ => self.fail(envelope.command_id, CoreFailureCode::InvalidCommand),
                }
            }
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }
}
