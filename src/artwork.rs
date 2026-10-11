//! QQ-specific image metadata and CDN sizes belong in the plugin, not the player.
use crate::discovery::cover_url;
use serde_json::Value;

pub fn song_cover(song: &Value) -> (Option<String>, Option<String>) {
    let direct = [
        "album_pic_800x800",
        "albumPic800x800",
        "album_pic_500x500",
        "albumPic500x500",
        "album_pic_300x300",
        "albumPic300x300",
        "album_pic",
        "albumPic",
        "albumPicUrl",
        "cover",
        "pic_url",
        "album_pic_150x150",
        "albumPic150x150",
    ]
    .into_iter()
    .find_map(|key| public_cover(song[key].as_str()))
    .or_else(|| {
        [
            "pic800x800",
            "pic500x500",
            "pic300x300",
            "pic",
            "albumSmallUrl",
            "pic150x150",
        ]
        .into_iter()
        .find_map(|key| public_cover(song["album"][key].as_str()))
    });
    let canonical = direct.or_else(|| {
        // A song's `mid` is NOT its album's mid. Never guess from song IDs or album IDs.
        [
            song["album_mid"].as_str(),
            song["albumMid"].as_str(),
            song["album"]["mid"].as_str(),
        ]
        .into_iter()
        .flatten()
        .find(|mid| valid_mid(mid))
        .map(|mid| format!("https://y.gtimg.cn/music/photo_new/T002R150x150M000{mid}.jpg"))
    });
    let Some(canonical) = canonical else {
        return (None, None);
    };
    let cover = sized_album_cover(&canonical, 800).unwrap_or(canonical);
    let small = sized_album_cover(&cover, 90);
    (Some(cover), small)
}

fn public_cover(value: Option<&str>) -> Option<String> {
    let value = value.filter(|v| v.len() <= 4096)?;
    let mut url = url::Url::parse(&cover_url(Some(value))?).ok()?;
    // Raw server queries can contain private locators. Never forward them to the host.
    url.set_query(None);
    Some(url.into())
}

fn valid_mid(mid: &str) -> bool {
    !mid.is_empty() && mid.len() <= 64 && mid.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn sized_album_cover(value: &str, size: u32) -> Option<String> {
    let mut url = url::Url::parse(value).ok()?;
    if url.host_str() != Some("y.gtimg.cn") {
        return None;
    }
    let path = url.path().strip_prefix("/music/photo_new/T002R")?;
    let (dimensions, tail) = path.split_once("M000")?;
    let (w, h) = dimensions.split_once('x')?;
    if w.is_empty() || h.is_empty() || !w.bytes().chain(h.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mid = tail.strip_suffix(".jpg")?;
    // Preserve verified CDN cover suffixes such as {album_mid}_1.jpg.
    if mid.is_empty()
        || mid.len() > 128
        || !mid.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return None;
    }
    url.set_path(&format!("/music/photo_new/T002R{size}x{size}M000{tail}"));
    Some(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn song_covers_use_direct_metadata_then_album_mid_and_keep_small_cdn_variants() {
        for field in [
            "album_pic",
            "albumPic",
            "album_pic_150x150",
            "album_pic_300x300",
            "album_pic_500x500",
            "albumPicUrl",
            "cover",
        ] {
            let song = json!({field:"http://y.gtimg.cn/music/photo_new/T002R500x500M000AlbumMid_1.jpg?token=secret#fragment", "album_mid":"OtherAlbum"});
            assert_eq!(
                song_cover(&song),
                (
                    Some(
                        "https://y.gtimg.cn/music/photo_new/T002R800x800M000AlbumMid_1.jpg".into()
                    ),
                    Some("https://y.gtimg.cn/music/photo_new/T002R90x90M000AlbumMid_1.jpg".into())
                )
            );
        }
        for song in [
            json!({"album_mid":"AlbumMid"}),
            json!({"albumMid":"AlbumMid"}),
            json!({"album":{"mid":"AlbumMid","name":"Album"}}),
        ] {
            assert!(
                song_cover(&song)
                    .0
                    .unwrap()
                    .contains("800x800M000AlbumMid.jpg")
            );
            assert!(
                song_cover(&song)
                    .1
                    .unwrap()
                    .contains("90x90M000AlbumMid.jpg")
            );
        }
        for song in [
            json!({"mid":"SongMid"}),
            json!({"song_mid":"SongMid","album_id":123}),
            json!({"album_mid":"../secret"}),
            json!({"album_mid":""}),
        ] {
            assert_eq!(song_cover(&song), (None, None));
        }
        for bad in [
            "file:///a",
            "https://u:p@y.gtimg.cn/a",
            "https://y.gtimg.cn.evil.test/a",
            "https://127.0.0.1/a",
            "https://y.gtimg.cn:8080/a",
        ] {
            assert_eq!(song_cover(&json!({"album_pic":bad})), (None, None));
            assert!(
                song_cover(&json!({"album_pic":bad,"album_mid":"AlbumMid"}))
                    .0
                    .is_some()
            );
        }
        assert_eq!(
            song_cover(&json!({"album_pic":"https://qpic.y.qq.com/unknown-image?token=secret"})),
            (Some("https://qpic.y.qq.com/unknown-image".into()), None)
        );
        assert_eq!(
            song_cover(
                &json!({"cover":"https://y.gtimg.cn/music/photo_new/T001R500x500M000SingerMid.jpg"})
            )
            .1,
            None
        );
    }

    #[test]
    fn full_size_metadata_wins_without_inventing_variants_for_unknown_public_paths() {
        assert_eq!(
            song_cover(
                &json!({"album_pic_150x150":"https://qpic.y.qq.com/low", "album_pic_500x500":"https://qpic.y.qq.com/full"})
            ),
            (Some("https://qpic.y.qq.com/full".into()), None)
        );
        assert_eq!(
            song_cover(
                &json!({"album":{"pic150x150":"https://qpic.y.qq.com/low", "pic800x800":"https://qpic.y.qq.com/full"}})
            ),
            (Some("https://qpic.y.qq.com/full".into()), None)
        );
    }
}
