use super::*;
use md5::{Digest, Md5};
use serde_json::Value;

fn vectors() -> Value {
    serde_json::from_str(include_str!("../../tests/qmc_vectors.json")).unwrap()
}

fn hex(raw: &str) -> Vec<u8> {
    raw.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn map_and_rc4_match_python_for_both_ekey_versions_and_offsets() {
    let vectors = vectors();
    let len = vectors["plain_len"].as_u64().unwrap() as usize;
    let mut plain: Vec<u8> = (0..len).map(|i| (i * 13 + 7) as u8).collect();
    plain[..3].copy_from_slice(b"ID3");
    for case in vectors["cases"].as_array().unwrap() {
        let ekey = case["ekey"].as_str().unwrap();
        assert_eq!(
            derive_master(ekey).unwrap(),
            hex(case["master_hex"].as_str().unwrap())
        );
        let stream = Stream::from_ekey(ekey).unwrap();
        let mut cipher = plain.clone();
        stream.apply(&mut cipher, 0);
        assert_eq!(
            format!("{:x}", Md5::digest(&cipher)),
            case["cipher_md5"].as_str().unwrap()
        );
        for span in case["spans"].as_array().unwrap() {
            let offset = span["offset"].as_u64().unwrap() as usize;
            let expected = hex(span["cipher_hex"].as_str().unwrap());
            let mut slice = plain[offset..offset + expected.len()].to_vec();
            stream.apply(&mut slice, offset as u64);
            assert_eq!(slice, expected);
        }
        for (index, chunk) in cipher.chunks_mut(997).enumerate() {
            stream.apply(chunk, (index * 997) as u64);
        }
        assert_eq!(cipher, plain);
    }
}

fn converted(bytes: &[u8], ekey: &str) -> (String, Vec<u8>) {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(bytes).unwrap();
    let format = convert(file.as_file_mut(), ekey).unwrap();
    (format.into(), std::fs::read(file.path()).unwrap())
}

#[test]
fn server_key_converts_raw_stream_and_all_v2_footers() {
    let vectors = vectors();
    let plain = STANDARD
        .decode(vectors["wav_plain_b64"].as_str().unwrap())
        .unwrap();
    for case in vectors["cases"].as_array().unwrap() {
        let cipher = STANDARD
            .decode(case["wav_cipher_b64"].as_str().unwrap())
            .unwrap();
        let ekey = case["ekey"].as_str().unwrap();
        let (format, audio) = converted(&cipher, ekey);
        assert_eq!(format, "wav");
        assert_eq!(audio, plain);
        for (csv, tag) in [
            (format!("{ekey},123,2"), b"QTag"),
            ("123,2,synthetic-mid".into(), b"STag"),
        ] {
            let mut bytes = cipher.clone();
            bytes.extend(csv.as_bytes());
            bytes.extend((csv.len() as u32).to_be_bytes());
            bytes.extend(tag);
            assert_eq!(converted(&bytes, ekey).1, plain);
        }
        let mut legacy = cipher.clone();
        legacy.extend(ekey.as_bytes());
        legacy.extend((ekey.len() as u32).to_le_bytes());
        assert_eq!(converted(&legacy, ekey).1, plain);
        let mut musicex = cipher.clone();
        musicex.extend([0; 188]);
        musicex.extend(0xc0u32.to_le_bytes());
        musicex.extend(1u32.to_le_bytes());
        musicex.extend(b"musicex\0");
        assert_eq!(converted(&musicex, ekey).1, plain);
    }
}

#[test]
fn large_in_place_conversion_does_not_reset_chunk_offsets() {
    let vectors = vectors();
    let len = vectors["plain_len"].as_u64().unwrap() as usize;
    let mut plain: Vec<u8> = (0..len).map(|i| (i * 13 + 7) as u8).collect();
    plain[..3].copy_from_slice(b"ID3");
    for case in vectors["cases"].as_array().unwrap() {
        let ekey = case["ekey"].as_str().unwrap();
        let mut cipher = plain.clone();
        Stream::from_ekey(ekey).unwrap().apply(&mut cipher, 0);
        assert_eq!(converted(&cipher, ekey).1, plain);
    }
}

#[test]
fn converted_reference_wav_is_actually_decodable_by_symphonia() {
    use symphonia::core::{
        codecs::DecoderOptions, formats::FormatOptions, io::MediaSourceStream,
        meta::MetadataOptions, probe::Hint,
    };
    let vectors = vectors();
    for case in vectors["cases"].as_array().unwrap() {
        let mut file = tempfile::Builder::new()
            .suffix(".audio")
            .tempfile()
            .unwrap();
        file.write_all(
            &STANDARD
                .decode(case["wav_cipher_b64"].as_str().unwrap())
                .unwrap(),
        )
        .unwrap();
        convert(file.as_file_mut(), case["ekey"].as_str().unwrap()).unwrap();
        let source = MediaSourceStream::new(Box::new(file.reopen().unwrap()), Default::default());
        let mut probed = symphonia::default::get_probe()
            .format(
                &Hint::new(),
                source,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .unwrap();
        let params = &probed.format.default_track().unwrap().codec_params;
        let mut decoder = symphonia::default::get_codecs()
            .make(params, &DecoderOptions::default())
            .unwrap();
        let packet = probed.format.next_packet().unwrap();
        let audio = decoder.decode(&packet).unwrap();
        assert_eq!(audio.frames(), 32);
        assert_eq!(audio.spec().rate, 8000);
    }
}

#[test]
fn rejects_invalid_keys_and_malformed_footers_without_panicking() {
    for key in [
        "",
        "not a key",
        "AAAA",
        EKEY_V2_PREFIX,
        "UVFNdXNpYyBFbmNWMixLZXk6AAAA",
    ] {
        assert!(Stream::from_ekey(key).is_err());
    }
    assert!(Stream::from_ekey(&"A".repeat(MAX_EKEY_BYTES + 1)).is_err());
    assert!(tea_cbc(&[255; 16], EKEY_V2_KEY1).is_err());
    for tail in [
        b"QTag".as_slice(),
        b"\0\0\x10\0QTag",
        b"bad\0\0\0\x03QTag",
        b"musicex\0",
    ] {
        assert!(footer_size(tail).is_err());
    }
    // Embedded EKey does not rescue a missing/invalid authorized response key.
    let vectors = vectors();
    let case = &vectors["cases"][0];
    let ekey = case["ekey"].as_str().unwrap();
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(
        &STANDARD
            .decode(case["wav_cipher_b64"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    let csv = format!("{ekey},123,2");
    file.write_all(csv.as_bytes()).unwrap();
    file.write_all(&(csv.len() as u32).to_be_bytes()).unwrap();
    file.write_all(b"QTag").unwrap();
    assert!(convert(file.as_file_mut(), "").is_err());
    // A different valid server key must not produce a playable file.
    assert_eq!(
        convert(
            file.as_file_mut(),
            vectors["cases"][2]["ekey"].as_str().unwrap()
        )
        .err()
        .unwrap()
        .code,
        "MEDIA_FORMAT"
    );
}

#[test]
fn plaintext_with_incidental_ekey_is_not_transformed() {
    let vectors = vectors();
    let wav = STANDARD
        .decode(vectors["wav_plain_b64"].as_str().unwrap())
        .unwrap();
    assert_eq!(converted(&wav, "unused").1, wav);
    assert_eq!(
        converted(&wav, vectors["cases"][0]["ekey"].as_str().unwrap()).1,
        wav
    );
    for (head, format) in [
        (b"fLaC".as_slice(), "flac"),
        (b"OggS", "ogg"),
        (b"ID3", "mp3"),
        (b"\0\0\0\x18ftypM4A ", "m4a"),
        (b"\xff\xfb", "mp3"),
    ] {
        assert_eq!(container(head), Some(format));
    }
    assert!(container(b"RIFF\0\0\0\0AVI ").is_none());
    assert!(container(b"unknown").is_none());
}

#[test]
fn conversion_deadline_is_enforced_before_rewriting_audio() {
    let vectors = vectors();
    let case = &vectors["cases"][0];
    let bytes = STANDARD
        .decode(case["wav_cipher_b64"].as_str().unwrap())
        .unwrap();
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(&bytes).unwrap();
    assert_eq!(
        convert_until(
            file.as_file_mut(),
            case["ekey"].as_str().unwrap(),
            Instant::now()
        )
        .err()
        .unwrap()
        .code,
        "MEDIA"
    );
    assert_eq!(std::fs::read(file.path()).unwrap(), bytes);
}
