use qqmusic::qrc::parse_qrc;

#[test]
fn raw_quoted_lyric_content_from_the_legacy_envelope_keeps_actual_text_and_word_times() {
    let value = "<?xml version=\"1.0\"?><QrcInfos><LyricInfo><Lyric_1 LyricContent=\"[ti:Synthetic]\n[1000,1000]\"(1000,100)A(1100,700)\"(1800,200)\n[2000,1000]B(2000,1000)\n\"/></LyricInfo></QrcInfos>";
    let lyric = parse_qrc(value).unwrap();
    assert_eq!(lyric.lines[0].text, "\"A\"");
    assert_eq!(lyric.lines[0].words[1].start_ms, 1100);
    assert_eq!(lyric.lines.len(), 2);
}

#[test]
fn raw_apostrophe_with_single_quote_envelope_and_escaped_quotes_decode_once() {
    let lyric = parse_qrc("<r LyricContent='[0,1000]don\'t(0,1000)'/>").unwrap();
    assert_eq!(lyric.lines[0].text, "don't");
    let lyric = parse_qrc("<r LyricContent=\"[0,1000]&quot;A&quot;(0,1000)\"/>").unwrap();
    assert_eq!(lyric.lines[0].text, "\"A\"");
}

#[test]
fn quote_compatibility_cannot_hide_invalid_roots_entities_or_duplicate_content() {
    for value in [
        "<r LyricContent=\"[0,1]\"A(0,1)\"/></wrong>",
        "<!DOCTYPE r [<!ENTITY x 'payload'>]><r LyricContent=\"[0,1]\"A(0,1)\"/>",
        "<r LyricContent=\"[0,1]\"A(0,1)&unknown;\"/>",
        "<r LyricContent=\"[0,1]\"A(0,1)\"/><r/>",
        "<r LyricContent=\"[0,1]\"A(0,1)\"><s LyricContent='[2,1]B(2,1)'/></r>",
        "<r LyricContent=\"[0,1]\"A(0,1)\" extra=\"B(0,1)\"/>",
    ] {
        assert!(parse_qrc(value).is_err(), "{value}");
    }
}
