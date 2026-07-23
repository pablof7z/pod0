use pod0_application::{
    CommandEnvelope, OperationResult, Projection, ProjectionRequest, PublicationsProjection,
    compose_generated_episode_publication,
};
use pod0_domain::{PublicationId, PublicationIntent, PublicationStage};
use pod0_nmp::PublicationAdapterEvent;

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
                if record.receipt_id.is_none() && record.stage == PublicationStage::Prepared {
                    self.pending_publications.push_back(draft);
                }
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

    pub(super) fn take_pending_publications(
        &mut self,
    ) -> Vec<pod0_application::Pod0PublicationDraft> {
        self.pending_publications.drain(..).collect()
    }

    pub(super) fn record_publication_receipt(
        &mut self,
        publication_id: PublicationId,
        receipt_id: u64,
    ) -> bool {
        let Some(store) = self.publication_store.as_ref() else {
            return false;
        };
        let before = store
            .publication(publication_id)
            .ok()
            .flatten()
            .map(|record| record.revision);
        let Ok(record) = store.record_receipt(publication_id, receipt_id, self.now()) else {
            return false;
        };
        if before == Some(record.revision) {
            return false;
        }
        self.advance_revision();
        true
    }

    pub(super) fn apply_publication_event(&mut self, event: PublicationAdapterEvent) -> bool {
        let Some(store) = self.publication_store.as_ref() else {
            return false;
        };
        let before = store
            .publication(event.publication_id)
            .ok()
            .flatten()
            .map(|record| record.revision);
        let Ok(record) = store.observe(event.publication_id, &event.observation, self.now()) else {
            return false;
        };
        if before == Some(record.revision) {
            return false;
        }
        self.advance_revision();
        true
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

    pub(super) fn publication_projection(
        &self,
        publication_id: Option<PublicationId>,
        request: &ProjectionRequest,
    ) -> Projection {
        Projection::Publications {
            value: self.publications_projection(publication_id, request.offset, request.max_items),
        }
    }

    pub(super) fn publication_records(&self) -> Vec<pod0_domain::PublicationRecord> {
        self.publication_store
            .as_ref()
            .and_then(|store| store.recoverable_publications().ok())
            .unwrap_or_default()
    }
}
