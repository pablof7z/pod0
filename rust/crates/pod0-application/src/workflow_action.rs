use pod0_domain::{ContentDigest, EpisodeId, ScheduledOccurrenceId, StateRevision};
use sha2::{Digest as _, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum WorkflowActionKind {
    Retry,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum WorkflowActionTarget {
    PublisherChapters {
        episode_id: EpisodeId,
    },
    ModelChapters {
        episode_id: EpisodeId,
    },
    Transcript {
        episode_id: EpisodeId,
    },
    Download {
        episode_id: EpisodeId,
    },
    ScheduledAgent {
        occurrence_id: ScheduledOccurrenceId,
    },
}

/// Exact opaque authorization emitted by a Rust projection. Swift may return
/// the token but cannot choose a command variant, revision, or configuration.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record,
)]
pub struct WorkflowActionToken {
    pub action: WorkflowActionKind,
    pub target: WorkflowActionTarget,
    pub expected_workflow_revision: StateRevision,
    pub authorization: ContentDigest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum WorkflowActionDispatchResult {
    Accepted,
    InvalidToken,
    Stale,
    NotAllowed,
    NotFound,
    StorageUnavailable,
}

impl WorkflowActionToken {
    #[must_use]
    pub fn issue(
        action: WorkflowActionKind,
        target: WorkflowActionTarget,
        expected_workflow_revision: StateRevision,
    ) -> Self {
        Self {
            action,
            target,
            expected_workflow_revision,
            authorization: workflow_action_authorization(
                action,
                target,
                expected_workflow_revision,
            ),
        }
    }

    #[must_use]
    pub fn is_structurally_valid(self) -> bool {
        self.authorization
            == workflow_action_authorization(
                self.action,
                self.target,
                self.expected_workflow_revision,
            )
    }
}

fn workflow_action_authorization(
    action: WorkflowActionKind,
    target: WorkflowActionTarget,
    revision: StateRevision,
) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0:workflow-action:v1");
    hash.update([match action {
        WorkflowActionKind::Retry => 1,
        WorkflowActionKind::Cancel => 2,
    }]);
    match target {
        WorkflowActionTarget::PublisherChapters { episode_id } => {
            hash.update([1]);
            hash.update(episode_id.into_bytes());
        }
        WorkflowActionTarget::ModelChapters { episode_id } => {
            hash.update([2]);
            hash.update(episode_id.into_bytes());
        }
        WorkflowActionTarget::Transcript { episode_id } => {
            hash.update([3]);
            hash.update(episode_id.into_bytes());
        }
        WorkflowActionTarget::Download { episode_id } => {
            hash.update([4]);
            hash.update(episode_id.into_bytes());
        }
        WorkflowActionTarget::ScheduledAgent { occurrence_id } => {
            hash.update([5]);
            hash.update(occurrence_id.into_bytes());
        }
    }
    hash.update(revision.value.to_be_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_authorization_is_exact_to_action_target_and_revision() {
        let episode_id = EpisodeId::from_parts(1, 2);
        let token = WorkflowActionToken::issue(
            WorkflowActionKind::Retry,
            WorkflowActionTarget::Transcript { episode_id },
            StateRevision::new(7),
        );
        assert!(token.is_structurally_valid());
        assert!(
            !WorkflowActionToken {
                action: WorkflowActionKind::Cancel,
                ..token
            }
            .is_structurally_valid()
        );
        assert!(
            !WorkflowActionToken {
                expected_workflow_revision: StateRevision::new(8),
                ..token
            }
            .is_structurally_valid()
        );
    }
}
