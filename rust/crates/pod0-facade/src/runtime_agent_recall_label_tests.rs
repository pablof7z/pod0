use super::recall_test_support::{approve_next, start_command, uuid_string};
use super::tests::{next_leased_agent_request, record_leased_agent_observation};
use crate::runtime_recall_test_support::{
    RecallFixture, evidence_input, recall_test_embedding, record,
};
use crate::*;

/// Issue #190: the `query_transcripts` tool result must carry the speaker's
/// diarization label and display name, not only the opaque speaker id.
///
/// The label and display name are already durable in
/// `pod0_transcript_speakers` and reach the transcript projection; only the
/// agent surface (`runtime_agent_recall.rs`) drops them. The fixture commits
/// a transcript artifact whose speakers match the evidence spans' speaker
/// ids, so the missing piece is purely the read-side join.
#[test]
fn transcript_query_result_carries_speaker_label_and_display_name() {
    let fixture = RecallFixture::new(true);
    commit_labelled_transcript(&fixture);
    let start = start_command(305);
    fixture.base.facade.dispatch(start.clone());
    let model = next_leased_agent_request(&fixture.base.facade);
    let HostRequest::ExecuteAgentModelTurn { execution } = &model.request.request else {
        panic!("expected model request");
    };
    record_leased_agent_observation(
        &fixture.base.facade,
        &model,
        HostObservation::AgentModelCompleted {
            turn_id: execution.turn_id,
            model_fence_id: execution.model_fence_id,
            assistant_text: "I'll check the transcript.".to_owned(),
            proposed_tool_call: Some(AgentModelToolCallObservation {
                provider_call_id: "recall-call".to_owned(),
                tool_name: "query_transcripts".to_owned(),
                arguments_json: format!(
                    r#"{{"query":"habit cues","episode_id":"{}","limit":2}}"#,
                    uuid_string(fixture.base.episode_id.into_bytes())
                ),
            }),
            usage: None,
        },
    );
    approve_next(&fixture);
    let embed = fixture.base.facade.next_host_requests(1).remove(0);
    let HostRequest::EmbedRecallQuery { query_id, .. } = &embed.request else {
        panic!("expected recall embedding request");
    };
    record(
        &fixture.base.facade,
        &embed,
        HostObservation::RecallQueryEmbedded {
            query_id: *query_id,
            embedding: RecallEmbeddingVector {
                values: recall_test_embedding(),
            },
        },
    );
    let rerank = fixture.base.facade.next_host_requests(1).remove(0);
    assert!(matches!(
        rerank.request,
        HostRequest::RerankRecallCandidates { .. }
    ));
    record(
        &fixture.base.facade,
        &rerank,
        HostObservation::Failed {
            code: HostFailureCode::ProviderUnavailable,
            safe_detail: None,
        },
    );
    let continuation = fixture.base.facade.next_host_requests(1).remove(0);
    let HostRequest::ExecuteAgentModelTurn { execution } = &continuation.request else {
        panic!("expected final model continuation");
    };
    let evidence = execution
        .messages
        .iter()
        .find(|message| message.role == AgentMessageRole::Tool)
        .expect("recall tool evidence must be durable");
    assert!(evidence.content.contains(r#""status":"ready""#));
    assert!(evidence.content.contains("daily cues"));
    assert!(
        evidence.content.contains(r#""speaker_label":"speaker_0""#),
        "query_transcripts evidence must carry the diarization label for the span's speaker, \
         not only an opaque id; tool result was: {}",
        evidence.content
    );
    assert!(
        evidence
            .content
            .contains(r#""speaker_display_name":"Ada Lovelace""#),
        "query_transcripts evidence must carry the display name already stored in \
         pod0_transcript_speakers; tool result was: {}",
        evidence.content
    );
}

/// Commits a selected transcript artifact whose speakers use the same ids as
/// the recall evidence spans (`SpeakerId::from_parts(0, 1)` and `(0, 2)`),
/// carrying Scribe-style labels and display names, then guards that both are
/// already durable through the existing transcript projection.
fn commit_labelled_transcript(fixture: &RecallFixture) {
    let evidence = evidence_input(&fixture.base);
    let artifact = TranscriptArtifactInput {
        episode_id: evidence.episode_id,
        podcast_id: evidence.podcast_id,
        source_revision: evidence.source_revision.clone(),
        source: evidence.source,
        provider: evidence.provider.clone(),
        source_payload_digest: evidence.source_payload_digest,
        language: "en-US".to_owned(),
        generated_at: UnixTimestampMilliseconds::new(1_800_000_000_050),
        speakers: vec![
            TranscriptArtifactSpeakerInput {
                speaker_id: SpeakerId::from_parts(0, 1),
                label: "speaker_0".to_owned(),
                display_name: Some("Ada Lovelace".to_owned()),
            },
            TranscriptArtifactSpeakerInput {
                speaker_id: SpeakerId::from_parts(0, 2),
                label: "speaker_1".to_owned(),
                display_name: Some("Grace Hopper".to_owned()),
            },
        ],
        segments: evidence
            .segments
            .iter()
            .map(|segment| TranscriptArtifactSegmentInput {
                text: segment.text.clone(),
                start_milliseconds: segment.start_milliseconds,
                end_milliseconds: segment.end_milliseconds,
                speaker_id: segment.speaker_id,
                words: Vec::new(),
            })
            .collect(),
    };
    fixture.base.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(305, 90),
        cancellation_id: CancellationId::from_parts(305, 91),
        expected_revision: None,
        command: ApplicationCommand::CommitTranscript {
            expected_selection_revision: StateRevision::INITIAL,
            artifact,
        },
    });
    let Projection::Transcript { value } = fixture
        .base
        .facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Transcript {
                episode_id: fixture.base.episode_id,
                scope: TranscriptProjectionScope::Speakers,
            },
            offset: 0,
            max_items: 10,
        })
        .projection
    else {
        panic!("expected transcript projection");
    };
    assert!(
        value.speakers.iter().any(|speaker| {
            speaker.label == "speaker_0" && speaker.display_name.as_deref() == Some("Ada Lovelace")
        }),
        "fixture guard: the label and display name must already be durable through the \
         transcript projection; only the agent surface is under test"
    );
}
