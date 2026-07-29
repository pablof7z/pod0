use std::collections::{BTreeMap, BTreeSet};

use pod0_domain::{EpisodeId, SpeakerId};
use pod0_storage::MAX_TRANSCRIPT_PROJECTION_ITEMS;

use crate::runtime_state::FacadeState;

/// Diarization labels and resolved display names for evidence speakers,
/// keyed by `(episode, speaker)`.
///
/// Values come from the same `selected_speakers` storage read the transcript
/// projection uses (issue #190), so the agent's `query_transcripts` result
/// cannot diverge from what the user reading the transcript sees, including
/// after a speaker entity rename. A missing store or unreadable selection
/// leaves slots empty rather than failing the whole tool result.
pub(crate) type SpeakerNameIndex = BTreeMap<(EpisodeId, SpeakerId), (String, Option<String>)>;

impl FacadeState {
    pub(crate) fn recall_speaker_names(
        &self,
        episodes: impl Iterator<Item = EpisodeId>,
    ) -> SpeakerNameIndex {
        let mut names = SpeakerNameIndex::new();
        let Some(store) = &self.transcript_store else {
            return names;
        };
        let mut seen = BTreeSet::new();
        for episode_id in episodes {
            if !seen.insert(episode_id) {
                continue;
            }
            let mut offset = 0_u32;
            loop {
                let Ok(page) =
                    store.selected_speakers(episode_id, offset, MAX_TRANSCRIPT_PROJECTION_ITEMS)
                else {
                    break;
                };
                let Ok(count) = u32::try_from(page.items.len()) else {
                    break;
                };
                for speaker in page.items {
                    names.insert(
                        (episode_id, speaker.speaker_id),
                        (speaker.label, speaker.display_name),
                    );
                }
                if !page.has_more || count == 0 {
                    break;
                }
                offset = offset.saturating_add(count);
            }
        }
        names
    }
}
