fn reason_matches_record(reason: CoreWakeReason, record: &ModelChapterWorkflowRecord) -> bool {
    match reason {
        CoreWakeReason::ModelChapterRetry { episode_id, .. } => episode_id == record.episode_id,
        CoreWakeReason::ModelChapterFinalization { request_id } => {
            Some(request_id) == record.request_id
        }
        CoreWakeReason::TranscriptProviderRecovery { .. }
        | CoreWakeReason::TranscriptRetry { .. }
        | CoreWakeReason::TranscriptFinalization { .. }
        | CoreWakeReason::FeedDiscoveryNotificationRetry { .. } => false,
        CoreWakeReason::Unsupported { .. } => false,
    }
}

fn wake_request_id(reason: CoreWakeReason, wake_at_ms: i64) -> HostRequestId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-core-wake-v1\0");
    hash.update(wake_at_ms.to_be_bytes());
    match reason {
        CoreWakeReason::ModelChapterRetry {
            episode_id,
            generation,
            submission_fence_id,
        } => {
            hash.update([1]);
            hash.update(episode_id.into_bytes());
            hash.update(generation.to_be_bytes());
            hash.update(submission_fence_id.into_bytes());
        }
        CoreWakeReason::ModelChapterFinalization { request_id } => {
            hash.update([2]);
            hash.update(request_id.into_bytes());
        }
        CoreWakeReason::TranscriptProviderRecovery {
            episode_id,
            attempt_id,
            submission_fence_id,
        } => {
            hash.update([3]);
            hash.update(episode_id.into_bytes());
            hash.update(attempt_id.into_bytes());
            hash.update(submission_fence_id.into_bytes());
        }
        CoreWakeReason::TranscriptRetry {
            episode_id,
            attempt_id,
            submission_fence_id,
        } => {
            hash.update([4]);
            hash.update(episode_id.into_bytes());
            hash.update(attempt_id.into_bytes());
            hash.update(submission_fence_id.into_bytes());
        }
        CoreWakeReason::TranscriptFinalization { request_id } => {
            hash.update([5]);
            hash.update(request_id.into_bytes());
        }
        CoreWakeReason::FeedDiscoveryNotificationRetry {
            occurrence_id,
            episode_id,
            attempt,
        } => {
            hash.update([6]);
            hash.update(occurrence_id.into_bytes());
            hash.update(episode_id.into_bytes());
            hash.update([attempt]);
        }
        CoreWakeReason::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
    let digest = hash.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    HostRequestId::from_bytes(bytes)
}
