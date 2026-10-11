use super::*;
use serde_json::json;

#[test]
fn lrc_fractions_multiple_timestamps_and_offsets_use_milliseconds() {
    let timeline = lrc::parse_lrc(
        "[ti:T][offset:-50]\n[00:01.5][00:03.500]A&amp;B\n[00:02.50]C",
        Some(5000),
    )
    .unwrap();
    assert_eq!(timeline.OffsetMs, -50);
    assert_eq!(
        timeline
            .Lines
            .iter()
            .map(|line| line.StartMs)
            .collect::<Vec<_>>(),
        [Some(1500), Some(2500), Some(3500)]
    );
    assert_eq!(timeline.Lines[0].Text, "A&B");
    assert_eq!(timeline.Lines[2].DurationMs, Some(1500));
    assert!(timeline.Lines[0].Words.is_empty());
}

#[test]
fn no_lyrics_plain_text_and_unknown_last_duration_are_explicit() {
    let lyric = parse_response(json!({"ret":0}), None).unwrap();
    assert_eq!(lyric, Lyrics::default());
    let lyric = parse_response(json!({"ret":0,"song_lyric":"line A\nline B"}), None).unwrap();
    assert!(
        lyric
            .Original
            .unwrap()
            .Lines
            .iter()
            .all(|line| line.StartMs.is_none())
    );
    let timeline = lrc::parse_lrc("[00:01]A", None).unwrap();
    assert_eq!(timeline.Lines[0].DurationMs, None);
}

#[test]
fn malformed_lrc_and_schema_never_become_successful_empty_lyrics() {
    for text in [
        "[00:99]A",
        "[00:01.1234]A",
        "[999999999999999:00]A",
        "[00:01]A\nuntimed",
    ] {
        assert_eq!(
            lrc::parse_lrc(text, None).unwrap_err().code,
            "LYRICS_FORMAT"
        );
    }
    for value in [
        json!({"ret":0,"song_lyric":12}),
        json!({"ret":0,"lyric_start_ms":false}),
    ] {
        assert_eq!(parse_response(value, None).unwrap_err().code, "SCHEMA");
    }
    assert_eq!(
        parse_response(json!({"ret":10200,"msg":"private-secret"}), None)
            .unwrap_err()
            .code,
        "APP_PERMISSION_DENIED"
    );
}

#[test]
fn qrc_original_and_translation_are_decoded_before_returning_standard_timelines() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/qrcVectors.json")).unwrap();
    let encoded = fixture["samples"][0]["api"].as_str().unwrap();
    let lyric = parse_response(json!({"ret":0,"song_qrc_enc_lyric":encoded,"song_translate_enc_lyric":encoded,"song_lyric":"[00:01]fallback","lyric_start_ms":300}), None).unwrap();
    assert_eq!(lyric.StartMs, Some(300));
    let original = lyric.Original.unwrap();
    let translation = lyric.Translation.unwrap();
    assert_eq!(original.Lines[0].Text, "你好");
    assert_eq!(original.Lines[0].Words[1].StartMs, 1500);
    assert_eq!(translation.Lines[0].Words[0].Text, "你");
    assert_eq!(original.OffsetMs, -100);
    let returned = serde_json::to_string(&original).unwrap();
    assert!(
        !returned.contains(encoded) && !returned.contains("qrc") && !returned.contains("enc_lyric")
    );
}

#[test]
fn invalid_qrc_uses_only_a_valid_timed_lrc_and_never_pretends_to_have_word_times() {
    let lyrics = parse_response(
        json!({"ret":0,"song_qrc_enc_lyric":"broken","song_lyric":"[00:01]valid"}),
        None,
    )
    .unwrap();
    let original = lyrics.Original.unwrap();
    assert_eq!(original.Lines[0].Text, "valid");
    assert!(original.Lines[0].Words.is_empty());
    for lrc in ["", "not a timed LRC", "[00:99]broken"] {
        assert_eq!(
            parse_response(
                json!({"ret":0,"song_qrc_enc_lyric":"broken","song_lyric":lrc}),
                None
            )
            .unwrap_err()
            .code,
            "LYRICS_ENCODING"
        );
    }
}

#[test]
fn independently_encrypted_lrc_translation_keeps_its_own_timeline() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/qrcVectors.json")).unwrap();
    let lyric = parse_response(json!({"ret":0,"song_lyric":"[00:01]Original","song_translate_enc_lyric":fixture["translation"]["api"]}), Some(5000)).unwrap();
    let translation = lyric.Translation.unwrap();
    assert_eq!(translation.OffsetMs, 50);
    assert_eq!(translation.Lines[0].StartMs, Some(1500));
    assert_eq!(translation.Lines[0].Text, "译文A");
    assert!(translation.Lines[0].Words.is_empty());
    assert_eq!(translation.Lines[1].DurationMs, Some(2000));
}

#[test]
fn duplicate_lrc_starts_have_the_same_next_distinct_end_and_entities_decode_once() {
    let timeline = lrc::parse_lrc("[00:01]A&amp;amp;\n[00:01]B\n[00:02]C", None).unwrap();
    assert_eq!(timeline.Lines[0].Text, "A&amp;");
    assert_eq!(timeline.Lines[0].DurationMs, Some(1000));
    assert_eq!(timeline.Lines[1].DurationMs, Some(1000));
    assert_eq!(timeline.Lines[2].DurationMs, None);
}

#[test]
fn slash_translations_become_empty_timed_boundaries_rather_than_leaking_previous_text() {
    let timeline = lrc::parse_lrc("[00:01]译文\n[00:02] // \n[00:03]下句", Some(4000)).unwrap();
    let normalized = normalize::translation(timeline).unwrap();
    assert_eq!(normalized.Lines.len(), 3);
    assert_eq!(normalized.Lines[0].DurationMs, Some(1000));
    assert_eq!(normalized.Lines[1].StartMs, Some(2000));
    assert_eq!(normalized.Lines[1].Text, "");
    assert!(normalized.Lines[1].Words.is_empty());
    Lyrics {
        Translation: Some(normalized),
        ..Default::default()
    }
    .validate()
    .unwrap();
    assert!(
        normalize::translation(lrc::parse_lrc("[00:01]//\n[00:02]//", None).unwrap()).is_none()
    );
    assert!(
        parse_response(json!({"ret":0,"song_translate_enc_lyric":" // "}), None)
            .unwrap()
            .Translation
            .is_none()
    );
}

#[test]
fn qrc_limits_and_upstream_denial_cannot_be_hidden_by_lrc_fallback() {
    assert_eq!(parse_response(json!({"ret":0,"song_qrc_enc_lyric":"A".repeat(2 * 1024 * 1024 + 1),"song_lyric":"[00:01]Valid"}), None).unwrap_err().code, "LYRICS_LIMIT");
    assert_eq!(
        parse_response(
            json!({"ret":10200,"song_qrc_enc_lyric":"broken","song_lyric":"[00:01]Valid"}),
            None
        )
        .unwrap_err()
        .code,
        "APP_PERMISSION_DENIED"
    );
}
