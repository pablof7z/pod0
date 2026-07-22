use std::collections::BTreeSet;

use pod0_domain::ContentDigest;
use pod0_storage::{
    LegacyTranscriptWorkflowBackupRow as StoredRow,
    LegacyTranscriptWorkflowRowClassification as StoredClassification, StorageError,
};

use crate::transcript_workflow_cutover_types::{
    LegacyTranscriptWorkflowBackupRow, LegacyTranscriptWorkflowRowClassification,
};

pub(super) fn source_generation(fingerprint: ContentDigest) -> u64 {
    let prefix: [u8; 8] = fingerprint.into_bytes()[..8]
        .try_into()
        .expect("digest prefix");
    (u64::from_be_bytes(prefix) & (i64::MAX as u64)).max(1)
}

pub(super) fn stored_rows(
    rows: Vec<LegacyTranscriptWorkflowBackupRow>,
) -> Result<Vec<StoredRow>, StorageError> {
    let mut rows = rows
        .into_iter()
        .map(|row| StoredRow {
            episode_id: row.episode_id,
            row_bytes: row.row_bytes,
            row_fingerprint: row.row_fingerprint,
            classification: classification(row.classification),
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| {
        (
            row.episode_id.into_bytes(),
            row.row_fingerprint.into_bytes(),
            classification_rank(row.classification),
        )
    });
    let unique = rows
        .iter()
        .map(|row| row.row_fingerprint)
        .collect::<BTreeSet<_>>();
    if unique.len() != rows.len() {
        return Err(StorageError::TranscriptWorkflowConflict);
    }
    Ok(rows)
}

fn classification(value: LegacyTranscriptWorkflowRowClassification) -> StoredClassification {
    match value {
        LegacyTranscriptWorkflowRowClassification::Restart => StoredClassification::Restart,
        LegacyTranscriptWorkflowRowClassification::RecoverProvider => {
            StoredClassification::RecoverProvider
        }
        LegacyTranscriptWorkflowRowClassification::Ambiguous => StoredClassification::Ambiguous,
        LegacyTranscriptWorkflowRowClassification::Blocked => StoredClassification::Blocked,
        LegacyTranscriptWorkflowRowClassification::Failed => StoredClassification::Failed,
        LegacyTranscriptWorkflowRowClassification::Cancelled => StoredClassification::Cancelled,
        LegacyTranscriptWorkflowRowClassification::Succeeded => StoredClassification::Succeeded,
        LegacyTranscriptWorkflowRowClassification::IndexPending => {
            StoredClassification::IndexPending
        }
        LegacyTranscriptWorkflowRowClassification::IndexSucceeded => {
            StoredClassification::IndexSucceeded
        }
        LegacyTranscriptWorkflowRowClassification::Obsolete => StoredClassification::Obsolete,
    }
}

const fn classification_rank(value: StoredClassification) -> u8 {
    match value {
        StoredClassification::Restart => 1,
        StoredClassification::RecoverProvider => 2,
        StoredClassification::Ambiguous => 3,
        StoredClassification::Blocked => 4,
        StoredClassification::Failed => 5,
        StoredClassification::Cancelled => 6,
        StoredClassification::Succeeded => 7,
        StoredClassification::IndexPending => 8,
        StoredClassification::IndexSucceeded => 9,
        StoredClassification::Obsolete => 10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_bit_fingerprints_keep_their_remaining_entropy() {
        let mut first = [0_u8; 32];
        first[..8].copy_from_slice(&0x8000_0000_0000_0001_u64.to_be_bytes());
        let mut second = first;
        second[..8].copy_from_slice(&0x8000_0000_0000_0002_u64.to_be_bytes());

        assert_eq!(source_generation(ContentDigest::from_bytes(first)), 1);
        assert_eq!(source_generation(ContentDigest::from_bytes(second)), 2);
    }
}
