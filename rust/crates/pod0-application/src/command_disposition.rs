use std::collections::BTreeMap;

use pod0_domain::{CancellationId, CommandId, StateRevision};

use crate::{CommandEnvelope, CoreFailureCode};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum CommandRejectionReason {
    InvalidInput,
    CommandIdentityConflict,
    MissingSubject,
    Unsupported,
    PrivacyBoundary,
    MissingPrerequisite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum CommandDisposition {
    Applied,
    Rejected {
        reason: CommandRejectionReason,
    },
    Stale {
        expected_revision: StateRevision,
        actual_revision: StateRevision,
    },
    Duplicate,
    NotAllowed,
    AlreadyComplete,
    NoOp,
    Cancelled,
    Failed {
        code: CoreFailureCode,
    },
    OutcomeUnknown,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct CommandReceipt {
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub expected_revision: Option<StateRevision>,
    pub committed_revision: StateRevision,
    pub disposition: CommandDisposition,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandAdmission {
    Ready,
    Complete(CommandReceipt),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandCompletionError {
    UnknownCommand,
    AlreadyComplete,
}

#[derive(Clone)]
struct CommandEntry {
    envelope: CommandEnvelope,
    receipt: Option<CommandReceipt>,
}

#[derive(Default)]
pub struct CommandReceiptLedger {
    entries: BTreeMap<CommandId, CommandEntry>,
}

impl CommandReceiptLedger {
    pub fn admit(
        &mut self,
        envelope: CommandEnvelope,
        current_revision: StateRevision,
    ) -> CommandAdmission {
        if let Some(existing) = self.entries.get(&envelope.command_id) {
            if existing.envelope != envelope {
                return CommandAdmission::Complete(receipt(
                    &envelope,
                    current_revision,
                    CommandDisposition::Rejected {
                        reason: CommandRejectionReason::CommandIdentityConflict,
                    },
                    false,
                ));
            }
            return CommandAdmission::Complete(existing.receipt.clone().map_or_else(
                || {
                    receipt(
                        &envelope,
                        current_revision,
                        CommandDisposition::Duplicate,
                        false,
                    )
                },
                |mut value| {
                    value.replayed = true;
                    value
                },
            ));
        }

        if let Some(expected_revision) = envelope.expected_revision
            && expected_revision != current_revision
        {
            let completed = receipt(
                &envelope,
                current_revision,
                CommandDisposition::Stale {
                    expected_revision,
                    actual_revision: current_revision,
                },
                false,
            );
            self.entries.insert(
                envelope.command_id,
                CommandEntry {
                    envelope,
                    receipt: Some(completed.clone()),
                },
            );
            return CommandAdmission::Complete(completed);
        }

        self.entries.insert(
            envelope.command_id,
            CommandEntry {
                envelope,
                receipt: None,
            },
        );
        CommandAdmission::Ready
    }

    pub fn complete(
        &mut self,
        command_id: CommandId,
        committed_revision: StateRevision,
        disposition: CommandDisposition,
    ) -> Result<CommandReceipt, CommandCompletionError> {
        let entry = self
            .entries
            .get_mut(&command_id)
            .ok_or(CommandCompletionError::UnknownCommand)?;
        if entry.receipt.is_some() {
            return Err(CommandCompletionError::AlreadyComplete);
        }
        let completed = receipt(&entry.envelope, committed_revision, disposition, false);
        entry.receipt = Some(completed.clone());
        Ok(completed)
    }

    #[must_use]
    pub fn receipt(&self, command_id: CommandId) -> Option<&CommandReceipt> {
        self.entries.get(&command_id)?.receipt.as_ref()
    }
}

fn receipt(
    envelope: &CommandEnvelope,
    committed_revision: StateRevision,
    disposition: CommandDisposition,
    replayed: bool,
) -> CommandReceipt {
    CommandReceipt {
        command_id: envelope.command_id,
        cancellation_id: envelope.cancellation_id,
        expected_revision: envelope.expected_revision,
        committed_revision,
        disposition,
        replayed,
    }
}
