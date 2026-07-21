use pod0_domain::{
    ChapterArtifact, ChapterArtifactId, ChapterArtifactInput, ChapterArtifactSource,
    MAX_CHAPTER_MODEL_BYTES, MAX_PROVENANCE_PROVIDER_BYTES,
};

use crate::{
    CHAPTER_MODEL_FORMAT_VERSION, CHAPTER_MODEL_POLICY_ID, CHAPTER_MODEL_POLICY_VERSION,
    ChapterModelDesiredStateInput, ChapterModelDesiredStatePlan, ChapterModelObservationMode,
    ChapterModelPlan, ChapterModelPlanInput, ChapterModelResponseFormat,
    MAX_CHAPTER_MODEL_EPISODE_TEXT_BYTES, MAX_CHAPTER_MODEL_TRANSCRIPT_INPUT_BYTES,
    MAX_CHAPTER_MODEL_TRANSCRIPT_SEGMENTS, MAX_MODEL_CHAPTER_COMPLETION_BYTES,
    MAX_MODEL_CHAPTER_PROMPT_BYTES, PlannedChapterModelRequest,
};

pub(crate) use crate::chapter_model_policy_version::input_version;

pub(crate) fn desired_state(
    input: ChapterModelDesiredStateInput,
    policy_id: &str,
) -> ChapterModelDesiredStatePlan {
    match input.selected_chapter_source {
        Some(ChapterArtifactSource::AgentComposed) => {
            ChapterModelDesiredStatePlan::PreserveAgentComposed
        }
        Some(ChapterArtifactSource::Unsupported { .. }) => {
            ChapterModelDesiredStatePlan::UnsupportedArtifact
        }
        _ => ChapterModelDesiredStatePlan::Compile {
            input_version: input_version(
                input.transcript_content_digest,
                &input.configured_model,
                policy_id,
            ),
        },
    }
}

pub(crate) fn request(input: ChapterModelPlanInput) -> ChapterModelPlan {
    let Some(transcript) = input.selected_transcript.as_ref() else {
        return ChapterModelPlan::TranscriptUnavailable;
    };
    if transcript.transcript_version_id != input.requested_transcript_version_id
        || transcript.transcript_content_digest != input.requested_transcript_content_digest
    {
        return ChapterModelPlan::StaleTranscript;
    }
    if !valid_episode(&input) || !valid_transcript(transcript) {
        return ChapterModelPlan::InvalidInput;
    }
    if transcript.segments.is_empty() {
        return ChapterModelPlan::EmptyTranscript;
    }
    if exceeds_input_bounds(&input) {
        return ChapterModelPlan::InputTooLarge;
    }

    let selected_source = input
        .selected_chapter_artifact
        .as_ref()
        .map(|artifact| artifact.provenance.source);
    let desired = desired_state(
        ChapterModelDesiredStateInput {
            transcript_content_digest: input.requested_transcript_content_digest,
            configured_model: input.configured_model.clone(),
            selected_chapter_source: selected_source,
        },
        CHAPTER_MODEL_POLICY_ID,
    );
    let source_version = match desired {
        ChapterModelDesiredStatePlan::Compile { input_version } => input_version,
        ChapterModelDesiredStatePlan::PreserveAgentComposed => {
            return ChapterModelPlan::PreserveAgentComposed;
        }
        ChapterModelDesiredStatePlan::UnsupportedArtifact => {
            return ChapterModelPlan::UnsupportedArtifact;
        }
    };
    let Some((provider, model)) = effective_model(&input.configured_model) else {
        return ChapterModelPlan::InvalidConfiguration;
    };
    let Some(duration_milliseconds) = duration_milliseconds(input.episode.duration_seconds) else {
        return ChapterModelPlan::InvalidInput;
    };
    if let Some(artifact_id) = input
        .selected_chapter_artifact
        .as_ref()
        .and_then(|artifact| current_model_artifact(artifact, &source_version, &provider, &input))
    {
        return ChapterModelPlan::Current { artifact_id };
    }
    let (mode, expected_artifact_source, system_prompt, user_prompt) = match mode(&input) {
        Mode::Generate => (
            ChapterModelObservationMode::Generate,
            ChapterArtifactSource::Generated,
            crate::chapter_model_policy_prompt::GENERATION_SYSTEM_PROMPT.to_owned(),
            crate::chapter_model_policy_prompt::generation_user_prompt(&input.episode, transcript),
        ),
        Mode::Enrich(artifact) => {
            let user = crate::chapter_model_policy_prompt::enrichment_user_prompt(
                &input.episode,
                transcript,
                &artifact.chapters,
            );
            (
                ChapterModelObservationMode::Enrich {
                    publisher_artifact: *artifact,
                },
                ChapterArtifactSource::PublisherEnriched,
                crate::chapter_model_policy_prompt::ENRICHMENT_SYSTEM_PROMPT.to_owned(),
                user,
            )
        }
        Mode::PreserveAgentComposed => return ChapterModelPlan::PreserveAgentComposed,
        Mode::UnsupportedArtifact => return ChapterModelPlan::UnsupportedArtifact,
    };
    if system_prompt.len().saturating_add(user_prompt.len()) > MAX_MODEL_CHAPTER_PROMPT_BYTES {
        return ChapterModelPlan::InputTooLarge;
    }
    ChapterModelPlan::Ready {
        request: PlannedChapterModelRequest {
            source_version,
            episode_id: input.episode.episode_id,
            podcast_id: input.episode.podcast_id,
            format_version: CHAPTER_MODEL_FORMAT_VERSION,
            requested_transcript_version_id: input.requested_transcript_version_id,
            requested_transcript_content_digest: input.requested_transcript_content_digest,
            selected_transcript_version_id: transcript.transcript_version_id,
            selected_transcript_content_digest: transcript.transcript_content_digest,
            policy_version: CHAPTER_MODEL_POLICY_VERSION,
            provider,
            model,
            system_prompt,
            user_prompt,
            response_format: ChapterModelResponseFormat::JsonObject,
            maximum_completion_bytes: MAX_MODEL_CHAPTER_COMPLETION_BYTES as u64,
            duration_milliseconds,
            mode,
            expected_artifact_source,
            expected_chapter_selection_revision: input.expected_chapter_selection_revision,
        },
    }
}

fn current_model_artifact(
    input: &ChapterArtifactInput,
    source_version: &str,
    provider: &str,
    plan: &ChapterModelPlanInput,
) -> Option<ChapterArtifactId> {
    let artifact = ChapterArtifact::seal(input.clone()).ok()?;
    let source_is_model = matches!(
        artifact.provenance.source,
        ChapterArtifactSource::Generated | ChapterArtifactSource::PublisherEnriched
    );
    (source_is_model
        && artifact.episode_id == plan.episode.episode_id
        && artifact.podcast_id == plan.episode.podcast_id
        && artifact.source_revision == source_version
        && artifact.provenance.policy_version == CHAPTER_MODEL_POLICY_VERSION
        && artifact.provenance.provider.as_deref() == Some(provider)
        && artifact
            .provenance
            .model
            .as_ref()
            .is_some_and(|value| !value.is_empty())
        && artifact.provenance.transcript_version_id == Some(plan.requested_transcript_version_id)
        && artifact.provenance.transcript_content_digest
            == Some(plan.requested_transcript_content_digest)
        && artifact.provenance.legacy_import.is_none())
    .then_some(artifact.artifact_id)
}

enum Mode {
    Generate,
    Enrich(Box<pod0_domain::ChapterArtifactInput>),
    PreserveAgentComposed,
    UnsupportedArtifact,
}

fn mode(input: &ChapterModelPlanInput) -> Mode {
    let Some(selected) = input.selected_chapter_artifact.clone() else {
        return Mode::Generate;
    };
    let source = selected.provenance.source;
    if matches!(source, ChapterArtifactSource::Generated) {
        return Mode::Generate;
    }
    if !matches!(
        source,
        ChapterArtifactSource::Publisher | ChapterArtifactSource::PublisherEnriched
    ) {
        return match source {
            ChapterArtifactSource::AgentComposed => Mode::PreserveAgentComposed,
            _ => Mode::UnsupportedArtifact,
        };
    }
    let Ok(selected) = ChapterArtifact::seal(selected) else {
        return Mode::UnsupportedArtifact;
    };
    if selected.episode_id != input.episode.episode_id
        || selected.podcast_id != input.episode.podcast_id
    {
        return Mode::UnsupportedArtifact;
    }
    let Some(base) = input.publisher_base_artifact.clone() else {
        return Mode::UnsupportedArtifact;
    };
    let Ok(base) = ChapterArtifact::seal(base) else {
        return Mode::UnsupportedArtifact;
    };
    if base.episode_id != input.episode.episode_id
        || base.podcast_id != input.episode.podcast_id
        || base.provenance.source != ChapterArtifactSource::Publisher
        || (selected.provenance.source == ChapterArtifactSource::Publisher
            && selected.artifact_id != base.artifact_id)
    {
        return Mode::UnsupportedArtifact;
    }
    Mode::Enrich(Box::new(base.as_input()))
}

fn valid_episode(input: &ChapterModelPlanInput) -> bool {
    input.episode.title.len() <= MAX_CHAPTER_MODEL_EPISODE_TEXT_BYTES
        && input.episode.description.len() <= MAX_CHAPTER_MODEL_EPISODE_TEXT_BYTES
        && input
            .episode
            .duration_seconds
            .is_none_or(|value| value.is_finite() && value >= 0.0)
}

fn valid_transcript(transcript: &crate::ChapterModelTranscriptInput) -> bool {
    transcript
        .segments
        .iter()
        .all(|segment| segment.start_seconds.is_finite() && segment.start_seconds >= 0.0)
}

fn exceeds_input_bounds(input: &ChapterModelPlanInput) -> bool {
    let transcript = input
        .selected_transcript
        .as_ref()
        .expect("checked by caller");
    if transcript.segments.len() > MAX_CHAPTER_MODEL_TRANSCRIPT_SEGMENTS {
        return true;
    }
    let bytes = transcript.segments.iter().fold(0_usize, |total, segment| {
        total.saturating_add(segment.text.len())
    });
    bytes > MAX_CHAPTER_MODEL_TRANSCRIPT_INPUT_BYTES
}

fn duration_milliseconds(duration: Option<f64>) -> Option<Option<u64>> {
    match duration {
        None => Some(None),
        Some(value) if value.is_finite() && value >= 0.0 => {
            let milliseconds = value * 1_000.0;
            (milliseconds <= u64::MAX as f64).then(|| Some(milliseconds.round() as u64))
        }
        Some(_) => None,
    }
}

fn effective_model(stored_id: &str) -> Option<(String, String)> {
    let value = stored_id.trim();
    let (provider, model) = value.find(':').map_or_else(
        || ("openrouter", value),
        |index| {
            let prefix = &value[..index];
            let remainder = value[index + 1..].trim();
            if matches!(prefix, "openrouter" | "ollama") && !remainder.is_empty() {
                (prefix, remainder)
            } else {
                ("openrouter", value)
            }
        },
    );
    (!model.is_empty()
        && provider.len() <= MAX_PROVENANCE_PROVIDER_BYTES
        && model.len() <= MAX_CHAPTER_MODEL_BYTES)
        .then(|| (provider.to_owned(), model.to_owned()))
}
