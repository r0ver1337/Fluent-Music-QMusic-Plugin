use qqmusic::qrc::parse_qrc;

#[test]
fn metadata_and_absolute_word_times_are_preserved() {
    let lyric = parse_qrc("[ti:Title][ar:Artist][al:Album][offset:-200]\n[by:Author]\n[1000,2000]你(1000,500)好(1500,500)").unwrap();
    assert_eq!(lyric.title.as_deref(), Some("Title"));
    assert_eq!(lyric.artist.as_deref(), Some("Artist"));
    assert_eq!(lyric.album.as_deref(), Some("Album"));
    assert_eq!(lyric.offset_ms, -200);
    assert_eq!(lyric.metadata["by"], "Author");
    assert_eq!(lyric.lines[0].start_ms, 1000);
    assert_eq!(lyric.lines[0].duration_ms, 2000);
    assert_eq!(lyric.lines[0].words[1].start_ms, 1500);
    assert_eq!(lyric.lines[0].words[1].duration_ms, 500);
}

#[test]
fn metadata_may_contain_commas_and_colons_without_becoming_a_timestamp() {
    let lyric = parse_qrc("[ti:Title, Part: One][ar:A, B]\n[0,1]A(0,1)").unwrap();
    assert_eq!(lyric.title.as_deref(), Some("Title, Part: One"));
    assert_eq!(lyric.artist.as_deref(), Some("A, B"));
    assert!(parse_qrc("[1:2,3]").is_err());
}

#[test]
fn standalone_cr_and_cr_lf_both_separate_lines() {
    let lyric = parse_qrc("[ti:T]\r[0,1]A(0,1)\r\n[1,1]B(1,1)").unwrap();
    assert_eq!(lyric.title.as_deref(), Some("T"));
    assert_eq!(lyric.lines.len(), 2);
}

#[test]
fn sorting_is_stable_and_does_not_adjust_the_original_timeline() {
    let lyric =
        parse_qrc("[5000,1000]B(5000,1000)\n[1000,500]A(1000,500)\n[1000,500]C(1000,500)").unwrap();
    assert_eq!(
        lyric
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>(),
        ["A", "C", "B"]
    );
    assert_eq!(lyric.lines[2].start_ms, 5000);
    assert_eq!(lyric.lines[2].duration_ms, 1000);
}

#[test]
fn xml_entities_and_physical_attribute_newlines_are_decoded_once() {
    let xml = "<?xml version='1.0'?><QrcInfos><Lyric_1 LyricContent='[ti:T&amp;amp;]\n[100,100]A&amp;B&lt;C&quot;(100,100)\n[200,100]D&apos;(200,100)'/></QrcInfos>";
    let lyric = parse_qrc(xml).unwrap();
    assert_eq!(lyric.title.as_deref(), Some("T&amp;"));
    assert_eq!(lyric.lines.len(), 2);
    assert_eq!(lyric.lines[0].text, "A&B<C\"");
    assert_eq!(lyric.lines[1].text, "D'");
    let reference = "<Lyric_1 LyricContent=\"[100,100]A(100,100)&#10;[200,100]B(200,100)\"/>";
    assert_eq!(parse_qrc(reference).unwrap().lines.len(), 2);
}

#[test]
fn utf16_ranges_handle_emoji_and_combining_characters_without_slicing_bytes() {
    let lyric = parse_qrc("[0,900]🎵(0,300)e\u{301}(300,300)你(600,300)").unwrap();
    let words = &lyric.lines[0].words;
    assert_eq!((words[0].start_utf16, words[0].end_utf16), (0, 2));
    assert_eq!((words[1].start_utf16, words[1].end_utf16), (2, 4));
    assert_eq!((words[2].start_utf16, words[2].end_utf16), (4, 5));
    assert_eq!(lyric.lines[0].text, "🎵e\u{301}你");
}

#[test]
fn signed_offset_and_negative_word_values_follow_java_clamping() {
    let lyric = parse_qrc("[offset:100]\n[-100,200]A(-1,-1)B(0,200)").unwrap();
    assert_eq!(lyric.offset_ms, 100);
    assert_eq!(lyric.lines[0].start_ms, -100);
    assert_eq!(lyric.lines[0].words[0].start_ms, 0);
    assert_eq!(lyric.lines[0].words[0].duration_ms, 0);
}

#[test]
fn textual_parentheses_are_not_mistaken_for_word_timestamps() {
    let lyric = parse_qrc("[0,100]a(note)b(x,y)c(0,100)").unwrap();
    assert_eq!(lyric.lines[0].text, "a(note)b(x,y)c");
}

#[test]
fn malformed_time_xml_and_untimed_suffix_are_rejected() {
    for value in [
        "not QRC",
        "[100,200]no timestamps",
        "[1,2,3]A(1,2)",
        "[1,-1]A(1,1)",
        "[1,2]A(x,2)",
        "[1,2]A(1,2)tail",
        "[1,2]A(1,2",
        "[offset:no]",
        "[9223372036854775807,1]A(1,1)",
        "[1,1]A(9223372036854775807,1)",
        "<r><Lyric_1 LyricContent='[1,1]A(1,1)'/></wrong>",
        "<r/>",
        "<r LyricContent='[1,1]A(1,1)'/><r/>",
        "<r LyricContent='[1,1]A(1,1)'><s LyricContent='[2,1]B(2,1)'/></r>",
        "<!DOCTYPE r [<!ENTITY x 'payload'>]><r LyricContent='&x;'/>",
        "<r LyricContent='&unknown;'/>",
        "<r LyricContent='[1,1]A(1,1)'>",
    ] {
        assert_eq!(
            parse_qrc(value).unwrap_err().code,
            "LYRICS_FORMAT",
            "{value}"
        );
    }
}

#[test]
fn empty_lyrics_and_bom_are_handled_explicitly() {
    assert!(parse_qrc("").unwrap().lines.is_empty());
    assert!(
        parse_qrc("<Lyric_1 LyricContent='' />")
            .unwrap()
            .lines
            .is_empty()
    );
    assert_eq!(parse_qrc("\u{feff}[0,1]A(0,1)").unwrap().lines[0].text, "A");
    assert_eq!(
        parse_qrc(" \u{feff}\n<r LyricContent='[0,1]A(0,1)'/> ")
            .unwrap()
            .lines[0]
            .text,
        "A"
    );
}

#[test]
fn parser_limits_and_serialized_field_names_are_stable() {
    assert_eq!(
        parse_qrc(&"x".repeat(2 * 1024 * 1024 + 1))
            .unwrap_err()
            .code,
        "LYRICS_LIMIT"
    );
    assert_eq!(
        parse_qrc(&"[0,1]A(0,1)\n".repeat(10_001)).unwrap_err().code,
        "LYRICS_LIMIT"
    );
    let lyric = parse_qrc("[0,1]A(0,1)").unwrap();
    let value = serde_json::to_value(lyric).unwrap();
    assert_eq!(value["lines"][0]["words"][0]["startUtf16"], 0);
    assert!(value.get("Lines").is_none());
}

#[test]
fn word_metadata_and_xml_limits_are_enforced() {
    let line = format!("[0,1]{}", "A(0,1)".repeat(100_001));
    assert_eq!(parse_qrc(&line).unwrap_err().code, "LYRICS_LIMIT");
    let metadata = (0..129)
        .map(|index| format!("[key{index}:value]\n"))
        .collect::<String>();
    assert_eq!(parse_qrc(&metadata).unwrap_err().code, "LYRICS_LIMIT");
    let nested = format!(
        "{}<Lyric_1 LyricContent='' />{}",
        "<r>".repeat(65),
        "</r>".repeat(65)
    );
    assert_eq!(parse_qrc(&nested).unwrap_err().code, "LYRICS_LIMIT");
    let nodes = format!("<r LyricContent=''>{}</r>", "<s/>".repeat(40_000));
    assert_eq!(parse_qrc(&nodes).unwrap_err().code, "LYRICS_LIMIT");
}

#[test]
fn long_nested_text_parentheses_are_scanned_without_changing_the_text() {
    let text = format!("{}note{}", "(".repeat(20_000), ")".repeat(20_000));
    let lyric = parse_qrc(&format!("[0,1]{text}(0,1)")).unwrap();
    assert_eq!(lyric.lines[0].text, text);
}
