use crate::listening_import_test_support::{
    EPISODE_ID, ImportFixture, create_sqlite_source, current_metadata, episode,
};
use crate::{
    AgentCommandContext, AgentGeneratedAudioCommitInput, AgentMutationOutcome, AgentStore,
    LibraryStore, StorageError, commit_listening_cutover,
};
use pod0_application::{
    AgentActionObservation, AgentActionOutcome, AgentAuthorizationObservation,
    AgentModelObservation, AgentToolAction, AgentToolName, AgentTurnStart, AgentTurnState,
    AgentWorkflowAcceptance, agent_generated_artifact_id, agent_generated_episode_id,
    agent_generated_script_digest, default_agent_generated_podcast_id,
};
use pod0_domain::{
    AgentAuthorizationId, AgentExecutionFenceId, AgentTurnId, CancellationId, CommandId,
    ContentDigest, ConversationId, GeneratedArtifactId, GeneratedAudioArtifactProvenance,
    PodcastId, UnixTimestampMilliseconds,
};

struct Fixture {
    _import: ImportFixture,
    agent: AgentStore,
    library: LibraryStore,
}

impl Fixture {
    fn new() -> Self {
        let import = ImportFixture::new();
        create_sqlite_source(
            &import.source,
            &current_metadata(7),
            &[episode(EPISODE_ID, "guid-1")],
        );
        import.stage(&import.plan()).unwrap();
        let path = import.target.clone();
        commit_listening_cutover(&path, 1_001).unwrap();
        Self {
            agent: AgentStore::open(&path).unwrap(),
            library: LibraryStore::open_authoritative(&path).unwrap(),
            _import: import,
        }
    }
}

fn command(seed: u8) -> CommandId {
    CommandId::from_bytes([seed; 16])
}

fn context(seed: u8, observed_at: i64) -> AgentCommandContext {
    AgentCommandContext {
        command_id: command(seed),
        command_fingerprint: [seed; 32],
        observed_at: UnixTimestampMilliseconds::new(observed_at),
    }
}

fn executing_state(podcast_id: Option<PodcastId>) -> (AgentTurnState, &'static str) {
    let script = "Today: one calm, useful idea.";
    let mut state = AgentTurnState::start(AgentTurnStart {
        conversation_id: ConversationId::from_parts(1, 2),
        turn_id: AgentTurnId::from_parts(3, 4),
        model_fence_id: AgentExecutionFenceId::from_parts(5, 6),
        user_input: "Make that a briefing".into(),
        model_reference: "openrouter/test".into(),
        available_tools: vec![AgentToolName::GenerateTtsEpisode],
        cancellation_id: CancellationId::from_parts(7, 8),
        observed_at: UnixTimestampMilliseconds::new(10),
    })
    .unwrap();
    assert_eq!(
        state.observe_model(AgentModelObservation {
            turn_id: state.projection().turn_id,
            model_fence_id: AgentExecutionFenceId::from_parts(5, 6),
            assistant_text: "I can make that briefing.".into(),
            proposed_action: Some(AgentToolAction::GenerateTtsEpisode {
                podcast_id,
                title: "Daily Briefing".into(),
                script: script.into(),
                voice_id: Some("calm".into()),
            }),
            usage: None,
            observed_at: UnixTimestampMilliseconds::new(20),
        }),
        AgentWorkflowAcceptance::Updated
    );
    let proposal = state.projection().proposal.unwrap();
    assert_eq!(
        state.authorize(AgentAuthorizationObservation {
            proposal_id: proposal.proposal_id,
            proposal_digest: proposal.proposal_digest,
            authority: proposal.required_authority,
            authorization_id: AgentAuthorizationId::from_parts(9, 10),
            approved: true,
            observed_at: UnixTimestampMilliseconds::new(30),
        }),
        AgentWorkflowAcceptance::Updated
    );
    assert_eq!(
        state.begin_execution(
            AgentExecutionFenceId::from_parts(11, 12),
            UnixTimestampMilliseconds::new(40),
        ),
        AgentWorkflowAcceptance::Updated
    );
    (state, script)
}

fn committed_state(mut state: AgentTurnState, artifact_id: GeneratedArtifactId) -> AgentTurnState {
    let projection = state.projection();
    assert_eq!(
        state.observe_action(AgentActionObservation {
            proposal_id: projection.proposal.unwrap().proposal_id,
            execution_fence_id: projection.execution_fence_id.unwrap(),
            outcome: AgentActionOutcome::Succeeded {
                bounded_result: r#"{"generated_episode":true}"#.into(),
                artifact_id: Some(artifact_id),
                recall_evidence: Vec::new(),
            },
            observed_at: UnixTimestampMilliseconds::new(50),
        }),
        AgentWorkflowAcceptance::Updated
    );
    assert_eq!(
        state.continue_after_commit(
            AgentExecutionFenceId::from_parts(13, 14),
            UnixTimestampMilliseconds::new(50),
        ),
        AgentWorkflowAcceptance::Updated
    );
    state
}

fn input(
    state: &AgentTurnState,
    script: &str,
    podcast_id: PodcastId,
    artifact_id: GeneratedArtifactId,
) -> AgentGeneratedAudioCommitInput {
    let url = format!("file:///private/agent/{}.mp3", artifact_id.into_bytes()[0]);
    let projection = state.projection();
    let proposal = projection.proposal.unwrap();
    let commit = projection.commit.unwrap();
    AgentGeneratedAudioCommitInput {
        podcast_id,
        episode_id: agent_generated_episode_id(podcast_id, &url),
        title: "Daily Briefing".into(),
        audio_url: url,
        media_type: "audio/mpeg".into(),
        duration_milliseconds: Some(12_000),
        provenance: GeneratedAudioArtifactProvenance {
            artifact_id,
            conversation_id: projection.conversation_id,
            turn_id: projection.turn_id,
            proposal_id: proposal.proposal_id,
            commit_id: commit.commit_id,
            media_content_digest: ContentDigest::from_bytes([21; 32]),
            script_content_digest: agent_generated_script_digest(script),
            media_byte_count: 2_048,
            voice_id: Some("calm".into()),
            model_reference: "openrouter/test".into(),
            committed_at: commit.committed_at,
        },
    }
}

#[test]
fn generated_audio_turn_episode_and_provenance_commit_atomically_and_replay() {
    let fixture = Fixture::new();
    let (initial, script) = executing_state(None);
    fixture.agent.start_turn(context(2, 10), &initial).unwrap();
    let expected = initial.projection().revision;
    let proposal = initial.projection().proposal.unwrap();
    let artifact_id = agent_generated_artifact_id(proposal.proposal_id, proposal.proposal_digest);
    let final_state = committed_state(initial, artifact_id);
    let input = input(
        &final_state,
        script,
        default_agent_generated_podcast_id(),
        artifact_id,
    );

    assert!(matches!(
        fixture
            .agent
            .commit_generated_audio(context(3, 50), expected, &final_state, &input)
            .unwrap(),
        AgentMutationOutcome::Applied(_)
    ));
    assert!(matches!(
        fixture
            .agent
            .commit_generated_audio(context(3, 50), expected, &final_state, &input)
            .unwrap(),
        AgentMutationOutcome::Duplicate(_)
    ));

    let reopened_agent = AgentStore::open(&fixture._import.target).unwrap();
    assert_eq!(
        reopened_agent
            .turn(final_state.projection().turn_id)
            .unwrap()
            .unwrap(),
        final_state
    );
    let snapshot = LibraryStore::open_authoritative(&fixture._import.target)
        .unwrap()
        .snapshot()
        .unwrap();
    let episode = snapshot
        .episodes
        .iter()
        .find(|episode| episode.episode_id == input.episode_id)
        .unwrap();
    assert_eq!(episode.generated_audio, Some(input.provenance.clone()));

    let connection = rusqlite::Connection::open(&fixture._import.target).unwrap();
    let turn_bytes = final_state.projection().turn_id.into_bytes();
    connection
        .execute(
            "DELETE FROM pod0_agent_command_receipts WHERE turn_id=?1",
            [turn_bytes.as_slice()],
        )
        .unwrap();
    connection
        .execute(
            "DELETE FROM pod0_agent_audit WHERE turn_id=?1",
            [turn_bytes.as_slice()],
        )
        .unwrap();
    connection
        .execute(
            "DELETE FROM pod0_agent_turns WHERE turn_id=?1",
            [turn_bytes.as_slice()],
        )
        .unwrap();
    let retained = fixture.library.snapshot().unwrap();
    assert_eq!(
        retained
            .episodes
            .iter()
            .find(|episode| episode.episode_id == input.episode_id)
            .unwrap()
            .generated_audio,
        Some(input.provenance)
    );
}

#[test]
fn missing_explicit_podcast_rolls_back_episode_artifact_and_turn() {
    let fixture = Fixture::new();
    let missing_podcast = PodcastId::from_parts(20, 21);
    let (initial, script) = executing_state(Some(missing_podcast));
    fixture.agent.start_turn(context(4, 10), &initial).unwrap();
    let expected = initial.projection().revision;
    let proposal = initial.projection().proposal.unwrap();
    let artifact_id = agent_generated_artifact_id(proposal.proposal_id, proposal.proposal_digest);
    let final_state = committed_state(initial.clone(), artifact_id);
    let input = input(&final_state, script, missing_podcast, artifact_id);

    assert_eq!(
        fixture
            .agent
            .commit_generated_audio(context(5, 50), expected, &final_state, &input)
            .unwrap_err(),
        StorageError::AgentTurnConflict
    );
    assert_eq!(
        fixture
            .agent
            .turn(initial.projection().turn_id)
            .unwrap()
            .unwrap(),
        initial
    );
    assert!(
        fixture
            .library
            .snapshot()
            .unwrap()
            .episodes
            .iter()
            .all(|episode| episode.episode_id != input.episode_id)
    );
}
