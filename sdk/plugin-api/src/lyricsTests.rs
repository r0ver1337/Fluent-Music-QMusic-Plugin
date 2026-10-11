#![allow(non_snake_case)]
use super::*;

fn lyric() -> Lyrics {
    Lyrics {
        Original: Some(LyricTimeline {
            OffsetMs: 0,
            Lines: vec![LyricLine {
                StartMs: Some(1000),
                DurationMs: Some(1000),
                Text: "AB".into(),
                Words: vec![
                    LyricWord {
                        StartMs: 1000,
                        DurationMs: 500,
                        Text: "A".into(),
                    },
                    LyricWord {
                        StartMs: 1500,
                        DurationMs: 500,
                        Text: "B".into(),
                    },
                ],
            }],
        }),
        ..Default::default()
    }
}

#[test]
fn lyricsWireShapeAndCapabilityAreServiceIndependent() {
    let value = lyric();
    value.validate().unwrap();
    let wire = serde_json::to_value(ResultData::Lyrics(value.clone())).unwrap();
    assert_eq!(wire["kind"], "lyrics");
    assert_eq!(
        wire["data"]["original"]["lines"][0]["words"][1]["startMs"],
        1500
    );
    assert_eq!(wire["data"]["translation"], serde_json::Value::Null);
    let ResultData::Lyrics(returned) = serde_json::from_value(wire).unwrap() else {
        panic!("lyrics")
    };
    assert_eq!(value, returned);
    let info = PluginInfo {
        id: "test".into(),
        name: "Test".into(),
        version: "1".into(),
        api_version: 1,
        capabilities: vec![],
        extensions: vec![Capability::Lyrics],
    };
    assert!(info.supports(&Capability::Lyrics));
}

#[test]
fn invalidTimeOrderingOverflowAndTextRangesAreRejected() {
    for mode in 0..5 {
        let mut value = lyric();
        let line = &mut value.Original.as_mut().unwrap().Lines[0];
        match mode {
            0 => line.Words[1].StartMs = 999,
            1 => line.Words[0].DurationMs = u64::MAX,
            2 => line.Words[0].Text = "Mismatch".into(),
            3 => line.StartMs = None,
            _ => {
                line.StartMs = Some(i64::MAX);
                line.DurationMs = Some(1);
            }
        }
        assert_eq!(value.validate().unwrap_err().code, "LYRICS_FORMAT");
    }
    let mut value = lyric();
    let timeline = value.Original.as_mut().unwrap();
    let mut earlier = timeline.Lines[0].clone();
    earlier.StartMs = Some(500);
    timeline.Lines.push(earlier);
    assert_eq!(value.validate().unwrap_err().code, "LYRICS_FORMAT");
}

#[test]
fn escapingAndRepeatedTextCannotExceedTheFrameBudget() {
    let mut value = Lyrics {
        Original: Some(LyricTimeline {
            OffsetMs: 0,
            Lines: vec![LyricLine {
                StartMs: None,
                DurationMs: None,
                Text: "\u{0001}".repeat(MAX_MESSAGE_BYTES / 4),
                Words: vec![],
            }],
        }),
        ..Default::default()
    };
    assert_eq!(value.validate().unwrap_err().code, "LYRICS_LIMIT");
    value.Original.as_mut().unwrap().Lines[0].Text = "A".repeat(MAX_MESSAGE_BYTES);
    assert_eq!(value.validate().unwrap_err().code, "LYRICS_LIMIT");
}
