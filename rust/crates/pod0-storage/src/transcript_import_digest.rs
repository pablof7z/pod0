use pod0_domain::ContentDigest;
use sha2::{Digest as _, Sha256};

use crate::StorageError;

pub(crate) struct TranscriptImportHash(Sha256);

impl TranscriptImportHash {
    pub(crate) fn new(domain: &[u8]) -> Self {
        let mut hash = Sha256::new();
        part_into(&mut hash, domain);
        Self(hash)
    }

    pub(crate) fn bytes(&mut self, value: &[u8]) {
        part_into(&mut self.0, value);
    }

    pub(crate) fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    pub(crate) fn optional_text(&mut self, value: Option<&str>) {
        match value {
            Some(value) => {
                self.0.update([1]);
                self.text(value);
            }
            None => self.0.update([0]),
        }
    }

    pub(crate) fn u32(&mut self, value: u32) {
        self.0.update(value.to_be_bytes());
    }

    pub(crate) fn u64(&mut self, value: u64) {
        self.0.update(value.to_be_bytes());
    }

    pub(crate) fn i64(&mut self, value: i64) {
        self.0.update(value.to_be_bytes());
    }

    pub(crate) fn finish(self) -> ContentDigest {
        ContentDigest::from_bytes(self.0.finalize().into())
    }
}

pub(crate) fn digest_bytes(value: &[u8]) -> ContentDigest {
    ContentDigest::from_bytes(Sha256::digest(value).into())
}

pub(crate) fn parse_hex_digest(value: &str) -> Result<ContentDigest, StorageError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(StorageError::InvalidLegacyRecord {
            entity: "transcript selection",
            index: 0,
            detail: "content hash is not SHA-256",
        });
    }
    let mut bytes = [0_u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(|_| {
            StorageError::InvalidLegacyRecord {
                entity: "transcript selection",
                index: 0,
                detail: "content hash is not SHA-256",
            }
        })?;
    }
    let canonical = hex_digest(ContentDigest::from_bytes(bytes));
    if canonical != value {
        return Err(StorageError::InvalidLegacyRecord {
            entity: "transcript selection",
            index: 0,
            detail: "content hash is not canonical lowercase SHA-256",
        });
    }
    Ok(ContentDigest::from_bytes(bytes))
}

pub(crate) fn hex_digest(value: ContentDigest) -> String {
    value
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn part_into(hash: &mut Sha256, value: &[u8]) {
    hash.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hash.update(value);
}
