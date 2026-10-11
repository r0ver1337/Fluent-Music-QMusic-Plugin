use super::*;

#[test]
fn gzip_responses_reject_truncation_trailing_members_and_expansion_over_limit() {
    let encode = |plain: &[u8]| {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(plain).unwrap();
        encoder.finish().unwrap()
    };
    let raw = encode(b"{}");
    assert_eq!(decompress(&raw, true).unwrap(), b"{}");
    assert!(decompress(&raw[..raw.len() - 1], true).is_err());
    let mut trailing = raw.clone();
    trailing.push(1);
    assert!(decompress(&trailing, true).is_err());
    let mut members = raw;
    members.extend(encode(b"{}"));
    assert!(decompress(&members, true).is_err());
    assert!(decompress(&encode(&vec![0; RESPONSE_LIMIT + 1]), true).is_err());
}
#[test]
fn compressed_responses_are_bounded_complete_and_reject_trailing_data() {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(b"{}").unwrap();
    let raw = encoder.finish().unwrap();
    assert_eq!(decompress(&raw, false).unwrap(), b"{}");
    assert!(decompress(&raw[..raw.len() - 1], false).is_err());
    let mut trailing = raw;
    trailing.push(1);
    assert!(decompress(&trailing, false).is_err());
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&vec![0; RESPONSE_LIMIT + 1]).unwrap();
    assert!(decompress(&encoder.finish().unwrap(), false).is_err());
}
