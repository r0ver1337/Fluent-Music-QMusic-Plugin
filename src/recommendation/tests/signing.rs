use super::super::signing::{material, signature, tea};
use base64::Engine;
#[test]
fn tea16_matches_independent_reference() {
    assert_eq!(
        tea([0; 8], &[0; 16]),
        [0xa8, 0x89, 0xf7, 0x98, 0x18, 0x2d, 0x80, 0x83]
    );
}
#[test]
#[allow(non_snake_case)]
fn embeddedMaterialMatchesTheVerifiedReference() {
    let key = material();
    let digest = ring::digest::digest(&ring::digest::SHA256, key.as_ref());
    assert_eq!(
        digest.as_ref(),
        &[
            0x4a, 0x68, 0xb2, 0x3b, 0x69, 0x1a, 0x7c, 0x04, 0x56, 0x4d, 0x4a, 0xfc, 0xf8, 0x1b,
            0xdc, 0xc7, 0x3c, 0x70, 0x34, 0x53, 0x3b, 0x39, 0xd1, 0x8a, 0x7f, 0x1e, 0xd8, 0x54,
            0x9e, 0x87, 0x8f, 0xe6,
        ]
    );
}
#[test]
fn mer_format_and_body_sensitivity() {
    let k = [7; 12];
    let r = [0; 32];
    let (a, m) = signature(b"a", b"fixture", &k, &r);
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(&a)
            .unwrap()
            .len(),
        32
    );
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(m)
            .unwrap()
            .len()
            % 8,
        0
    );
    assert_ne!(a, signature(b"b", b"fixture", &k, &r).0);
}
#[test]
fn composed_signature_matches_independent_python_fixture() {
    let key: [u8; 12] = std::array::from_fn(|i| i as u8);
    let random: [u8; 32] = std::array::from_fn(|i| i as u8);
    let (sign, mask) = signature(br#"{"fixture":true}"#, b"fixture", &key, &random);
    assert_eq!(sign, "QUJDREVGR0hJSktMtJ4ZVz180G6cYxb9laAD3XeDaQM=");
    assert_eq!(mask, "P9hOZbpJJPd7diKegVP1JvawZ0Ur0kyv");
}
