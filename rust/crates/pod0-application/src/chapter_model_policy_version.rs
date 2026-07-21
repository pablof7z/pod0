use pod0_domain::ContentDigest;
use sha2::{Digest as _, Sha256};

pub(crate) fn input_version(
    transcript_content_digest: ContentDigest,
    configured_model: &str,
    policy_id: &str,
) -> String {
    let digest = transcript_content_digest.into_bytes();
    let mut digest_hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut digest_hex, "{byte:02x}").expect("writing to String cannot fail");
    }
    let mut hash = Sha256::new();
    hash.update(digest_hex.as_bytes());
    hash.update([0x1f]);
    hash.update(configured_model.as_bytes());
    hash.update([0x1f]);
    hash.update(policy_id.as_bytes());
    format!("{:x}", hash.finalize())
}
