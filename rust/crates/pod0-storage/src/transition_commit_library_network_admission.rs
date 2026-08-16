use pod0_application::{
    DurableLibraryNetworkEffectRequest, LibraryHttpRequest, LibraryNetworkIntent,
    LibraryNetworkStep, plan_directory_search, plan_library_network_admission, plan_top_chart,
};
use pod0_domain::{HostRequestId, StateRevision, UnixTimestampMilliseconds};
use rusqlite::params;
use sha2::{Digest, Sha256};

use crate::{
    LibraryNetworkAdmissionInput, StorageError, TransitionIngress, TransitionIngressKind,
    library_network_store::serialize, transition_commit::TransitionCommit,
};

pub(crate) fn commit(
    path: &std::path::Path,
    input: LibraryNetworkAdmissionInput,
) -> Result<crate::CommitReceipt, StorageError> {
    let effect_input = input.clone();
    TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: input.command_id.into_bytes(),
            fingerprint: input.fingerprint,
        },
        UnixTimestampMilliseconds::new(input.now_ms.max(0)),
        |transaction| {
            let current = current_revision(transaction)?;
            let request = initial_request(&effect_input, next_revision(current)?)?;
            plan_library_network_admission(input.command_id, current, None, request)
                .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, mutation| match mutation {
            pod0_application::LibraryNetworkMutation::Apply => {
                require_revision(transaction, expected)?;
                let committed = next_revision(expected)?;
                let request = initial_request(&input, committed)?;
                transaction
                    .execute(
                        "INSERT INTO pod0_library_network_workflows(command_id,cancellation_id,\
                         command_fingerprint,intent_json,stage,revision,pending_request_id,\
                         pending_step_json,result_json,failure_code,created_at_ms,updated_at_ms) \
                         VALUES(?1,?2,?3,?4,'requested',?5,?6,?7,NULL,NULL,?8,?8)",
                        params![
                            input.command_id.into_bytes().as_slice(),
                            input.cancellation_id.into_bytes().as_slice(),
                            input.command_fingerprint,
                            serialize(&input.intent)?,
                            i64::try_from(committed.value)
                                .map_err(|_| StorageError::InvalidActivity)?,
                            request.request_id.into_bytes().as_slice(),
                            serialize(&request.step)?,
                            input.now_ms
                        ],
                    )
                    .map_err(|error| {
                        StorageError::sqlite("insert library network workflow", error)
                    })?;
                set_revision(transaction, committed)?;
                Ok(committed)
            }
            pod0_application::LibraryNetworkMutation::Duplicate { committed_revision } => {
                Ok(committed_revision)
            }
        },
    )
}

fn initial_request(
    input: &LibraryNetworkAdmissionInput,
    revision: StateRevision,
) -> Result<DurableLibraryNetworkEffectRequest, StorageError> {
    let (step, http) = match &input.intent {
        LibraryNetworkIntent::DirectorySearch { query, limit } => (
            LibraryNetworkStep::DirectorySearch,
            plan_directory_search(query, *limit).ok_or(StorageError::InvalidActivity)?,
        ),
        LibraryNetworkIntent::TopPodcasts { storefront, limit } => (
            LibraryNetworkStep::TopChart,
            plan_top_chart(storefront, *limit).ok_or(StorageError::InvalidActivity)?,
        ),
        LibraryNetworkIntent::SharedEpisodeImport { source_url } => (
            LibraryNetworkStep::SharedPage,
            shared_page_request(source_url)?,
        ),
        LibraryNetworkIntent::CatalogEpisodeSearch {
            episode_query,
            podcast_hint,
            ..
        } => (
            LibraryNetworkStep::CatalogDirectory,
            plan_directory_search(podcast_hint.as_deref().unwrap_or(episode_query), 8)
                .ok_or(StorageError::InvalidActivity)?,
        ),
    };
    Ok(DurableLibraryNetworkEffectRequest {
        request_id: request_id(input.command_id, &step),
        command_id: input.command_id,
        cancellation_id: input.cancellation_id,
        issued_revision: revision,
        deadline_at: Some(UnixTimestampMilliseconds::new(input.deadline_at_ms)),
        step,
        http,
    })
}

fn shared_page_request(source_url: &str) -> Result<LibraryHttpRequest, StorageError> {
    let url = url::Url::parse(source_url).map_err(|_| StorageError::InvalidActivity)?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(StorageError::InvalidActivity);
    }
    Ok(LibraryHttpRequest {
        url: url.into(),
        accept: "text/html, application/xhtml+xml, application/rss+xml;q=0.9, application/xml;q=0.8, audio/*;q=0.7".into(),
        maximum_response_bytes: 5_000_000,
    })
}

pub(crate) fn request_id(
    command_id: pod0_domain::CommandId,
    step: &LibraryNetworkStep,
) -> HostRequestId {
    let mut hash = Sha256::new();
    hash.update(b"pod0/library-network-request/v1\0");
    hash.update(command_id.into_bytes());
    hash.update(serde_json::to_vec(step).expect("serializable step"));
    let digest: [u8; 32] = hash.finalize().into();
    HostRequestId::from_bytes(digest[..16].try_into().expect("fixed digest"))
}

fn current_revision(
    transaction: &rusqlite::Transaction<'_>,
) -> Result<StateRevision, StorageError> {
    let value: i64 = transaction
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read library revision", error))?;
    Ok(StateRevision::new(
        u64::try_from(value).map_err(|_| StorageError::InvalidActivity)?,
    ))
}

fn next_revision(value: StateRevision) -> Result<StateRevision, StorageError> {
    value
        .value
        .checked_add(1)
        .map(StateRevision::new)
        .ok_or(StorageError::InvalidActivity)
}

fn require_revision(
    transaction: &rusqlite::Transaction<'_>,
    expected: StateRevision,
) -> Result<(), StorageError> {
    (current_revision(transaction)? == expected)
        .then_some(())
        .ok_or(StorageError::RevisionConflict)
}

fn set_revision(
    transaction: &rusqlite::Transaction<'_>,
    revision: StateRevision,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "UPDATE pod0_playback_state SET state_revision=?1 WHERE singleton=1",
            [i64::try_from(revision.value).map_err(|_| StorageError::InvalidActivity)?],
        )
        .map_err(|error| StorageError::sqlite("advance library revision", error))?;
    Ok(())
}
