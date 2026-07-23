use std::path::{Path, PathBuf};

use pod0_domain::{
    SignerAccountId, SignerAccountRecord, SignerCredentialKind, SignerStage, StateRevision,
    UnixTimestampMilliseconds,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::migration_db::{configure, open_connection, user_version, validate_open_database};
use crate::{CURRENT_SCHEMA_VERSION, StorageError};

#[derive(Clone, Debug)]
pub struct SignerStore {
    path: PathBuf,
}

impl SignerStore {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let connection = open_current(path, true)?;
        drop(connection);
        Ok(Self {
            path: path.to_owned(),
        })
    }

    pub fn account(&self) -> Result<Option<SignerAccountRecord>, StorageError> {
        let connection = open_current(&self.path, true)?;
        read_account(&connection)
    }

    pub fn begin_provisioning(
        &self,
        now: UnixTimestampMilliseconds,
    ) -> Result<SignerAccountRecord, StorageError> {
        self.write(|transaction| {
            let revision = next_revision(read_revision(transaction)?)?;
            let stored_revision = stored_revision(revision)?;
            transaction
                .execute(
                    "UPDATE pod0_signer_state SET account_id=NULL,credential_kind_code='local_keychain',\
                     credential_kind_wire_code=NULL,expected_author_hex=NULL,state_revision=?1,\
                     stage_code='provisioning',updated_at_ms=?2,safe_detail=NULL WHERE singleton=1",
                    params![stored_revision, now.value],
                )
                .map_err(|error| StorageError::sqlite("begin signer provisioning", error))?;
            Ok(SignerAccountRecord {
                account_id: None,
                credential_kind: SignerCredentialKind::LocalKeychain,
                expected_author_hex: None,
                revision,
                stage: SignerStage::Provisioning,
                updated_at: now,
                safe_detail: None,
            })
        })
    }

    pub fn begin_restoring(
        &self,
        account_id: SignerAccountId,
        expected_author_hex: &str,
        now: UnixTimestampMilliseconds,
    ) -> Result<SignerAccountRecord, StorageError> {
        self.set_account(
            account_id,
            expected_author_hex,
            SignerStage::Restoring,
            now,
            None,
        )
    }

    pub fn mark_ready(
        &self,
        account_id: SignerAccountId,
        expected_author_hex: &str,
        now: UnixTimestampMilliseconds,
    ) -> Result<SignerAccountRecord, StorageError> {
        self.set_account(
            account_id,
            expected_author_hex,
            SignerStage::Ready,
            now,
            None,
        )
    }

    pub fn mark_unavailable(
        &self,
        account_id: SignerAccountId,
        expected_author_hex: &str,
        now: UnixTimestampMilliseconds,
        safe_detail: Option<&str>,
    ) -> Result<SignerAccountRecord, StorageError> {
        self.set_account(
            account_id,
            expected_author_hex,
            SignerStage::Unavailable,
            now,
            safe_detail,
        )
    }

    pub fn begin_sign_out(
        &self,
        account_id: SignerAccountId,
        now: UnixTimestampMilliseconds,
    ) -> Result<SignerAccountRecord, StorageError> {
        let current = self.account()?.ok_or(StorageError::SignerNotFound)?;
        if current.account_id != Some(account_id) {
            return Err(StorageError::SignerConflict);
        }
        self.set_account(
            account_id,
            current
                .expected_author_hex
                .as_deref()
                .ok_or(StorageError::InvalidSignerState)?,
            SignerStage::SigningOut,
            now,
            None,
        )
    }

    pub fn clear(
        &self,
        account_id: SignerAccountId,
        now: UnixTimestampMilliseconds,
    ) -> Result<(), StorageError> {
        self.write(|transaction| {
            let current = read_account(transaction)?.ok_or(StorageError::SignerNotFound)?;
            if current.account_id != Some(account_id) {
                return Err(StorageError::SignerConflict);
            }
            let revision = next_revision(current.revision)?;
            let stored_revision = stored_revision(revision)?;
            transaction
                .execute(
                    "UPDATE pod0_signer_state SET account_id=NULL,credential_kind_code=NULL,\
                     credential_kind_wire_code=NULL,expected_author_hex=NULL,state_revision=?1,\
                     stage_code='unconfigured',updated_at_ms=?2,safe_detail=NULL WHERE singleton=1",
                    params![stored_revision, now.value],
                )
                .map_err(|error| StorageError::sqlite("clear signer state", error))?;
            Ok(())
        })
    }

    pub fn reset_provisioning(&self, now: UnixTimestampMilliseconds) -> Result<(), StorageError> {
        self.write(|transaction| {
            let current = read_account(transaction)?.ok_or(StorageError::SignerNotFound)?;
            if current.stage != SignerStage::Provisioning || current.account_id.is_some() {
                return Err(StorageError::SignerConflict);
            }
            let revision = next_revision(current.revision)?;
            let stored_revision = stored_revision(revision)?;
            transaction
                .execute(
                    "UPDATE pod0_signer_state SET account_id=NULL,credential_kind_code=NULL,\
                     credential_kind_wire_code=NULL,expected_author_hex=NULL,state_revision=?1,\
                     stage_code='unconfigured',updated_at_ms=?2,safe_detail=NULL WHERE singleton=1",
                    params![stored_revision, now.value],
                )
                .map_err(|error| StorageError::sqlite("reset signer provisioning", error))?;
            Ok(())
        })
    }

    fn set_account(
        &self,
        account_id: SignerAccountId,
        expected_author_hex: &str,
        stage: SignerStage,
        now: UnixTimestampMilliseconds,
        safe_detail: Option<&str>,
    ) -> Result<SignerAccountRecord, StorageError> {
        if !valid_lower_hex(expected_author_hex, 64)
            || safe_detail.is_some_and(|value| value.len() > 512)
        {
            return Err(StorageError::InvalidSignerState);
        }
        self.write(|transaction| {
            if let Some(current) = read_account(transaction)?
                && current.account_id.is_some()
                && (current.account_id != Some(account_id)
                    || current.expected_author_hex.as_deref() != Some(expected_author_hex))
            {
                return Err(StorageError::SignerConflict);
            }
            let revision = next_revision(read_revision(transaction)?)?;
            let stored_revision = stored_revision(revision)?;
            transaction
                .execute(
                    "UPDATE pod0_signer_state SET account_id=?1,credential_kind_code='local_keychain',\
                     credential_kind_wire_code=NULL,expected_author_hex=?2,state_revision=?3,\
                     stage_code=?4,updated_at_ms=?5,safe_detail=?6 WHERE singleton=1",
                    params![
                        account_id.into_bytes().as_slice(),
                        expected_author_hex,
                        stored_revision,
                        stage_code(stage),
                        now.value,
                        safe_detail,
                    ],
                )
                .map_err(|error| StorageError::sqlite("update signer state", error))?;
            Ok(SignerAccountRecord {
                account_id: Some(account_id),
                credential_kind: SignerCredentialKind::LocalKeychain,
                expected_author_hex: Some(expected_author_hex.to_owned()),
                revision,
                stage,
                updated_at: now,
                safe_detail: safe_detail.map(str::to_owned),
            })
        })
    }

    fn write<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let mut connection = open_current(&self.path, false)?;
        configure(&connection)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| StorageError::sqlite("begin signer mutation", error))?;
        let result = operation(&transaction)?;
        transaction
            .commit()
            .map_err(|error| StorageError::sqlite("commit signer mutation", error))?;
        Ok(result)
    }
}

include!("signer_store_codec.rs");
