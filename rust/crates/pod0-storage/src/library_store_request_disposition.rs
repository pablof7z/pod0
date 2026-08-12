use pod0_application::{ActivityActor, ActivityOrigin, ActivitySubject, RequestDisposition};
use pod0_domain::{CommandId, ContentDigest, EpisodeId, UnixTimestampMilliseconds};

use crate::{CommitReceipt, LibraryStore, StorageError};

impl LibraryStore {
    #[allow(clippy::too_many_arguments)]
    pub fn record_request_disposition(
        &self,
        command_id: CommandId,
        fingerprint: ContentDigest,
        subject: ActivitySubject,
        episode_id: Option<EpisodeId>,
        actor: ActivityActor,
        origin: ActivityOrigin,
        disposition: RequestDisposition,
        observed_at: UnixTimestampMilliseconds,
    ) -> Result<CommitReceipt, StorageError> {
        crate::transition_commit::commit_request_disposition(
            self.path(),
            command_id,
            fingerprint,
            subject,
            episode_id,
            actor,
            origin,
            disposition,
            observed_at,
        )
    }
}
