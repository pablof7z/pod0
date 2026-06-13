//! Persistence helpers for downloaded episode side-maps.

use std::collections::BTreeMap;

use podcast_core::{DownloadState, EpisodeId};

use super::PodcastStore;

impl PodcastStore {
    pub(super) fn hydrate_download_maps(
        &mut self,
        local_paths: Vec<(String, String)>,
        file_sizes: Vec<(String, i64)>,
    ) {
        self.local_paths.clear();
        self.file_sizes.clear();

        for (id, path) in local_paths {
            if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
                self.local_paths.insert(EpisodeId(uuid), path);
            }
        }

        for episodes in self.episodes.values() {
            for ep in episodes {
                if self.local_paths.contains_key(&ep.id) {
                    continue;
                }
                if let DownloadState::Downloaded {
                    local_file_url,
                    byte_count,
                } = &ep.download_state
                {
                    if let Ok(path) = local_file_url.to_file_path() {
                        self.local_paths
                            .insert(ep.id, path.to_string_lossy().into_owned());
                        self.file_sizes.insert(ep.id, *byte_count);
                    }
                }
            }
        }

        for (id, size) in file_sizes {
            if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
                let episode_id = EpisodeId(uuid);
                if self.local_paths.contains_key(&episode_id) {
                    self.file_sizes.insert(episode_id, size);
                }
            }
        }
    }

    pub(super) fn persisted_local_paths(&self) -> Vec<(String, String)> {
        self.local_paths
            .iter()
            .map(|(id, path)| (id.0.to_string(), path.clone()))
            .collect::<BTreeMap<_, _>>()
            .into_iter()
            .collect()
    }

    pub(super) fn persisted_file_sizes(&self) -> Vec<(String, i64)> {
        self.file_sizes
            .iter()
            .filter(|(id, _)| self.local_paths.contains_key(id))
            .map(|(id, size)| (id.0.to_string(), *size))
            .collect::<BTreeMap<_, _>>()
            .into_iter()
            .collect()
    }
}
