use pod0_application::{
    ApplicationCommand, TranscriptProvider as Provider, TranscriptWorkflowOrigin as Origin,
};
use sha2::{Digest as _, Sha256};

pub(super) fn hash_transcript_workflow_command(hash: &mut Sha256, command: &ApplicationCommand) {
    match command {
        ApplicationCommand::EnsureTranscriptWorkflow {
            episode_id,
            origin,
            configuration,
        } => {
            hash.update(b"ensure-transcript-workflow\0");
            hash.update(episode_id.into_bytes());
            hash_configuration(hash, *origin, configuration);
        }
        ApplicationCommand::RetryTranscriptWorkflow {
            episode_id,
            expected_workflow_revision,
            configuration,
        } => {
            hash.update(b"retry-transcript-workflow\0");
            hash.update(episode_id.into_bytes());
            hash.update(expected_workflow_revision.value.to_be_bytes());
            hash_configuration(hash, Origin::User, configuration);
        }
        ApplicationCommand::CancelTranscriptWorkflow {
            episode_id,
            expected_workflow_revision,
        } => {
            hash.update(b"cancel-transcript-workflow\0");
            hash.update(episode_id.into_bytes());
            hash.update(expected_workflow_revision.value.to_be_bytes());
        }
        _ => unreachable!("transcript workflow fingerprint routing"),
    }
}

fn hash_configuration(
    hash: &mut Sha256,
    origin: Origin,
    configuration: &pod0_application::TranscriptWorkflowConfiguration,
) {
    match origin {
        Origin::User => hash.update([1]),
        Origin::Automatic => hash.update([2]),
        Origin::Playback => hash.update([3]),
        Origin::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
    hash_transcript_configuration(hash, configuration);
}

pub(super) fn hash_transcript_configuration(
    hash: &mut Sha256,
    configuration: &pod0_application::TranscriptWorkflowConfiguration,
) {
    match configuration.provider {
        Provider::AssemblyAi => hash.update([1]),
        Provider::ElevenLabsScribe => hash.update([2]),
        Provider::OpenRouterWhisper => hash.update([3]),
        Provider::AppleSpeech => hash.update([4]),
        Provider::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
    hash.update(configuration.model.as_bytes());
    hash.update([0]);
    if let Some(url) = &configuration.local_audio_url {
        hash.update(url.as_bytes());
    }
    hash.update([
        0,
        u8::from(configuration.credential_available),
        u8::from(configuration.auto_publisher_enabled),
        u8::from(configuration.auto_provider_enabled),
    ]);
}
