#![cfg_attr(not(feature = "std"), no_std)]

use bls12_381::{
    hash_to_curve::{ExpandMsgXmd, HashToCurve},
    Bls12, G1Affine, G1Projective, G2Affine, G2Prepared,
};
use pairing::{group::Group, MultiMillerLoop};
use sha2_09::{Digest, Sha256};

pub const QUICKNET_CHAIN_HASH: [u8; 32] = [
    0x52, 0xdb, 0x9b, 0xa7, 0x0e, 0x0c, 0xc0, 0xf6, 0xea, 0xf7, 0x80, 0x3d, 0xd0, 0x74, 0x47, 0xa1,
    0xf5, 0x47, 0x77, 0x35, 0xfd, 0x3f, 0x66, 0x17, 0x92, 0xba, 0x94, 0x60, 0x0c, 0x84, 0xe9, 0x71,
];

/// Immutable quicknet timing parameters bound by `QUICKNET_CHAIN_HASH`.
pub const QUICKNET_GENESIS_UNIX_SECONDS: u64 = 1_692_803_367;
pub const QUICKNET_PERIOD_SECONDS: u64 = 3;

pub const QUICKNET_PUBLIC_KEY: [u8; 96] = [
    0x83, 0xcf, 0x0f, 0x28, 0x96, 0xad, 0xee, 0x7e, 0xb8, 0xb5, 0xf0, 0x1f, 0xca, 0xd3, 0x91, 0x22,
    0x12, 0xc4, 0x37, 0xe0, 0x07, 0x3e, 0x91, 0x1f, 0xb9, 0x00, 0x22, 0xd3, 0xe7, 0x60, 0x18, 0x3c,
    0x8c, 0x4b, 0x45, 0x0b, 0x6a, 0x0a, 0x6c, 0x3a, 0xc6, 0xa5, 0x77, 0x6a, 0x2d, 0x10, 0x64, 0x51,
    0x0d, 0x1f, 0xec, 0x75, 0x8c, 0x92, 0x1c, 0xc2, 0x2b, 0x0e, 0x17, 0xe6, 0x3a, 0xaf, 0x4b, 0xcb,
    0x5e, 0xd6, 0x63, 0x04, 0xde, 0x9c, 0xf8, 0x09, 0xbd, 0x27, 0x4c, 0xa7, 0x3b, 0xab, 0x4a, 0xf5,
    0xa6, 0xe9, 0xc7, 0x6a, 0x4b, 0xc0, 0x9e, 0x76, 0xea, 0xe8, 0x99, 0x1e, 0xf5, 0xec, 0xe4, 0x5a,
];

const QUICKNET_HASH_TO_G1_DOMAIN: &[u8] = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_";

/// Return the quicknet round that should exist at `unix_seconds`.
///
/// Round one is emitted at genesis. A timestamp before genesis cannot map to
/// this beacon.
pub fn round_at_unix_seconds(unix_seconds: u64) -> Option<u64> {
    let elapsed = unix_seconds.checked_sub(QUICKNET_GENESIS_UNIX_SECONDS)?;
    elapsed.checked_div(QUICKNET_PERIOD_SECONDS)?.checked_add(1)
}

/// Verify a Quicknet unchained beacon and derive the canonical drand output.
///
/// This function performs no network or JSON work. The caller must pin the
/// round and transport the 48-byte signature into runtime state.
pub fn verify_and_derive(round: u64, signature: &[u8]) -> Option<[u8; 32]> {
    let signature_bytes: [u8; 48] = signature.try_into().ok()?;
    let signature_point = Option::<G1Affine>::from(G1Affine::from_compressed(&signature_bytes))?;
    let public_key = Option::<G2Affine>::from(G2Affine::from_compressed(&QUICKNET_PUBLIC_KEY))?;

    let mut message_hasher = Sha256::new();
    message_hasher.update(round.to_be_bytes());
    let message = message_hasher.finalize();
    let message_point: G1Projective =
        HashToCurve::<ExpandMsgXmd<Sha256>>::hash_to_curve(message, QUICKNET_HASH_TO_G1_DOMAIN);
    let message_point = G1Affine::from(message_point);

    let negative_signature = -signature_point;
    let prepared_generator = G2Prepared::from(G2Affine::generator());
    let prepared_public_key = G2Prepared::from(public_key);
    let miller = Bls12::multi_miller_loop(&[
        (&negative_signature, &prepared_generator),
        (&message_point, &prepared_public_key),
    ]);
    if !bool::from(miller.final_exponentiation().is_identity()) {
        return None;
    }

    let digest = Sha256::digest(signature);
    Some(digest.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROUND_123_SIGNATURE: [u8; 48] = [
        0xb7, 0x5c, 0x69, 0xd0, 0xb7, 0x2a, 0x5d, 0x90, 0x6e, 0x85, 0x4e, 0x80, 0x8b, 0xa7, 0xe2,
        0xac, 0xcb, 0x15, 0x42, 0xac, 0x35, 0x5a, 0xe4, 0x86, 0xd5, 0x91, 0xaa, 0x9d, 0x43, 0x76,
        0x54, 0x82, 0xe2, 0x6c, 0xd0, 0x2d, 0xf8, 0x35, 0xd3, 0x54, 0x6d, 0x23, 0xc4, 0xb1, 0x3e,
        0x0d, 0xfc, 0x92,
    ];

    #[test]
    fn quicknet_round_123_vector_verifies_and_derives_randomness() {
        assert_eq!(
            verify_and_derive(123, &ROUND_123_SIGNATURE),
            Some([
                0xfb, 0x8f, 0x7b, 0xc2, 0x9b, 0xf2, 0x4d, 0xb5, 0x18, 0x71, 0xec, 0x8c, 0x79, 0xf3,
                0xa1, 0xe4, 0xbd, 0x05, 0x57, 0xbc, 0x0d, 0xfc, 0xee, 0x9e, 0xd1, 0xd9, 0x24, 0xe6,
                0x9d, 0x1c, 0x60, 0xdc,
            ])
        );
        assert_eq!(verify_and_derive(122, &ROUND_123_SIGNATURE), None);
        assert_eq!(verify_and_derive(123, &[0; 47]), None);
    }

    #[test]
    fn quicknet_round_clock_is_genesis_and_period_bound() {
        assert_eq!(
            round_at_unix_seconds(QUICKNET_GENESIS_UNIX_SECONDS - 1),
            None
        );
        assert_eq!(
            round_at_unix_seconds(QUICKNET_GENESIS_UNIX_SECONDS),
            Some(1)
        );
        assert_eq!(
            round_at_unix_seconds(QUICKNET_GENESIS_UNIX_SECONDS + 2),
            Some(1)
        );
        assert_eq!(
            round_at_unix_seconds(QUICKNET_GENESIS_UNIX_SECONDS + 3),
            Some(2)
        );
    }
}
