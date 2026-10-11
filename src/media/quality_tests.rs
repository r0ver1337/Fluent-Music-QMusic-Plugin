use super::*;
use serde_json::json;

#[test]
fn flat_sizes_are_paired_with_their_own_quality_and_keys_never_escape() {
    let song = json!({"song_size_standard": "5000000", "song_play_url_standard": "https://stream.qqmusic.qq.com/a.mp3",
        "song_size_sq": 32000000, "song_ekey_sq": "synthetic-private-key", "size_flac": 99999999,
        "song_play_url_hq": "https://stream.qqmusic.qq.com/a.mgg", "song_ekey_hq": "hq-key"});
    let result = list(&song).unwrap();
    assert_eq!(
        result
            .Items
            .iter()
            .map(|item| item.Id.as_str())
            .collect::<Vec<_>>(),
        vec!["sq", "hq", "standard"]
    );
    assert_eq!(result.Items[0].FileSizeBytes, Some(32000000));
    assert!(!result.Items[0].Playable);
    assert_eq!(result.Items[1].FileSizeBytes, None);
    assert_eq!(result.Items[1].Name, "高品质");
    assert_eq!(result.Items[1].Priority, 5);
    assert_eq!(result.Items[2].FileSizeBytes, Some(5000000));
    assert_eq!(select(&song, "hq").unwrap().ekey, Some("hq-key"));
    assert!(select(&song, "standard").unwrap().ekey.is_none());
    let wire = serde_json::to_string(&result).unwrap();
    for secret in [
        "synthetic-private-key",
        "hq-key",
        "https://",
        "song_play_url",
    ] {
        assert!(!wire.contains(secret));
    }
}

#[test]
fn absent_qualities_are_not_invented_and_unknown_size_is_not_zero() {
    let song = json!({"song_size_sq":0,"song_size_hq":null,"song_play_url_standard":"https://stream.qqmusic.qq.com/a.mp3", "size_320mp3":123});
    let result = list(&song).unwrap();
    assert_eq!(result.Items.len(), 1);
    assert_eq!(result.Items[0].Id, "standard");
    assert_eq!(result.Items[0].FileSizeBytes, None);
    assert_eq!(
        select(&song, "sq").err().unwrap().code,
        "QUALITY_UNAVAILABLE"
    );
}

#[test]
fn permission_denial_and_unsupported_formats_are_visible_but_cannot_play() {
    for field in ["copyright", "playable"] {
        let mut song =
            json!({"song_size_sq":123, "song_play_url_sq":"https://stream.qqmusic.qq.com/a.flac"});
        song[field] = json!(0);
        let result = list(&song).unwrap();
        assert_eq!(result.Items.len(), 1);
        assert!(!result.Items[0].Playable);
        assert_eq!(
            select(&song, "sq").err().unwrap().code,
            "QUALITY_UNAVAILABLE"
        );
    }
    let song =
        json!({"song_size_dolby": 42, "song_play_url_dolby":"https://stream.qqmusic.qq.com/a.ec3"});
    assert!(!list(&song).unwrap().Items[0].Playable);
    assert!(select(&song, "dolby").is_err());
}

#[test]
fn typed_quality_entries_respect_explicit_denial_instead_of_using_flat_fallback() {
    let song = json!({"song_play_url_sq":"https://stream.qqmusic.qq.com/flat.flac", "song_ekey_sq":"flat-key",
    "qualityList":[
        {"qualityType":6,"size":30000000,"playAccess":0,"url":"https://stream.qqmusic.qq.com/sq.mflac","eKey":"sq-key"},
        {"qulityType":4,"size":0,"playAccess":1,"url":"https://stream.qqmusic.qq.com/std.mgg","ekey":"std-key"},
        {"qualityType":99,"size":60000000}
    ]});
    let result = list(&song).unwrap();
    assert_eq!(result.Items.len(), 2);
    assert!(!result.Items[0].Playable);
    assert_eq!(result.Items[1].FileSizeBytes, None);
    assert_eq!(select(&song, "standard").unwrap().ekey, Some("std-key"));
    assert!(select(&song, "sq").is_err());
    assert!(result.Items.iter().all(|item| item.Priority != 8));
}

#[test]
fn bad_metadata_and_urls_fail_instead_of_silently_downgrading() {
    for song in [
        json!({"song_size_sq":-1}),
        json!({"song_size_sq":"not-a-size"}),
        json!({"song_play_url_sq":"https://example.com/a"}),
        json!({"qualityList":{}}),
        json!({"qualityList":[{"qualityType":6},{"qualityType":6}]}),
    ] {
        assert!(list(&song).is_err());
    }
}

#[test]
fn action_permissions_are_masks_and_explicit_entry_permissions_take_precedence() {
    let song = json!({"action":{"playAccess":{"SQ":0,"STANDARD":2}},
        "song_play_url_sq":"https://stream.qqmusic.qq.com/sq.flac",
        "song_play_url_standard":"https://stream.qqmusic.qq.com/std.mp3"});
    let result = list(&song).unwrap();
    assert!(!result.Items[0].Playable);
    assert!(result.Items[1].Playable);
    let mut song = song;
    song["qualityList"] = json!([{"qualityType":6,"playAccess":128,"url":"https://stream.qqmusic.qq.com/authorized.flac"}]);
    assert!(list(&song).unwrap().Items[0].Playable);
}

#[test]
fn catalogue_names_are_plain_and_unknown_formats_are_not_returned() {
    let catalogue = catalogue();
    assert_eq!(catalogue.Levels.len(), 7);
    assert!(
        catalogue
            .Levels
            .iter()
            .all(|level| !level.Name.contains("其他") && !level.Name.contains("暂不支持"))
    );
    assert!(catalogue.Levels[0].Disabled && catalogue.Levels[2].Disabled);
    assert!(!catalogue.Levels[1].Disabled);
    assert_eq!(catalogue.Levels[4].Name, "高品质");
    assert!(catalogue.Levels.iter().all(|level| level.Name != "高清"));
    assert!(
        list(&json!({"qualityList":[{"qualityType":99,"size":60000000}]}))
            .unwrap()
            .Items
            .is_empty()
    );
}
