use super::*;

pub(super) fn original(
    qrc_text: &str,
    lrc_text: &str,
    duration_ms: Option<u64>,
) -> Result<Option<LyricTimeline>, ApiError> {
    if qrc_text.is_empty() {
        return if lrc_text.is_empty() {
            Ok(None)
        } else {
            lrc::parse_lrc(lrc_text, duration_ms).map(Some)
        };
    }
    let qrc = qrc::decode_qrc_api(qrc_text)
        .map(from_qrc)
        .and_then(|timeline| {
            let lyrics = Lyrics {
                Original: Some(timeline),
                ..Default::default()
            };
            lyrics.validate()?;
            Ok(lyrics.Original.unwrap())
        });
    match qrc {
        Ok(timeline) => Ok(Some(timeline)),
        Err(error) => {
            if matches!(
                error.code.as_str(),
                "LYRICS_FORMAT" | "LYRICS_ENCODING" | "LYRICS_DECRYPT"
            ) && !lrc_text.is_empty()
                && let Ok(timeline) = lrc::parse_lrc(lrc_text, duration_ms)
                && !timeline.Lines.is_empty()
                && timeline.Lines.iter().all(|line| line.StartMs.is_some())
            {
                let lyrics = Lyrics {
                    Original: Some(timeline),
                    ..Default::default()
                };
                if lyrics.validate().is_ok() {
                    return Ok(lyrics.Original);
                }
            }
            Err(error)
        }
    }
}

pub(super) fn translation(mut timeline: LyricTimeline) -> Option<LyricTimeline> {
    for line in &mut timeline.Lines {
        if line.Text.trim() == "//" {
            line.Text.clear();
            line.Words.clear();
        }
    }
    timeline
        .Lines
        .iter()
        .any(|line| !line.Text.trim().is_empty())
        .then_some(timeline)
}
