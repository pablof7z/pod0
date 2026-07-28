use pod0_domain::{
    AgentCommitId, AgentProposalId, AgentTurnId, ContentDigest, GeneratedArtifactId, StateRevision,
};
use sha2::{Digest as _, Sha256};

use crate::AgentToolAction;
use crate::agent_action_hash_primitives::{
    fields, hash_item_ids, hash_optional_id, hash_optional_text, hash_optional_u64,
    hash_recall_scope, hash_tag, hash_text, hash_tool, queue_code,
};

pub fn agent_proposal_identity(
    turn_id: AgentTurnId,
    revision: StateRevision,
    action: &AgentToolAction,
) -> (AgentProposalId, ContentDigest) {
    let mut hasher = Sha256::new();
    hasher.update(b"pod0:agent-proposal:v1\0");
    hasher.update(turn_id.into_bytes());
    hasher.update(revision.value.to_be_bytes());
    hash_action(&mut hasher, action);
    let digest: [u8; 32] = hasher.finalize().into();
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest[..16]);
    (
        AgentProposalId::from_bytes(id),
        ContentDigest::from_bytes(digest),
    )
}

pub fn agent_commit_id(proposal_id: AgentProposalId, digest: ContentDigest) -> AgentCommitId {
    let mut hasher = Sha256::new();
    hasher.update(b"pod0:agent-commit:v1\0");
    hasher.update(proposal_id.into_bytes());
    hasher.update(digest.into_bytes());
    let bytes: [u8; 32] = hasher.finalize().into();
    let mut id = [0_u8; 16];
    id.copy_from_slice(&bytes[..16]);
    AgentCommitId::from_bytes(id)
}

pub fn agent_generated_artifact_id(
    proposal_id: AgentProposalId,
    digest: ContentDigest,
) -> GeneratedArtifactId {
    let mut hasher = Sha256::new();
    hasher.update(b"pod0:agent-generated-artifact:v1\0");
    hasher.update(proposal_id.into_bytes());
    hasher.update(digest.into_bytes());
    let bytes: [u8; 32] = hasher.finalize().into();
    let mut id = [0_u8; 16];
    id.copy_from_slice(&bytes[..16]);
    GeneratedArtifactId::from_bytes(id)
}

pub fn agent_generated_script_digest(script: &str) -> ContentDigest {
    let mut hasher = Sha256::new();
    hasher.update(b"pod0:agent-generated-script:v1\0");
    hash_text(&mut hasher, script);
    ContentDigest::from_bytes(hasher.finalize().into())
}

fn hash_action(hasher: &mut Sha256, action: &AgentToolAction) {
    use AgentToolAction::*;
    match action {
        NoArguments { tool } => fields(hasher, 1, |h| hash_tool(h, *tool)),
        TextInput { tool, text } => fields(hasher, 2, |h| {
            hash_tool(h, *tool);
            hash_text(h, text);
        }),
        Search {
            tool,
            query,
            scope,
            limit,
        } => fields(hasher, 3, |h| {
            hash_tool(h, *tool);
            hash_text(h, query);
            hash_optional_text(h, scope.as_deref());
            h.update(limit.to_be_bytes());
        }),
        QueryTranscripts {
            query,
            scope,
            limit,
        } => fields(hasher, 23, |h| {
            hash_text(h, query);
            hash_recall_scope(h, *scope);
            h.update(limit.to_be_bytes());
        }),
        Episode { tool, episode_id } => fields(hasher, 4, |h| {
            hash_tool(h, *tool);
            h.update(episode_id.into_bytes());
        }),
        Podcast { tool, podcast_id } => fields(hasher, 5, |h| {
            hash_tool(h, *tool);
            h.update(podcast_id.into_bytes());
        }),
        PlayEpisode {
            episode_id,
            start_milliseconds,
            end_milliseconds,
            placement,
        } => fields(hasher, 6, |h| {
            h.update(episode_id.into_bytes());
            hash_optional_u64(h, *start_milliseconds);
            hash_optional_u64(h, *end_milliseconds);
            hash_tag(h, queue_code(*placement));
        }),
        SetPlaybackRate { permille } => fields(hasher, 7, |h| h.update(permille.to_be_bytes())),
        SetSleepTimer {
            duration_milliseconds,
        } => fields(hasher, 8, |h| hash_optional_u64(h, *duration_milliseconds)),
        CreateNote { text } => fields(hasher, 9, |h| hash_text(h, text)),
        RecordMemory { text } => fields(hasher, 10, |h| hash_text(h, text)),
        Ask { question, context } => fields(hasher, 11, |h| {
            hash_text(h, question);
            hash_optional_text(h, context.as_deref());
        }),
        ScheduleTask { task } => fields(hasher, 12, |h| {
            hash_optional_id(h, task.task_id.map(|id| id.into_bytes()));
            hash_text(h, &task.label);
            hash_text(h, &task.prompt);
            hash_text(h, &task.model_reference);
            h.update(task.interval_milliseconds.to_be_bytes());
            h.update(task.next_run_at.value.to_be_bytes());
        }),
        CancelScheduledTask {
            task_id,
            expected_revision,
        } => fields(hasher, 13, |h| {
            h.update(task_id.into_bytes());
            h.update(expected_revision.value.to_be_bytes());
        }),
        ChangePodcastCategory {
            podcast_id,
            category,
        } => fields(hasher, 14, |h| {
            h.update(podcast_id.into_bytes());
            hash_text(h, category);
        }),
        CreateClip {
            episode_id,
            podcast_id,
            start_milliseconds,
            end_milliseconds,
            caption,
            frozen_transcript_text,
        } => fields(hasher, 15, |h| {
            h.update(episode_id.into_bytes());
            h.update(podcast_id.into_bytes());
            h.update(start_milliseconds.to_be_bytes());
            h.update(end_milliseconds.to_be_bytes());
            hash_optional_text(h, caption.as_deref());
            hash_text(h, frozen_transcript_text);
        }),
        SubscribePodcast { feed_url } => fields(hasher, 16, |h| hash_text(h, feed_url)),
        IngestYoutubeVideo { url } => fields(hasher, 17, |h| hash_text(h, url)),
        ConfigureAgentVoice { voice_id } => fields(hasher, 18, |h| hash_text(h, voice_id)),
        CreatePodcast { title, description } => fields(hasher, 19, |h| {
            hash_text(h, title);
            hash_text(h, description);
        }),
        UpdatePodcast {
            podcast_id,
            title,
            description,
        } => fields(hasher, 20, |h| {
            h.update(podcast_id.into_bytes());
            hash_text(h, title);
            hash_text(h, description);
        }),
        GenerateTtsEpisode {
            podcast_id,
            title,
            script,
            voice_id,
        } => fields(hasher, 21, |h| {
            hash_optional_id(h, podcast_id.map(|id| id.into_bytes()));
            hash_text(h, title);
            hash_text(h, script);
            hash_optional_text(h, voice_id.as_deref());
        }),
        GeneratePodcastArtwork { podcast_id, prompt } => fields(hasher, 22, |h| {
            h.update(podcast_id.into_bytes());
            hash_text(h, prompt);
        }),
        WriteCategory {
            category_id,
            name,
            description,
            color_hex,
            delete,
        } => fields(hasher, 24, |h| {
            hash_optional_id(h, category_id.map(|id| id.into_bytes()));
            hash_optional_text(h, name.as_deref());
            hash_optional_text(h, description.as_deref());
            hash_optional_text(h, color_hex.as_deref());
            hash_tag(h, u32::from(*delete));
        }),
        TagItems {
            category_id,
            add_item_ids,
            remove_item_ids,
        } => fields(hasher, 25, |h| {
            h.update(category_id.into_bytes());
            hash_item_ids(h, add_item_ids);
            hash_item_ids(h, remove_item_ids);
        }),
    }
}
