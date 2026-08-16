use pod0_application::RecallEvidenceProjection;
use pod0_domain::TranscriptSource;
use serde_json::{Value, json};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn agent_recall_result_for_evidence(
        &self,
        recall_evidence: &[RecallEvidenceProjection],
    ) -> String {
        let status = if recall_evidence.is_empty() {
            "no_evidence"
        } else {
            "ready"
        };
        self.agent_recall_result_for_status(status, recall_evidence)
    }

    pub(super) fn agent_recall_result_for_status(
        &self,
        status: &str,
        recall_evidence: &[RecallEvidenceProjection],
    ) -> String {
        let speaker_names =
            self.recall_speaker_names(recall_evidence.iter().map(|item| item.episode_id));
        let evidence = recall_evidence
            .iter()
            .map(|item| {
                let speaker = item
                    .speaker_id
                    .and_then(|id| speaker_names.get(&(item.episode_id, id)));
                let episode = self
                    .listening
                    .episodes
                    .iter()
                    .find(|episode| episode.episode_id == item.episode_id);
                let podcast = self
                    .listening
                    .podcasts
                    .iter()
                    .find(|podcast| podcast.podcast_id == item.podcast_id);
                json!({
                    "episode_id": opaque_id(item.episode_id.into_bytes()),
                    "podcast_id": opaque_id(item.podcast_id.into_bytes()),
                    "episode_title": episode.map(|value| value.title.as_str()),
                    "podcast_title": podcast.map(|value| value.title.as_str()),
                    "start_milliseconds": item.start_milliseconds,
                    "end_milliseconds": item.end_milliseconds,
                    "timestamp": timestamp(item.start_milliseconds),
                    "excerpt": item.excerpt,
                    "speaker_id": item.speaker_id.map(|id| opaque_id(id.into_bytes())),
                    "speaker_label": speaker.map(|(label, _)| label.as_str()),
                    "speaker_display_name": speaker.and_then(|(_, name)| name.as_deref()),
                    "transcript_source": transcript_source(item.provenance.source),
                    "provider": item.provenance.provider,
                    "playable_reference": {
                        "episode_id": opaque_id(item.episode_id.into_bytes()),
                        "start_milliseconds": item.start_milliseconds,
                    },
                })
            })
            .collect::<Vec<Value>>();
        json!({
            "status": status,
            "evidence": evidence,
        })
        .to_string()
    }
}

fn transcript_source(source: TranscriptSource) -> &'static str {
    match source {
        TranscriptSource::Publisher => "publisher",
        TranscriptSource::Scribe => "scribe",
        TranscriptSource::Whisper => "whisper",
        TranscriptSource::OnDevice => "on_device",
        TranscriptSource::AssemblyAi => "assembly_ai",
        TranscriptSource::Other => "other",
        TranscriptSource::Unsupported { .. } => "unsupported",
    }
}

fn timestamp(milliseconds: u64) -> String {
    let seconds = milliseconds / 1_000;
    let hours = seconds / 3_600;
    let minutes = seconds % 3_600 / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn opaque_id(bytes: [u8; 16]) -> String {
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}
