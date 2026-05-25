//! Action types for the transcript ingestion pipeline.
//!
//! These are pure data envelopes — no executor logic lives here. The
//! kernel + STT capability adapters land in M5; this file just pins the
//! action shapes that `nmp-app-podcast` will dispatch.

use serde::{Deserialize, Serialize};

use crate::types::TranscriptKind;
use podcast_core::SttProvider;

/// Kick off an ingest for one episode. The router resolves the provider
/// chain (publisher → on-device → BYOK fallback) using episode metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestTranscript {
    pub episode_id: String,
    /// Optional publisher transcript URL. When present, the router tries
    /// the publisher path first; when absent, it jumps straight to the
    /// configured STT provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher_url: Option<String>,
    /// MIME hint for the publisher transcript (when known). The router
    /// uses this to pick the parser without sniffing bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher_kind: Option<TranscriptKind>,
}

/// Retry a previously-failed ingest. Resets the per-episode state to
/// `Queued` and re-enters the router.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryTranscript {
    pub episode_id: String,
}

/// Force a specific STT provider for one episode, bypassing the publisher
/// fetch and the global auto-fallback gate. Used by the Diagnostics
/// "Retry with…" menu so the user can try an alternative provider for one
/// call without flipping their global setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverrideProvider {
    pub episode_id: String,
    pub provider: SttProvider,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_action_round_trip() {
        let action = IngestTranscript {
            episode_id: "ep-1".into(),
            publisher_url: Some("https://example.com/t.vtt".into()),
            publisher_kind: Some(TranscriptKind::Vtt),
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: IngestTranscript = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn retry_action_round_trip() {
        let action = RetryTranscript {
            episode_id: "ep-1".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: RetryTranscript = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }

    #[test]
    fn override_action_round_trip() {
        let action = OverrideProvider {
            episode_id: "ep-1".into(),
            provider: SttProvider::AssemblyAi,
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: OverrideProvider = serde_json::from_str(&json).unwrap();
        assert_eq!(action, back);
    }
}
