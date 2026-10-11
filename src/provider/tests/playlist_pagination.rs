use super::authenticated_provider;
use super::playlist_reentry::{PlaylistServer, full_page, read_page};
use super::*;

#[test]
fn ordinary_playlist_reads_every_page_without_conditional_version_or_cursor() {
    let server = PlaylistServer::new(4, |_, request| {
        let page = request["page"].parse::<u64>().unwrap();
        if request["lastUpdateTime"] != "0" || !request["passback"].is_empty() {
            return json!({"ret":0,"song_list":[],"total_num":101,"has_more":0,"lastUpdateTime":1800000000});
        }
        let mut result = full_page("123", Some(json!(1800000000)));
        let count = if page == 2 { 1 } else { 50 };
        result["song_list"] = json!((0..count).map(|index| json!({"song_id":42 + page * 50 + index,"song_name":format!("第{page}页歌曲{index}")})).collect::<Vec<_>>());
        result["total_num"] = json!(101);
        result["has_more"] = json!(page < 2);
        result
    });
    let mut provider = authenticated_provider(&server.base);
    let first = read_page(&mut provider, "123", 0, None).unwrap();
    let second = read_page(&mut provider, "123", 1, first.cursor.as_deref()).unwrap();
    let third = read_page(&mut provider, "123", 2, second.cursor.as_deref()).unwrap();
    let reopened = read_page(&mut provider, "123", 0, None).unwrap();
    server.thread.join().unwrap();
    assert_eq!(
        [first.items.len(), second.items.len(), third.items.len()],
        [50, 50, 1]
    );
    assert_eq!(first.items[0].id, "42");
    assert_eq!(second.items[0].id, "92");
    assert_eq!(third.items[0].id, "142");
    assert_eq!(reopened.items[0].id, "42");
    assert!(first.has_more && second.has_more && !third.has_more);
    for (request, page) in server.requests.lock().unwrap().iter().zip([0, 1, 2, 0]) {
        assert_eq!(request["page"], page.to_string());
        assert_eq!(request["pagemode"], "0");
        assert_eq!(request["page_size"], "50");
        assert_eq!(request["lastUpdateTime"], "0");
        assert_eq!(request["passback"], "");
    }
}

#[test]
fn liked_songs_keep_cursor_mode_and_cannot_continue_without_cursor() {
    let server = PlaylistServer::new(3, |index, request| {
        let mut result = full_page("0", Some(json!(1800000000 + index as u64)));
        result["song_list"][0]["song_id"] = json!(42 + index);
        result["passback"] = json!(format!("cursor-{}", index + 1));
        if index > 0 {
            assert_eq!(
                request["lastUpdateTime"],
                (1800000000 + index as u64 - 1).to_string()
            );
            assert_eq!(request["passback"], format!("cursor-{index}"));
        }
        result
    });
    let mut provider = authenticated_provider(&server.base);
    let first = read_page(&mut provider, "0", 0, None).unwrap();
    assert_eq!(
        read_page(&mut provider, "0", 1, None).unwrap_err().code,
        "PARAMETERS"
    );
    let second = read_page(&mut provider, "0", 1, first.cursor.as_deref()).unwrap();
    let third = read_page(&mut provider, "0", 2, second.cursor.as_deref()).unwrap();
    assert_eq!(second.items[0].id, "43");
    assert_eq!(third.items[0].id, "44");
    server.thread.join().unwrap();
    for request in server.requests.lock().unwrap().iter() {
        assert_eq!(request["page"], "0");
        assert_eq!(request["pagemode"], "1");
    }
}
