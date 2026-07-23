use pod0_domain::{AgentCommitId, AgentProposalId, AgentTurnId, ContentDigest, StateRevision};
use sha2::{Digest as _, Sha256};

use crate::{ALL_AGENT_TOOL_NAMES, AgentToolAction, AgentToolName, QueuePlacement};

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
    }
}

fn fields(hasher: &mut Sha256, tag: u32, write: impl FnOnce(&mut Sha256)) {
    hash_tag(hasher, tag);
    write(hasher);
}

fn queue_code(value: QueuePlacement) -> u32 {
    match value {
        QueuePlacement::Back => 1,
        QueuePlacement::Next => 2,
        QueuePlacement::Unsupported { wire_code } => wire_code,
    }
}

fn hash_tag(hasher: &mut Sha256, value: u32) {
    hasher.update(value.to_be_bytes());
}

fn hash_tool(hasher: &mut Sha256, tool: AgentToolName) {
    let name = ALL_AGENT_TOOL_NAMES
        .iter()
        .find_map(|(name, candidate)| (*candidate == tool).then_some(*name))
        .expect("every typed tool has a stable wire name");
    hash_text(hasher, name);
}

fn hash_text(hasher: &mut Sha256, value: &str) {
    hasher.update(
        u64::try_from(value.len())
            .expect("bounded action text length fits u64")
            .to_be_bytes(),
    );
    hasher.update(value.as_bytes());
}

fn hash_optional_text(hasher: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hash_text(hasher, value);
        }
        None => hasher.update([0]),
    }
}

fn hash_optional_u64(hasher: &mut Sha256, value: Option<u64>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value.to_be_bytes());
        }
        None => hasher.update([0]),
    }
}

fn hash_optional_id(hasher: &mut Sha256, value: Option<[u8; 16]>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value);
        }
        None => hasher.update([0]),
    }
}
