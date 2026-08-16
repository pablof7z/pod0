use std::path::PathBuf;
use std::sync::Arc;

use pod0_domain::CommandId;
use pod0_storage::{UserDataTarget, UserDataTargetKind, ValidatedUserDataInventory};
use sha2::{Digest as _, Sha256};

use crate::Pod0Facade;
use crate::runtime_state_defaults::{default_recall_index, empty_listening_snapshot};
pub use crate::user_data_erasure_facade_types::*;

#[uniffi::export]
pub fn recover_pending_erasure(
    locations: UserDataErasureLocations,
) -> Result<Option<UserDataErasureResult>, UserDataErasureError> {
    let recovery_root = PathBuf::from(&locations.recovery_root);
    let markers = pod0_storage::pending_user_data_erasure_markers(&recovery_root)
        .map_err(|_| UserDataErasureError::RecoveryRequired)?;
    if markers.len() > 1 {
        return Err(UserDataErasureError::RecoveryRequired);
    }
    let Some(marker) = markers.first() else {
        return Ok(None);
    };
    let inventory = inventory(&locations)?;
    pod0_storage::recover_user_data_erasure(marker, &recovery_root, &inventory)
        .map(project_progress)
        .map(Some)
        .map_err(|_| UserDataErasureError::RecoveryRequired)
}

#[uniffi::export]
pub fn record_native_erasure_observation(
    locations: UserDataErasureLocations,
    action_id: CommandId,
    observed_attempt: u16,
    succeeded: bool,
) -> Result<UserDataErasureResult, UserDataErasureError> {
    let recovery_root = PathBuf::from(&locations.recovery_root);
    let markers = pod0_storage::pending_user_data_erasure_markers(&recovery_root)
        .map_err(|_| UserDataErasureError::RecoveryRequired)?;
    let [marker] = markers.as_slice() else {
        return Err(UserDataErasureError::RecoveryRequired);
    };
    let inventory = inventory(&locations)?;
    pod0_storage::observe_native_user_data_erasure(
        marker,
        &recovery_root,
        &inventory,
        action_id,
        observed_attempt,
        succeeded,
    )
    .map(project_progress)
    .map_err(|_| UserDataErasureError::RecoveryRequired)
}

#[uniffi::export]
impl Pod0Facade {
    pub fn store_identity(&self) -> Result<CommandId, UserDataErasureError> {
        let state = self.state();
        if state.erasure_lifecycle != ErasureLifecycle::Active {
            return Err(UserDataErasureError::Conflict);
        }
        state
            .store
            .as_ref()
            .ok_or(UserDataErasureError::Conflict)?
            .store_identity()
            .map_err(|_| UserDataErasureError::Conflict)
    }

    pub fn prepare_erasure(
        &self,
        expected_store_id: CommandId,
        nonce: Vec<u8>,
        retained_settings_json: Vec<u8>,
        locations: UserDataErasureLocations,
    ) -> Result<Arc<UserDataErasureToken>, UserDataErasureError> {
        if !(16..=64).contains(&nonce.len()) {
            return Err(UserDataErasureError::Conflict);
        }
        let mut state = self.state();
        if state.erasure_lifecycle != ErasureLifecycle::Active {
            return Err(UserDataErasureError::Conflict);
        }
        let core = state
            .core_store_path
            .as_ref()
            .ok_or(UserDataErasureError::Conflict)?;
        let inventory = inventory(&locations)?;
        if inventory
            .targets()
            .iter()
            .find(|target| target.kind == UserDataTargetKind::CoreSqlite)
            .and_then(UserDataTarget::path)
            != Some(core)
        {
            return Err(UserDataErasureError::Conflict);
        }
        if state
            .store
            .as_ref()
            .ok_or(UserDataErasureError::Conflict)?
            .store_identity()
            .map_err(|_| UserDataErasureError::Conflict)?
            != expected_store_id
        {
            return Err(UserDataErasureError::Conflict);
        }
        let (operation_id, fresh_store_id) = derived_ids(expected_store_id, &nonce);
        state.erasure_lifecycle = ErasureLifecycle::Erasing;
        state.fence_for_erasure();
        let confirmation = pod0_storage::prepare_user_data_erasure(
            inventory,
            PathBuf::from(&locations.recovery_root).as_path(),
            expected_store_id,
            fresh_store_id,
            operation_id,
            retained_settings_json,
        )
        .map_err(|_| {
            state.erasure_lifecycle = ErasureLifecycle::RecoveryRequired;
            UserDataErasureError::RecoveryRequired
        })?;
        let token = Arc::new(UserDataErasureToken { operation_id });
        state.erasure_lifecycle = ErasureLifecycle::Prepared;
        state.prepared_erasure = Some(PreparedFacadeErasure {
            token: Arc::clone(&token),
            confirmation,
        });
        Ok(token)
    }

    pub fn confirm_erasure(
        &self,
        token: Arc<UserDataErasureToken>,
    ) -> Result<UserDataErasureResult, UserDataErasureError> {
        let mut state = self.state();
        if state.erasure_lifecycle != ErasureLifecycle::Prepared {
            return Err(UserDataErasureError::Conflict);
        }
        let prepared = state
            .prepared_erasure
            .take()
            .ok_or(UserDataErasureError::Conflict)?;
        if !Arc::ptr_eq(&token, &prepared.token)
            || token.operation_id != prepared.token.operation_id
        {
            state.prepared_erasure = Some(prepared);
            return Err(UserDataErasureError::Conflict);
        }
        state.erasure_lifecycle = ErasureLifecycle::Erasing;
        match pod0_storage::confirm_user_data_erasure(prepared.confirmation) {
            Ok(progress) => {
                state.erasure_lifecycle = match progress {
                    pod0_storage::UserDataErasureProgress::Complete(_) => ErasureLifecycle::Erased,
                    pod0_storage::UserDataErasureProgress::AwaitingNativeActions(_) => {
                        ErasureLifecycle::Erasing
                    }
                };
                Ok(project_progress(progress))
            }
            Err(_) => {
                state.erasure_lifecycle = ErasureLifecycle::RecoveryRequired;
                Err(UserDataErasureError::RecoveryRequired)
            }
        }
    }
}

impl crate::runtime_state::FacadeState {
    pub(super) fn fence_for_erasure(&mut self) {
        self.store.take();
        self.evidence_store.take();
        self.transcript_store.take();
        self.scheduled_agent_store.take();
        self.agent_store.take();
        self.publication_store.take();
        self.recall_index = default_recall_index();
        self.listening = empty_listening_snapshot();
        self.notes.notes.clear();
        self.memories.memories.clear();
        self.memories.compiled = None;
        self.clips.clips.clear();
        self.host_requests = Default::default();
        self.pending_transcripts.clear();
        self.recalls.clear();
    }
}

fn inventory(
    locations: &UserDataErasureLocations,
) -> Result<ValidatedUserDataInventory, UserDataErasureError> {
    let targets = locations
        .targets
        .iter()
        .map(|target| {
            let kind: UserDataTargetKind = target.kind.into();
            if let Some(covered_by) = target.covered_by {
                return if target.location.is_empty() {
                    UserDataTarget::covered_by(kind, covered_by.into())
                } else {
                    UserDataTarget::covered_by(kind, kind)
                };
            }
            kind.native_action_identifier().map_or_else(
                || UserDataTarget::filesystem(kind, PathBuf::from(&target.location)),
                |identifier| {
                    if target.location != identifier {
                        return UserDataTarget::native(kind, "invalid-native-identifier");
                    }
                    UserDataTarget::native(kind, identifier)
                },
            )
        })
        .collect();
    let roots = locations
        .allowed_roots
        .iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    ValidatedUserDataInventory::validate(targets, &roots)
        .map_err(|_| UserDataErasureError::Conflict)
}

fn project_progress(progress: pod0_storage::UserDataErasureProgress) -> UserDataErasureResult {
    match progress {
        pod0_storage::UserDataErasureProgress::Complete(fresh_store_id) => {
            UserDataErasureResult::Complete { fresh_store_id }
        }
        pod0_storage::UserDataErasureProgress::AwaitingNativeActions(actions) => {
            UserDataErasureResult::AwaitingNativeActions {
                actions: actions
                    .into_iter()
                    .map(|action| NativeErasureAction {
                        action_id: action.action_id,
                        operation_id: action.operation_id,
                        kind: storage_kind(action.kind),
                        identifier: action.identifier,
                        attempt: action.attempt,
                    })
                    .collect(),
            }
        }
    }
}

fn storage_kind(kind: UserDataTargetKind) -> UserDataErasureTargetKind {
    kind.into()
}

fn derived_ids(expected: CommandId, nonce: &[u8]) -> (CommandId, CommandId) {
    (
        derived_id(b"pod0-erasure-operation-v1", expected, nonce),
        derived_id(b"pod0-erasure-fresh-store-v1", expected, nonce),
    )
}

fn derived_id(domain: &[u8], expected: CommandId, nonce: &[u8]) -> CommandId {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update(expected.into_bytes());
    hash.update(nonce);
    CommandId::from_bytes(hash.finalize()[..16].try_into().expect("digest prefix"))
}

#[cfg(test)]
#[path = "user_data_erasure_facade_tests.rs"]
mod tests;
