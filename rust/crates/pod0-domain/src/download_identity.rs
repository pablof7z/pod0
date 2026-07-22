use sha2::{Digest as _, Sha256};

use crate::{DownloadAttemptId, DownloadIntentId};

/// Stable attempt identity shared by the application contract and durable store.
#[must_use]
pub fn download_attempt_identity(
    intent_id: DownloadIntentId,
    attempt: u16,
) -> Option<DownloadAttemptId> {
    if attempt == 0 {
        return None;
    }
    let mut hash = FramedHash::new(b"pod0-download-attempt-v1");
    hash.bytes(&intent_id.into_bytes());
    hash.u64(u64::from(attempt));
    Some(DownloadAttemptId::from_bytes(hash.first_16()))
}

struct FramedHash(Sha256);

impl FramedHash {
    fn new(domain: &[u8]) -> Self {
        let mut value = Self(Sha256::new());
        value.bytes(domain);
        value
    }

    fn bytes(&mut self, value: &[u8]) {
        self.0.update((value.len() as u64).to_be_bytes());
        self.0.update(value);
    }

    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_be_bytes());
    }

    fn first_16(self) -> [u8; 16] {
        self.0.finalize()[..16]
            .try_into()
            .expect("SHA-256 prefix length")
    }
}
