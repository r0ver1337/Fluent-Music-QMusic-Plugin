use qqmusic::qrc::{decode_qrc_api, decode_qrc_hex, decrypt_qrc_api, decrypt_qrc_hex};
use serde_json::Value;

fn vectors() -> Value {
    serde_json::from_str(include_str!("fixtures/qrcVectors.json")).unwrap()
}

#[test]
fn independent_reference_vectors_decrypt_exactly() {
    let fixture = vectors();
    for sample in fixture["samples"].as_array().unwrap() {
        let hex = sample["hex"].as_str().unwrap();
        let api = sample["api"].as_str().unwrap();
        let plain = sample["plaintext"].as_str().unwrap();
        assert_eq!(decrypt_qrc_hex(hex).unwrap(), plain);
        assert_eq!(decrypt_qrc_api(api).unwrap(), plain);
        assert_eq!(decrypt_qrc_hex(&hex.to_uppercase()).unwrap(), plain);
        assert_eq!(decode_qrc_hex(hex).unwrap(), decode_qrc_api(api).unwrap());
    }
}

#[test]
fn api_outer_layer_is_explicit_and_accepts_ascii_whitespace() {
    let fixture = vectors();
    let sample = &fixture["samples"][0];
    let api = sample["api"].as_str().unwrap();
    let wrapped = api
        .as_bytes()
        .chunks(60)
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
        .collect::<Vec<_>>()
        .join("\r\n");
    assert_eq!(
        decrypt_qrc_api(&wrapped).unwrap(),
        sample["plaintext"].as_str().unwrap()
    );
    assert!(decrypt_qrc_hex(api).is_err());
    assert!(decrypt_qrc_api(sample["hex"].as_str().unwrap()).is_err());
}

#[test]
fn empty_invalid_and_unaligned_input_is_rejected_without_panic() {
    for value in [
        "",
        " ",
        "0",
        "0011",
        "你",
        "gggggggggggggggg",
        "00 00 00 00 00 00 00 00",
    ] {
        assert_eq!(decrypt_qrc_hex(value).unwrap_err().code, "LYRICS_ENCODING");
    }
    for value in ["!invalid!", "A", "////", ""] {
        assert_eq!(decrypt_qrc_api(value).unwrap_err().code, "LYRICS_ENCODING");
    }
    assert_eq!(
        decrypt_qrc_hex(&"00".repeat(512 * 1024 + 8))
            .unwrap_err()
            .code,
        "LYRICS_LIMIT"
    );
    assert_eq!(
        decrypt_qrc_api(&"A".repeat(2 * 1024 * 1024 + 1))
            .unwrap_err()
            .code,
        "LYRICS_LIMIT"
    );
}

#[test]
fn compressed_stream_must_finish_and_validate_checksum_and_padding() {
    let fixture = vectors();
    for name in ["truncatedZlib", "invalidChecksum", "nonzeroPadding"] {
        assert_eq!(
            decrypt_qrc_hex(fixture["invalid"][name].as_str().unwrap())
                .unwrap_err()
                .code,
            "LYRICS_DECRYPT"
        );
    }
    assert_eq!(
        decrypt_qrc_hex(fixture["invalid"]["invalidUtf8"].as_str().unwrap())
            .unwrap_err()
            .code,
        "LYRICS_ENCODING"
    );
    assert_eq!(
        decrypt_qrc_hex(fixture["invalid"]["decompressionBomb"].as_str().unwrap())
            .unwrap_err()
            .code,
        "LYRICS_LIMIT"
    );
}

#[test]
fn corrupted_ciphertext_does_not_produce_successful_lyrics() {
    let fixture = vectors();
    let hex = fixture["samples"][0]["hex"].as_str().unwrap();
    for index in (0..hex.len()).step_by(16) {
        let mut damaged = hex.as_bytes().to_vec();
        damaged[index] = if damaged[index] == b'0' { b'1' } else { b'0' };
        assert!(decode_qrc_hex(std::str::from_utf8(&damaged).unwrap()).is_err());
    }
}

#[test]
fn end_to_end_xml_produces_original_words_and_absolute_times() {
    let fixture = vectors();
    let lyric = decode_qrc_api(fixture["samples"][0]["api"].as_str().unwrap()).unwrap();
    assert_eq!(lyric.title.as_deref(), Some("Fixture"));
    assert_eq!(lyric.offset_ms, -100);
    assert_eq!(lyric.lines[0].text, "你好");
    assert_eq!(lyric.lines[0].words[0].start_ms, 1000);
    assert_eq!(lyric.lines[0].words[1].start_ms, 1500);
    assert_eq!(lyric.lines[1].text, "A&B");
}
