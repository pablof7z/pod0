use pod0_application::{
    CommandEnvelope, OperationResult, Projection, ProjectionRequest, PublicationsProjection,
    compose_generated_episode_publication,
};
use pod0_domain::{PublicationId, PublicationIntent, PublicationStage};

use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    pub(super) fn pub_nmp(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        intent: &PublicationIntent,
    ) {
        self.prepare_generated_episode_publication(envelope, fingerprint, intent);
    }

    pub(super) fn prepare_generated_episode_publication(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        intent: &PublicationIntent,
    ) {
        let episode = self.listening.episodes.iter().find(|episode| {
            episode
                .generated_audio
                .as_ref()
                .is_some_and(|generated| generated.artifact_id == intent.artifact_id)
        });
        let podcast = episode.and_then(|episode| {
            self.listening
                .podcasts
                .iter()
                .find(|podcast| podcast.podcast_id == episode.podcast_id)
        });
        let result = episode
            .zip(podcast)
            .ok_or(pod0_storage::StorageError::PublicationNotFound)
            .and_then(|(episode, podcast)| {
                self.publication_store
                    .as_ref()
                    .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)?
                    .prepare_generated_episode(
                        envelope.command_id,
                        fingerprint,
                        intent,
                        episode,
                        podcast,
                        self.now(),
                    )
                    .and_then(|outcome| {
                        let record = outcome.record();
                        let draft = compose_generated_episode_publication(record, episode, podcast)
                            .map_err(|_| pod0_storage::StorageError::InvalidPublication)?;
                        Ok((record.clone(), draft))
                    })
            });
        match result {
            Ok((record, draft)) => {
                let _ = draft;
                self.revision =
                    pod0_domain::StateRevision::new(self.revision.value.max(record.revision.value));
                self.succeed(
                    envelope.command_id,
                    Some(OperationResult::PublicationPrepared {
                        publication_id: record.publication_id,
                    }),
                );
            }
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    pub(super) fn publications_projection(
        &self,
        publication_id: Option<PublicationId>,
        offset: u32,
        maximum: u16,
    ) -> PublicationsProjection {
        let mut value = PublicationsProjection {
            items: self
                .publication_store
                .as_ref()
                .and_then(|store| store.page(publication_id, 0, 200).ok())
                .unwrap_or_default(),
            operations: self.operations.clone(),
            has_more: false,
        };
        value.enforce_bounds(
            usize::try_from(offset).unwrap_or(usize::MAX),
            usize::from(maximum.clamp(1, pod0_application::MAX_PROJECTION_ITEMS)),
        );
        value
    }

    pub(super) fn take_pending_publications(
        &mut self,
        maximum: usize,
    ) -> Vec<pod0_application::LeasedNMPPublicationDraft> {
        let Some(store) = self.store.as_ref() else {
            return Vec::new();
        };
        let mut drafts = Vec::with_capacity(maximum);
        while drafts.len() < maximum {
            let Ok(Some(effect)) = store.claim_next_publication_effect(self.now(), 120_000) else {
                break;
            };
            drafts.push(pod0_application::LeasedNMPPublicationDraft {
                lease: effect.lease,
                draft: effect.draft,
            });
        }
        drafts
    }

    pub(super) fn rehydrate_publications(&mut self) -> Result<(), pod0_storage::StorageError> {
        let records = self
            .publication_store
            .as_ref()
            .ok_or(pod0_storage::StorageError::CutoverNotAuthoritative)?
            .recoverable_publications()?;
        for record in records {
            if record.receipt_id.is_some() || record.stage != PublicationStage::Prepared {
                continue;
            }
            let Some(episode) = self
                .listening
                .episodes
                .iter()
                .find(|episode| episode.episode_id == record.episode_id)
            else {
                continue;
            };
            let Some(podcast) = self
                .listening
                .podcasts
                .iter()
                .find(|podcast| podcast.podcast_id == record.podcast_id)
            else {
                continue;
            };
            compose_generated_episode_publication(&record, episode, podcast)
                .map_err(|_| pod0_storage::StorageError::InvalidPublication)?;
        }
        Ok(())
    }

    pub(super) fn publication_receipt_links(
        &self,
    ) -> Vec<pod0_application::NMPPublicationReceiptLink> {
        self.publication_store
            .as_ref()
            .and_then(|store| store.recoverable_publications().ok())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|record| {
                record.receipt_id.and_then(|receipt_id| {
                    let lease = self
                        .store
                        .as_ref()?
                        .active_publication_lease(record.publication_id)
                        .ok()??;
                    Some(pod0_application::NMPPublicationReceiptLink {
                        publication_id: record.publication_id,
                        receipt_id,
                        lease,
                    })
                })
            })
            .collect()
    }

    pub(super) fn record_publication_receipt(
        &mut self,
        input: pod0_application::LeasedNMPPublicationReceipt,
    ) -> bool {
        let Some(store) = self.publication_store.as_ref() else {
            return false;
        };
        let Ok(record) = store.record_leased_receipt(input, self.now()) else {
            return false;
        };
        self.revision =
            pod0_domain::StateRevision::new(self.revision.value.max(record.revision.value));
        true
    }

    pub(super) fn record_publication_observation(
        &mut self,
        input: pod0_application::LeasedNMPPublicationObservation,
    ) -> bool {
        let Some(store) = self.publication_store.as_ref() else {
            return false;
        };
        let Ok(record) = store.observe_leased(input, self.now()) else {
            return false;
        };
        self.revision =
            pod0_domain::StateRevision::new(self.revision.value.max(record.revision.value));
        true
    }

    pub(super) fn publication_projection(
        &self,
        publication_id: Option<PublicationId>,
        request: &ProjectionRequest,
    ) -> Projection {
        Projection::Publications {
            value: self.publications_projection(publication_id, request.offset, request.max_items),
        }
    }
}
