use pod0_application::{HostObservation, HostObservationEnvelope};
use pod0_domain::{ContentDigest, EpisodeId};
use sha2::{Digest as _, Sha256};

pub(super) fn fingerprint(observation: &HostObservationEnvelope) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/playback-observation-ingress/v1");
    hash.update(observation.request_id.into_bytes());
    hash.update(observation.cancellation_id.into_bytes());
    hash.update(observation.observed_request_revision.value.to_be_bytes());
    hash.update(observation.sequence_number.to_be_bytes());
    hash.update(observation.observed_at.value.to_be_bytes());
    match &observation.observation {
        HostObservation::PlaybackObserved { value } => {
            hash.update([1]);
            hash.update(value.episode_id.map_or([0; 16], EpisodeId::into_bytes));
            state(&mut hash, value.state);
            hash.update(value.position_milliseconds.to_be_bytes());
            hash.update(value.duration_milliseconds.to_be_bytes());
            route(&mut hash, value.route);
            interruption(&mut hash, value.interruption);
            hash.update([u8::from(value.ended)]);
        }
        HostObservation::Failed { code, safe_detail } => {
            hash.update([2]);
            failure(&mut hash, *code);
            hash.update(safe_detail.as_deref().unwrap_or_default().as_bytes());
        }
        HostObservation::Cancelled => hash.update([3]),
        HostObservation::Unsupported { wire_code } => {
            hash.update([4]);
            hash.update(wire_code.to_be_bytes());
        }
        _ => hash.update([255]),
    }
    ContentDigest::from_bytes(hash.finalize().into())
}

fn state(hash: &mut Sha256, value: pod0_application::PlaybackHostState) {
    use pod0_application::PlaybackHostState as S;
    let (tag, wire) = match value {
        S::Idle => (0, None),
        S::Loading => (1, None),
        S::Prepared => (2, None),
        S::Playing => (3, None),
        S::Paused => (4, None),
        S::Buffering => (5, None),
        S::Failed => (6, None),
        S::Unsupported { wire_code } => (7, Some(wire_code)),
    };
    tagged(hash, tag, wire);
}

fn route(hash: &mut Sha256, value: pod0_application::PlaybackAudioRoute) {
    use pod0_application::PlaybackAudioRoute as R;
    let (tag, wire) = match value {
        R::BuiltIn => (0, None),
        R::Wired => (1, None),
        R::Bluetooth => (2, None),
        R::AirPlay => (3, None),
        R::Car => (4, None),
        R::External => (5, None),
        R::Unknown => (6, None),
        R::Unsupported { wire_code } => (7, Some(wire_code)),
    };
    tagged(hash, tag, wire);
}

fn interruption(hash: &mut Sha256, value: pod0_application::PlaybackInterruption) {
    use pod0_application::PlaybackInterruption as I;
    let (tag, wire) = match value {
        I::None => (0, None),
        I::Began => (1, None),
        I::EndedShouldResume => (2, None),
        I::EndedShouldRemainPaused => (3, None),
        I::RouteLost => (4, None),
        I::MediaServicesReset => (5, None),
        I::Unsupported { wire_code } => (6, Some(wire_code)),
    };
    tagged(hash, tag, wire);
}

fn failure(hash: &mut Sha256, value: pod0_application::HostFailureCode) {
    use pod0_application::HostFailureCode as F;
    let (tag, wire) = match value {
        F::Offline => (0, None),
        F::TimedOut => (1, None),
        F::PermissionDenied => (2, None),
        F::InvalidResponse => (3, None),
        F::ResponseTooLarge => (4, None),
        F::MediaUnavailable => (5, None),
        F::ProviderUnavailable => (6, None),
        F::Unauthorized => (7, None),
        F::IndexUnavailable => (8, None),
        F::PlatformFailure => (9, None),
        F::Unsupported { wire_code } => (10, Some(wire_code)),
    };
    tagged(hash, tag, wire);
}

fn tagged(hash: &mut Sha256, tag: u8, wire: Option<u32>) {
    hash.update([tag]);
    if let Some(wire) = wire {
        hash.update(wire.to_be_bytes());
    }
}
