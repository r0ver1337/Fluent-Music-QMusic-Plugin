use super::{format_error, limit_error};
use fluent_plugin_api::ApiError;
use quick_xml::{Reader, escape::unescape, events::Event};

pub(super) fn lyric_content(value: &str) -> Result<String, ApiError> {
    match strict_content(value) {
        Ok(content) => Ok(content),
        Err(error) if error.code == "LYRICS_FORMAT" => {
            if let Some(repaired) = super::legacy_xml::repair_quotes(value)? {
                strict_content(&repaired)
            } else {
                Err(error)
            }
        }
        Err(error) => Err(error),
    }
}

fn strict_content(value: &str) -> Result<String, ApiError> {
    let mut reader = Reader::from_str(value);
    let mut depth = 0usize;
    let mut roots = 0;
    let mut nodes = 0;
    let mut content = None;
    loop {
        nodes += 1;
        if nodes > 40_000 || depth > 64 {
            return Err(limit_error());
        }
        let event = reader.read_event().map_err(|_| format_error())?;
        let starts_element = matches!(&event, Event::Start(_));
        match event {
            Event::Start(element) | Event::Empty(element) => {
                if depth == 0 {
                    roots += 1;
                }
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|_| format_error())?;
                    if attribute.key.as_ref() == b"LyricContent" {
                        if content.is_some() {
                            return Err(format_error());
                        }
                        // QRC embeds physical newlines in attributes; XML normalization would erase line boundaries.
                        let decoded = reader
                            .decoder()
                            .decode(&attribute.value)
                            .map_err(|_| format_error())?;
                        content =
                            Some(unescape(&decoded).map_err(|_| format_error())?.into_owned());
                    }
                }
                if starts_element {
                    depth += 1;
                }
            }
            Event::End(_) => depth = depth.checked_sub(1).ok_or_else(format_error)?,
            Event::DocType(_) | Event::GeneralRef(_) => return Err(format_error()),
            Event::Text(text)
                if depth == 0 && text.as_ref().iter().any(|byte| !byte.is_ascii_whitespace()) =>
            {
                return Err(format_error());
            }
            Event::CData(_) if depth == 0 => return Err(format_error()),
            Event::Eof => break,
            _ => {}
        }
    }
    if roots != 1 || depth != 0 {
        return Err(format_error());
    }
    content.ok_or_else(format_error)
}
