use pod0_application::{
    HostObservation, HostObservationReceipt, HostObservationRejection, LeasedHostObservationEnvelope,
    LibraryDocumentObservation, LibraryNetworkIntent, LibraryNetworkStep, OperationResult,
};
use crate::runtime_library_network_actions::{catalog_directory_action, catalog_feed_action};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn record_leased_library_network_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
    ) -> (bool, HostObservationReceipt) {
        let request_id = leased.observation.request_id;
        let Some(store) = self.store.clone() else {
            return (false, retain(request_id));
        };
        let Ok(Some(effect)) = store.effect_request(leased.lease.intent_id) else {
            return (false, stale(request_id));
        };
        let pod0_application::DurableEffectExecution::LibraryNetwork { request } = effect.execution
        else {
            return (false, mismatched(request_id));
        };
        let Ok(Some(workflow)) = store.library_network_workflow(request.command_id) else {
            return (false, stale(request_id));
        };
        let Some(step) = workflow.pending_step.clone() else {
            return (false, stale(request_id));
        };
        let (document, action) = match &leased.observation.observation {
            HostObservation::LibraryDocumentFetched {
                workflow_command_id,
                step: observed_step,
                bytes,
                response_url,
                mime_type,
                http_status,
            } if *workflow_command_id == workflow.command_id && *observed_step == step => {
                let document = LibraryDocumentObservation {
                    bytes: bytes.clone(),
                    response_url: response_url.clone(),
                    mime_type: mime_type.clone(),
                    http_status: *http_status,
                };
                let Some(action) = successful_action(
                    &workflow.intent,
                    &step,
                    &document,
                    leased.observation.observed_at.value,
                ) else {
                    return (false, mismatched(request_id));
                };
                (document, action)
            }
            HostObservation::Failed { code, .. } => (
                empty_document(),
                pod0_storage::LibraryNetworkObservationAction::Fail {
                    code: format!("{code:?}"),
                },
            ),
            HostObservation::Cancelled => (
                empty_document(),
                pod0_storage::LibraryNetworkObservationAction::Cancel,
            ),
            _ => return (false, mismatched(request_id)),
        };
        let result = store.commit_library_network_observation(
            pod0_storage::LibraryNetworkObservationInput {
                lease: leased.lease,
                command_id: workflow.command_id,
                request_id,
                cancellation_id: leased.observation.cancellation_id,
                observed_request_revision: leased.observation.observed_request_revision,
                sequence_number: leased.observation.sequence_number,
                observation: document,
                action,
                observed_at_ms: leased.observation.observed_at.value,
            },
        );
        match result {
            Ok(record) => {
                self.revision = record.revision;
                match record.stage {
                    pod0_storage::StoredLibraryNetworkStage::Completed => self.succeed(
                        record.command_id,
                        record.result.map(|value| match value {
                            pod0_storage::StoredLibraryNetworkResult::Directory { entries } => {
                                OperationResult::PodcastDirectoryResults { results: entries }
                            }
                            pod0_storage::StoredLibraryNetworkResult::SharedEpisode { episode_id } => {
                                OperationResult::SharedEpisodeImported { episode_id }
                            }
                            pod0_storage::StoredLibraryNetworkResult::Catalog {
                                episode_ids,
                                bounded_result,
                            } => OperationResult::PodcastCatalogResults {
                                episode_ids,
                                bounded_result,
                            },
                        }),
                    ),
                    pod0_storage::StoredLibraryNetworkStage::Failed => self.fail(
                        record.command_id,
                        pod0_application::CoreFailureCode::HostRejected,
                    ),
                    pod0_storage::StoredLibraryNetworkStage::Cancelled => self.fail(
                        record.command_id,
                        pod0_application::CoreFailureCode::Cancelled,
                    ),
                    _ => {}
                }
                (true, applied(request_id, record.revision))
            }
            Err(pod0_storage::StorageError::RevisionConflict) => (false, stale(request_id)),
            Err(_) => (false, retain(request_id)),
        }
    }
}

fn successful_action(
    intent: &LibraryNetworkIntent,
    step: &LibraryNetworkStep,
    document: &LibraryDocumentObservation,
    observed_at_ms: i64,
) -> Option<pod0_storage::LibraryNetworkObservationAction> {
    if !(200..300).contains(&document.http_status) {
        return Some(pod0_storage::LibraryNetworkObservationAction::Fail {
            code: "http".into(),
        });
    }
    match (intent, step) {
        (LibraryNetworkIntent::DirectorySearch { .. }, LibraryNetworkStep::DirectorySearch) => {
            Some(pod0_storage::LibraryNetworkObservationAction::CompleteDirectory {
                results: pod0_application::parse_directory_response(&document.bytes).ok()?,
            })
        }
        (LibraryNetworkIntent::TopPodcasts { .. }, LibraryNetworkStep::TopChart) => {
            let ranked_ids = pod0_application::parse_top_chart_ids(&document.bytes).ok()?;
            let request = pod0_application::plan_directory_lookup(&ranked_ids)?;
            Some(pod0_storage::LibraryNetworkObservationAction::ContinueTopLookup {
                ranked_ids,
                request,
            })
        }
        (
            LibraryNetworkIntent::TopPodcasts { .. },
            LibraryNetworkStep::DirectoryLookup { ranked_ids },
        ) => Some(pod0_storage::LibraryNetworkObservationAction::CompleteTopLookup {
            results: pod0_application::order_directory_results(
                pod0_application::parse_directory_response(&document.bytes).ok()?,
                ranked_ids,
            ),
        }),
        (LibraryNetworkIntent::SharedEpisodeImport { .. }, LibraryNetworkStep::SharedPage) => {
            shared_page_action(document, observed_at_ms)
        }
        (
            LibraryNetworkIntent::SharedEpisodeImport { .. },
            LibraryNetworkStep::SharedAppleLookup { page },
        ) => shared_lookup_action(document, page),
        (
            LibraryNetworkIntent::SharedEpisodeImport { .. },
            LibraryNetworkStep::SharedFeed { page },
        ) => Some(match pod0_application::resolve_shared_episode_from_feed(
            &document.bytes,
            &document.response_url,
            page,
            observed_at_ms,
        ) {
            Ok(episode) => pod0_storage::LibraryNetworkObservationAction::CompleteShared { episode },
            Err(_) => complete_page_or_fail(page, &document.response_url),
        }),
        (
            LibraryNetworkIntent::CatalogEpisodeSearch { .. },
            LibraryNetworkStep::CatalogDirectory,
        ) => catalog_directory_action(document),
        (
            LibraryNetworkIntent::CatalogEpisodeSearch { .. },
            LibraryNetworkStep::CatalogFeed {
                feed_urls,
                ordinal,
                candidates,
            },
        ) => catalog_feed_action(
            intent,
            document,
            feed_urls,
            *ordinal,
            candidates.clone(),
            observed_at_ms,
        ),
        _ => Some(pod0_storage::LibraryNetworkObservationAction::Fail {
            code: "unsupported-step".into(),
        }),
    }
}

fn shared_page_action(
    document: &LibraryDocumentObservation,
    observed_at_ms: i64,
) -> Option<pod0_storage::LibraryNetworkObservationAction> {
    if document
        .mime_type
        .as_deref()
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("audio/"))
    {
        return Some(pod0_storage::LibraryNetworkObservationAction::CompleteShared {
            episode: pod0_application::direct_shared_episode(
                &document.response_url,
                observed_at_ms,
            )?,
        });
    }
    let page = pod0_application::parse_episode_web_page(&document.bytes, &document.response_url)?;
    if let Some(feed_url) = page.feed_url.as_deref()
        && let Some(request) = pod0_application::plan_shared_feed_request(feed_url)
    {
        return Some(pod0_storage::LibraryNetworkObservationAction::ContinueShared {
            step: LibraryNetworkStep::SharedFeed { page },
            request,
        });
    }
    if let Some(identifier) = page.apple_podcast_id.as_deref()
        && let Some(request) = pod0_application::plan_shared_apple_lookup(identifier)
    {
        return Some(pod0_storage::LibraryNetworkObservationAction::ContinueShared {
            step: LibraryNetworkStep::SharedAppleLookup { page },
            request,
        });
    }
    Some(complete_page_or_fail(&page, &document.response_url))
}

fn shared_lookup_action(
    document: &LibraryDocumentObservation,
    page: &pod0_application::EpisodeWebPageMetadata,
) -> Option<pod0_storage::LibraryNetworkObservationAction> {
    let Some(feed_url) = pod0_application::parse_shared_lookup_feed_url(&document.bytes) else {
        return Some(complete_page_or_fail(page, &document.response_url));
    };
    let mut page = page.clone();
    page.feed_url = Some(feed_url.clone());
    Some(pod0_storage::LibraryNetworkObservationAction::ContinueShared {
        step: LibraryNetworkStep::SharedFeed { page },
        request: pod0_application::plan_shared_feed_request(&feed_url)?,
    })
}

fn complete_page_or_fail(
    page: &pod0_application::EpisodeWebPageMetadata,
    response_url: &str,
) -> pod0_storage::LibraryNetworkObservationAction {
    pod0_application::page_direct_episode(page, response_url, 0).map_or_else(
        || pod0_storage::LibraryNetworkObservationAction::Fail {
            code: "no-playable-episode".into(),
        },
        |episode| pod0_storage::LibraryNetworkObservationAction::CompleteShared { episode },
    )
}

fn empty_document() -> LibraryDocumentObservation {
    LibraryDocumentObservation {
        bytes: Vec::new(),
        response_url: String::new(),
        mime_type: None,
        http_status: 0,
    }
}

fn applied(
    request_id: pod0_domain::HostRequestId,
    _revision: pod0_domain::StateRevision,
) -> HostObservationReceipt {
    HostObservationReceipt::Persisted {
        request_id,
        terminal: true,
    }
}

fn retain(request_id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    HostObservationReceipt::RetainAndRetry { request_id }
}

fn stale(request_id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    rejected(request_id, HostObservationRejection::StaleWorkflow)
}

fn mismatched(request_id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    rejected(request_id, HostObservationRejection::MismatchedPayload)
}

fn rejected(
    request_id: pod0_domain::HostRequestId,
    reason: HostObservationRejection,
) -> HostObservationReceipt {
    HostObservationReceipt::Rejected { request_id, reason }
}
