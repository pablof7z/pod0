use crate::{
    MAX_TRANSCRIPT_MODEL_BYTES, TranscriptEvidenceDecision, TranscriptGenerationDecision,
    TranscriptProvider, TranscriptWorkflowFailureCode, TranscriptWorkflowOrigin,
    TranscriptWorkflowPlan, TranscriptWorkflowPlanInput, TranscriptWorkflowRequest,
    transcript_evidence_input_version, transcript_workflow_id,
};

const MAX_PUBLISHER_MIME_HINT_BYTES: usize = 128;

#[must_use]
pub fn plan_transcript_workflow(input: TranscriptWorkflowPlanInput) -> TranscriptWorkflowPlan {
    let evidence = plan_evidence(&input);
    if input
        .committed_transcript
        .as_ref()
        .is_some_and(|value| value.source_revision == input.source_revision)
    {
        return TranscriptWorkflowPlan {
            generation: TranscriptGenerationDecision::Current,
            request: None,
            evidence,
        };
    }
    let request = match transcript_workflow_request(&input) {
        Ok(Some(request)) => request,
        Ok(None) => return no_generation(evidence),
        Err(code) => return blocked(code, evidence),
    };
    if !request.publisher_first
        && input.configured_provider.requires_credential()
        && !input.credential_available
    {
        return match input.origin {
            TranscriptWorkflowOrigin::User => TranscriptWorkflowPlan {
                generation: TranscriptGenerationDecision::AwaitingCredential {
                    provider: input.configured_provider,
                },
                request: None,
                evidence,
            },
            TranscriptWorkflowOrigin::Automatic | TranscriptWorkflowOrigin::Unsupported { .. } => {
                no_generation(evidence)
            }
            TranscriptWorkflowOrigin::Playback => TranscriptWorkflowPlan {
                generation: TranscriptGenerationDecision::AwaitingCredential {
                    provider: input.configured_provider,
                },
                request: None,
                evidence,
            },
        };
    }
    if !request.publisher_first
        && input.configured_provider.requires_local_audio()
        && input.local_audio_url.is_none()
    {
        return TranscriptWorkflowPlan {
            generation: TranscriptGenerationDecision::AwaitingLocalAudio,
            request: None,
            evidence,
        };
    }
    TranscriptWorkflowPlan {
        generation: TranscriptGenerationDecision::Ensure,
        request: Some(request),
        evidence,
    }
}

/// Builds the stable request identity and capability facts without consulting
/// transient credential or local-file availability. Migration uses this to
/// preserve terminal and recoverable legacy work while Rust remains the only
/// owner of request policy and identity.
pub fn transcript_workflow_request(
    input: &TranscriptWorkflowPlanInput,
) -> Result<Option<TranscriptWorkflowRequest>, TranscriptWorkflowFailureCode> {
    let (publisher_first, provider_fallback_enabled) = requested_paths(input);
    if !publisher_first && !provider_fallback_enabled {
        return Ok(None);
    }
    if !valid_source_revision(&input.source_revision)
        || !valid_model(&input.configured_model)
        || crate::normalize_media_url(&input.remote_audio_url).is_none()
        || invalid_optional_url(input.local_audio_url.as_deref())
        || (publisher_first && invalid_publisher(input.publisher_transcript_url.as_deref()))
        || (publisher_first
            && input
                .publisher_mime_hint
                .as_ref()
                .is_some_and(|value| value.len() > MAX_PUBLISHER_MIME_HINT_BYTES))
    {
        return Err(TranscriptWorkflowFailureCode::InvalidRequest);
    }
    if provider_fallback_enabled
        && matches!(
            input.configured_provider,
            TranscriptProvider::Unsupported { .. }
        )
    {
        return Err(TranscriptWorkflowFailureCode::UnsupportedProvider);
    }
    Ok(Some(TranscriptWorkflowRequest {
        workflow_id: transcript_workflow_id(
            input.episode_id,
            &input.source_revision,
            input.configured_provider,
            &input.configured_model,
        ),
        episode_id: input.episode_id,
        source_revision: input.source_revision.clone(),
        origin: input.origin,
        provider: input.configured_provider,
        model: input.configured_model.clone(),
        remote_audio_url: input.remote_audio_url.clone(),
        local_audio_url: input.local_audio_url.clone(),
        publisher_transcript_url: input.publisher_transcript_url.clone(),
        publisher_mime_hint: input.publisher_mime_hint.clone(),
        publisher_first,
        provider_fallback_enabled,
    }))
}

fn requested_paths(input: &TranscriptWorkflowPlanInput) -> (bool, bool) {
    match input.origin {
        TranscriptWorkflowOrigin::User => (false, true),
        TranscriptWorkflowOrigin::Automatic => (
            input.auto_publisher_enabled && input.publisher_transcript_url.is_some(),
            input.auto_provider_enabled,
        ),
        TranscriptWorkflowOrigin::Playback => (
            input.auto_publisher_enabled && input.publisher_transcript_url.is_some(),
            true,
        ),
        TranscriptWorkflowOrigin::Unsupported { .. } => (false, false),
    }
}

fn plan_evidence(input: &TranscriptWorkflowPlanInput) -> TranscriptEvidenceDecision {
    let Some(committed) = input
        .committed_transcript
        .as_ref()
        .filter(|value| value.source_revision == input.source_revision)
    else {
        return TranscriptEvidenceDecision::AwaitingTranscript;
    };
    let Some(expected) = transcript_evidence_input_version(
        committed.transcript_version_id,
        committed.content_digest,
        &input.embedding_space_id,
    ) else {
        return TranscriptEvidenceDecision::Blocked {
            code: TranscriptWorkflowFailureCode::InvalidRequest,
        };
    };
    if input.selected_evidence_input_version.as_deref() == Some(expected.as_str()) {
        TranscriptEvidenceDecision::Current
    } else {
        TranscriptEvidenceDecision::Ensure {
            input_version: expected,
        }
    }
}

fn no_generation(evidence: TranscriptEvidenceDecision) -> TranscriptWorkflowPlan {
    TranscriptWorkflowPlan {
        generation: TranscriptGenerationDecision::NotRequested,
        request: None,
        evidence,
    }
}

fn blocked(
    code: TranscriptWorkflowFailureCode,
    evidence: TranscriptEvidenceDecision,
) -> TranscriptWorkflowPlan {
    TranscriptWorkflowPlan {
        generation: TranscriptGenerationDecision::Blocked { code },
        request: None,
        evidence,
    }
}

fn valid_source_revision(value: &str) -> bool {
    !value.trim().is_empty()
        && value.trim() == value
        && value.len() <= pod0_domain::MAX_SOURCE_REVISION_BYTES
}

fn valid_model(value: &str) -> bool {
    !value.trim().is_empty() && value.trim() == value && value.len() <= MAX_TRANSCRIPT_MODEL_BYTES
}

fn invalid_optional_url(value: Option<&str>) -> bool {
    value.is_some_and(|url| crate::normalize_media_url(url).is_none())
}

fn invalid_publisher(value: Option<&str>) -> bool {
    value.is_some_and(|url| {
        crate::normalize_media_url(url).is_none()
            || !(url.starts_with("https://") || url.starts_with("http://"))
    })
}
