use pod0_application::{
    ActivitySubject, DurableLifecycleEffectRequest, LifecycleWakeAdmissionInput,
    plan_lifecycle_wake_admission,
};
use pod0_domain::{ContentDigest, UnixTimestampMilliseconds};
use sha2::{Digest as _, Sha256};

use crate::transition_commit::TransitionCommit;
use crate::{LibraryStore, StorageError, TransitionIngress, TransitionIngressKind};

impl LibraryStore {
    pub fn authorize_lifecycle_wake(
        &self,
        request: DurableLifecycleEffectRequest,
        committed_at: UnixTimestampMilliseconds,
    ) -> Result<crate::CommitReceipt, StorageError> {
        let fingerprint = wake_fingerprint(&request)?;
        let subject = wake_subject(request.reason);
        TransitionCommit::open(self.path())?.commit_planned_with(
            TransitionIngress {
                kind: TransitionIngressKind::Recovery,
                id: request.request_id.into_bytes(),
                fingerprint,
            },
            committed_at,
            |_| {
                plan_lifecycle_wake_admission(LifecycleWakeAdmissionInput { request, subject })
                    .map_err(|_| StorageError::InvalidActivity)
            },
            |_, expected, ()| Ok(expected),
        )
    }
}

pub(crate) fn wake_subject(reason: pod0_application::CoreWakeReason) -> ActivitySubject {
    match reason {
        pod0_application::CoreWakeReason::ModelChapterRetry { episode_id, .. }
        | pod0_application::CoreWakeReason::TranscriptProviderRecovery { episode_id, .. }
        | pod0_application::CoreWakeReason::TranscriptRetry { episode_id, .. }
        | pod0_application::CoreWakeReason::FeedDiscoveryNotificationRetry { episode_id, .. } => {
            ActivitySubject::Episode { episode_id }
        }
        pod0_application::CoreWakeReason::ModelChapterFinalization { .. }
        | pod0_application::CoreWakeReason::TranscriptFinalization { .. }
        | pod0_application::CoreWakeReason::FeedFetchRetry { .. }
        | pod0_application::CoreWakeReason::Unsupported { .. } => ActivitySubject::Global,
    }
}

fn wake_fingerprint(
    request: &DurableLifecycleEffectRequest,
) -> Result<ContentDigest, StorageError> {
    let bytes = serde_json::to_vec(request).map_err(|_| StorageError::InvalidActivity)?;
    Ok(ContentDigest::from_bytes(Sha256::digest(bytes).into()))
}
