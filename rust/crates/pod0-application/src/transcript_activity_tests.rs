use pod0_domain::{
    CommandId, EpisodeId, HostRequestId, StateRevision, TranscriptWorkflowId,
    UnixTimestampMilliseconds,
};

use crate::{
    ActivityFact, ActivityOrigin, ActivitySubject, ExternalEffectKind, RequestDisposition,
    TranscriptSubmissionActivityInput, TranscriptWorkflowOrigin, plan_transcript_submission,
};

#[test]
fn transcript_submission_plan_couples_transition_fact_and_effect() {
    let episode_id = EpisodeId::from_parts(1, 2);
    let workflow_id = TranscriptWorkflowId::from_parts(3, 4);
    let request_id = HostRequestId::from_parts(5, 6);
    let plan = plan_transcript_submission(TranscriptSubmissionActivityInput {
        request_id,
        command_id: CommandId::from_parts(7, 8),
        episode_id,
        workflow_id,
        workflow_revision: StateRevision::new(9),
        origin: TranscriptWorkflowOrigin::Playback,
        deadline_at: Some(UnixTimestampMilliseconds::new(20_000)),
        execution: crate::DurableTranscriptEffectRequest {
            request_id,
            command_id: CommandId::from_parts(7, 8),
            cancellation_id: pod0_domain::CancellationId::from_parts(9, 1),
            issued_revision: StateRevision::new(9),
            deadline_at: Some(UnixTimestampMilliseconds::new(20_000)),
            capability: crate::TranscriptCapabilityRequest::FetchPublisher {
                context: crate::TranscriptCapabilityContext {
                    episode_id,
                    podcast_id: pod0_domain::PodcastId::from_parts(2, 3),
                    source_revision: "source-v1".to_owned(),
                },
                source_url: "https://example.com/transcript.vtt".to_owned(),
                mime_hint: None,
                maximum_response_bytes: crate::MAX_TRANSCRIPT_CAPABILITY_RESPONSE_BYTES,
            },
        },
    })
    .unwrap();
    assert_eq!(plan.disposition(), RequestDisposition::Accepted);
    let (_, expected, _, facts, effects, commands, _) = plan.into_parts();
    assert_eq!(expected, StateRevision::new(9));
    assert_eq!(facts.len(), 3);
    assert_eq!(facts.get(0).unwrap().origin, ActivityOrigin::Playback);
    assert!(matches!(
        facts.get(1).unwrap().fact,
        ActivityFact::DomainTransition { .. }
    ));
    let ActivityFact::EffectAuthorized { intent_id, kind } = facts.get(2).unwrap().fact else {
        panic!("missing effect authorization")
    };
    assert_eq!(kind, ExternalEffectKind::TranscriptProvider);
    assert_eq!(effects[0].intent_id, intent_id);
    assert_eq!(effects[0].authorizing_fact_index, 2);
    assert_eq!(
        effects[0].request.subject,
        ActivitySubject::TranscriptWorkflow { workflow_id }
    );
    assert_eq!(effects[0].request.episode_id, Some(episode_id));
    assert!(commands.is_empty());
}
