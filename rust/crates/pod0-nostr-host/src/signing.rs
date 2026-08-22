use std::fmt;

use k256::{
    AffinePoint, NonZeroScalar, ProjectivePoint, Scalar,
    elliptic_curve::{
        bigint::U256,
        generic_array::GenericArray,
        ops::Reduce,
        point::AffineCoordinates,
        subtle::ConditionallySelectable,
    },
    schnorr::SigningKey,
};
use nostr::{Event, PublicKey, UnsignedEvent, secp256k1::schnorr::Signature};
use rand_core::{OsRng, RngCore as _};
use zeroize::{ZeroizeOnDrop, Zeroizing};

use crate::{SigningSecretError, secure_hash::sha256};

const AUXILIARY_TAG: &[u8] = b"BIP0340/aux";
const CHALLENGE_TAG: &[u8] = b"BIP0340/challenge";
const NONCE_TAG: &[u8] = b"BIP0340/nonce";
const BECH32_CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";

/// Parsed signing authority backed by RustCrypto key material that zeroizes on drop.
pub struct SigningSecret {
    signing_key: SigningKey,
    public_key: PublicKey,
}

impl SigningSecret {
    pub fn parse(value: &str) -> Result<Self, SigningSecretError> {
        let bytes = decode_secret(value).ok_or(SigningSecretError)?;
        let signing_key =
            SigningKey::from_bytes(bytes.as_slice()).map_err(|_| SigningSecretError)?;
        let public_key = PublicKey::from_slice(signing_key.verifying_key().to_bytes().as_slice())
            .map_err(|_| SigningSecretError)?;
        Ok(Self {
            signing_key,
            public_key,
        })
    }

    #[must_use]
    pub fn public_key_hex(&self) -> String {
        self.public_key.to_hex()
    }

    pub(crate) fn public_key(&self) -> PublicKey {
        self.public_key
    }

    pub(crate) fn sign_event(&self, mut unsigned: UnsignedEvent) -> Result<Event, ()> {
        let event_id = unsigned.id();
        let signature = self.sign_id(event_id.as_bytes())?;
        let event = Event::new(
            event_id,
            unsigned.pubkey,
            unsigned.created_at,
            unsigned.kind,
            unsigned.tags,
            unsigned.content,
            signature,
        );
        event.verify().map_err(|_| ())?;
        Ok(event)
    }

    fn sign_id(&self, message: &[u8; 32]) -> Result<Signature, ()> {
        let mut auxiliary = Zeroizing::new([0_u8; 32]);
        OsRng.fill_bytes(&mut *auxiliary);
        let mut masked_key = tagged_hash(AUXILIARY_TAG, &[&*auxiliary]);
        let secret_bytes = Zeroizing::new(self.signing_key.to_bytes());
        for (masked, secret) in masked_key.iter_mut().zip(secret_bytes.iter()) {
            *masked ^= secret;
        }

        let nonce_digest = tagged_hash(
            NONCE_TAG,
            &[&*masked_key, self.public_key.as_bytes(), message],
        );
        let mut nonce =
            Zeroizing::new(NonZeroScalar::try_from(&nonce_digest[..]).map_err(|_| ())?);
        let nonce_point = (ProjectivePoint::GENERATOR * **nonce).to_affine();
        let negated_nonce = Zeroizing::new(-(*nonce));
        nonce.conditional_assign(&negated_nonce, nonce_point.y_is_odd());
        let r = AffinePoint::x(&nonce_point);

        let challenge_digest = tagged_hash(
            CHALLENGE_TAG,
            &[r.as_slice(), self.public_key.as_bytes(), message],
        );
        let challenge = <Scalar as Reduce<U256>>::reduce_bytes(GenericArray::from_slice(
            challenge_digest.as_slice(),
        ));
        let response =
            Zeroizing::new(**nonce + challenge * **self.signing_key.as_nonzero_scalar());

        let mut signature_bytes = [0_u8; 64];
        signature_bytes[..32].copy_from_slice(r.as_slice());
        signature_bytes[32..].copy_from_slice(response.to_bytes().as_slice());
        Signature::from_slice(&signature_bytes).map_err(|_| ())
    }
}

impl fmt::Debug for SigningSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SigningSecret")
            .field("public_key", &self.public_key)
            .finish_non_exhaustive()
    }
}

impl ZeroizeOnDrop for SigningSecret {}

fn tagged_hash(tag: &[u8], parts: &[&[u8]]) -> Zeroizing<[u8; 32]> {
    let tag_hash = sha256(&[tag]);
    let mut inputs = Vec::with_capacity(parts.len() + 2);
    inputs.push(&tag_hash[..]);
    inputs.push(&tag_hash[..]);
    inputs.extend_from_slice(parts);
    sha256(&inputs)
}

fn decode_secret(value: &str) -> Option<Zeroizing<[u8; 32]>> {
    decode_hex(value).or_else(|| decode_nsec(value))
}

fn decode_hex(value: &str) -> Option<Zeroizing<[u8; 32]>> {
    if value.len() != 64 {
        return None;
    }
    let mut output = Zeroizing::new([0_u8; 32]);
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = hex_value(pair[0])?.checked_mul(16)? + hex_value(pair[1])?;
    }
    Some(output)
}

fn decode_nsec(value: &str) -> Option<Zeroizing<[u8; 32]>> {
    if value.bytes().any(|byte| !(33..=126).contains(&byte))
        || (value.bytes().any(|byte| byte.is_ascii_lowercase())
            && value.bytes().any(|byte| byte.is_ascii_uppercase()))
    {
        return None;
    }
    let separator = value.rfind('1')?;
    if !value[..separator].eq_ignore_ascii_case("nsec") {
        return None;
    }
    let encoded = &value.as_bytes()[separator + 1..];
    if encoded.len() != 58 || !valid_bech32_checksum(&value.as_bytes()[..separator], encoded) {
        return None;
    }

    let mut output = Zeroizing::new([0_u8; 32]);
    let mut accumulator = 0_u16;
    let mut bits = 0_u8;
    let mut written = 0_usize;
    for character in &encoded[..encoded.len() - 6] {
        accumulator = (accumulator << 5) | u16::from(bech32_value(*character)?);
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            *output.get_mut(written)? = (accumulator >> bits) as u8;
            written += 1;
            accumulator &= (1_u16 << bits) - 1;
        }
    }
    (written == 32 && bits < 5 && accumulator == 0).then_some(output)
}

fn valid_bech32_checksum(hrp: &[u8], encoded: &[u8]) -> bool {
    let mut checksum = 1_u32;
    for byte in hrp {
        checksum = polymod(checksum, u32::from(byte.to_ascii_lowercase() >> 5));
    }
    checksum = polymod(checksum, 0);
    for byte in hrp {
        checksum = polymod(checksum, u32::from(byte.to_ascii_lowercase() & 0x1f));
    }
    for byte in encoded {
        let Some(value) = bech32_value(*byte) else {
            return false;
        };
        checksum = polymod(checksum, u32::from(value));
    }
    checksum == 1
}

fn polymod(previous: u32, value: u32) -> u32 {
    let generators = [
        0x3b6a_57b2,
        0x2650_8e6d,
        0x1ea1_19fa,
        0x3d42_33dd,
        0x2a14_62b3,
    ];
    let top = previous >> 25;
    let mut checksum = ((previous & 0x01ff_ffff) << 5) ^ value;
    for (index, generator) in generators.iter().enumerate() {
        if ((top >> index) & 1) != 0 {
            checksum ^= generator;
        }
    }
    checksum
}

fn bech32_value(character: u8) -> Option<u8> {
    BECH32_CHARSET
        .iter()
        .position(|candidate| *candidate == character.to_ascii_lowercase())
        .and_then(|index| u8::try_from(index).ok())
}

fn hex_value(character: u8) -> Option<u8> {
    match character {
        b'0'..=b'9' => Some(character - b'0'),
        b'a'..=b'f' => Some(character - b'a' + 10),
        b'A'..=b'F' => Some(character - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "0000000000000000000000000000000000000000000000000000000000000001";
    const NSEC: &str = "nsec1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqsmhltgl";
    const AUTHOR: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    #[test]
    fn parses_hex_and_nsec_without_nonzeroized_owned_secret_buffers() {
        assert_eq!(SigningSecret::parse(SECRET).unwrap().public_key_hex(), AUTHOR);
        assert_eq!(SigningSecret::parse(NSEC).unwrap().public_key_hex(), AUTHOR);
    }

    #[test]
    fn retained_key_declares_zeroize_on_drop() {
        fn assert_zeroize_on_drop<T: ZeroizeOnDrop>() {}
        assert_zeroize_on_drop::<SigningKey>();
        assert_zeroize_on_drop::<SigningSecret>();
    }
}
