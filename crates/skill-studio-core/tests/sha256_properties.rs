// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! `identity::sha256_hex` is a hand-written SHA-256. Content fingerprints,
//! proposal ids, pack hashes and scope keys all use it, so it must agree
//! with a reference implementation (the `sha2` crate) on any input,
//! including lengths at the 55/56/64-byte padding boundaries.

use std::fmt::Write as _;

use proptest::prelude::*;
use sha2::{Digest, Sha256};
use skill_studio_core::identity::sha256_hex;

fn reference(data: &[u8]) -> String {
    let mut hex = String::new();
    for byte in Sha256::digest(data) {
        write!(hex, "{byte:02x}").unwrap();
    }
    hex
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn sha256_hex_matches_the_sha2_crate_for_any_input(
        data in prop_oneof![
            prop::collection::vec(any::<u8>(), 0..300),
            (50usize..70).prop_map(|len| vec![0x61; len]),
            (110usize..135).prop_map(|len| vec![0xff; len]),
        ]
    ) {
        prop_assert_eq!(sha256_hex(&data), reference(&data), "{} bytes", data.len());
    }
}
