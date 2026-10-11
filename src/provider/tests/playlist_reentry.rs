use super::authenticated_provider;
use super::*;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

pub(super) struct PlaylistServer {
    pub(super) base: String,
    pub(super) requests: Arc<Mutex<Vec<BTreeMap<String, String>>>>,
    pub(super) thread: JoinHandle<()>,
}
impl PlaylistServer {
    pub(super) fn new(
        count: usize,
        reply: impl Fn(usize, &BTreeMap<String, String>) -> Value + Send + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let thread = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            for index in 0..count {
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                        _ => return,
                    }
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                while !bytes.windows(4).any(|part| part == b"\r\n\r\n") {
                    let size = socket.read(&mut buffer).unwrap();
                    assert!(size > 0);
                    bytes.extend_from_slice(&buffer[..size]);
                    assert!(bytes.len() <= 65536);
                }
                let request = String::from_utf8(bytes).unwrap();
                let target = request
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap();
                let query = target.split_once('?').unwrap().1;
                let parameters = url::form_urlencoded::parse(query.as_bytes())
                    .into_owned()
                    .collect::<BTreeMap<_, _>>();
                let body = serde_json::to_vec(&reply(index, &parameters)).unwrap();
                captured.lock().unwrap().push(parameters);
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                socket.write_all(&body).unwrap();
            }
        });
        Self {
            base,
            requests,
            thread,
        }
    }
}

pub(super) fn read_page(
    provider: &mut Provider,
    id: &str,
    page: u32,
    cursor: Option<&str>,
) -> Result<Page<Track>, ApiError> {
    match provider.handle(Operation::Playlist {
        id: id.into(),
        page,
        cursor: cursor.map(str::to_owned),
    })? {
        ResultData::Tracks(page) => Ok(page),
        _ => panic!("Expected a track page"),
    }
}
pub(super) fn full_page(id: &str, update: Option<Value>) -> Value {
    let mut response = json!({"ret":0,"diss_id":id,"song_list":[{"song_id":42,"song_name":"离线歌曲","song_token":"synthetic-song-token"}],"total_num":2,"has_more":1,"passback":"next-page"});
    if let Some(update) = update {
        response["lastUpdateTime"] = update;
    }
    response
}

#[test]
fn reopening_library_playlist_always_requests_a_full_first_page() {
    let server = PlaylistServer::new(6, |_, request| {
        if request["opi_cmd"] == PLAYLISTS {
            return json!({"ret":0,"data":[{"dissid":"123","dissname":"离线歌单","song_num":2}]});
        }
        if request["lastUpdateTime"] == "0" {
            full_page("123", Some(json!(1800000000)))
        } else {
            json!({"ret":0,"diss_id":"123","lastUpdateTime":1800000000,"song_list":[],"total_num":2,"has_more":0})
        }
    });
    let mut provider = authenticated_provider(&server.base);
    let mut counts = Vec::new();
    for _ in 0..3 {
        let ResultData::Playlists(items) = provider.handle(Operation::Playlists).unwrap() else {
            panic!("playlists")
        };
        let page = read_page(&mut provider, &items[0].id, 0, None).unwrap();
        counts.push(page.items.len());
    }
    server.thread.join().unwrap();
    assert_eq!(
        counts,
        [1, 1, 1],
        "Reopening must not interpret an unchanged reply as zero songs"
    );
    for request in server
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|request| request["opi_cmd"] == DETAIL)
    {
        assert_eq!(request["page"], "0");
        assert_eq!(request["lastUpdateTime"], "0");
        assert_eq!(request["passback"], "");
    }
}

#[test]
fn pagination_modes_remain_separate_and_reopening_resets_version_and_cursor() {
    for id in ["123", "0"] {
        let id_owned = id.to_owned();
        let server = PlaylistServer::new(3, move |index, _| {
            let mut response = full_page(&id_owned, Some(json!(1800000000 + index as u64)));
            if index == 1 {
                response["song_list"][0]["song_id"] = json!(43);
                response["has_more"] = json!(0);
            }
            response
        });
        let mut provider = authenticated_provider(&server.base);
        let first = read_page(&mut provider, id, 0, None).unwrap();
        let next = read_page(&mut provider, id, 1, first.cursor.as_deref()).unwrap();
        let reopened = read_page(&mut provider, id, 0, Some("stale-cursor")).unwrap();
        assert_eq!(first.items[0].id, "42");
        assert_eq!(next.items[0].id, "43");
        assert_eq!(reopened.items[0].id, "42");
        assert_eq!(next.page, 1);
        server.thread.join().unwrap();
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests[0]["lastUpdateTime"], "0");
        assert_eq!(
            requests[1]["lastUpdateTime"],
            if id == "0" { "1800000000" } else { "0" }
        );
        assert_eq!(
            requests[1]["passback"],
            if id == "0" { "next-page" } else { "" }
        );
        assert_eq!(requests[1]["page"], if id == "0" { "0" } else { "1" });
        assert_eq!(requests[1]["pagemode"], if id == "0" { "1" } else { "0" });
        assert_eq!(requests[2]["lastUpdateTime"], "0");
        assert_eq!(requests[2]["passback"], "");
    }
}

#[test]
fn missing_version_on_reopen_cannot_reuse_an_older_read_version() {
    let server = PlaylistServer::new(3, |index, _| {
        full_page("0", (index == 0).then(|| json!(1800000000)))
    });
    let mut provider = authenticated_provider(&server.base);
    read_page(&mut provider, "0", 0, None).unwrap();
    let reopened = read_page(&mut provider, "0", 0, None).unwrap();
    read_page(&mut provider, "0", 1, reopened.cursor.as_deref()).unwrap();
    server.thread.join().unwrap();
    assert_eq!(server.requests.lock().unwrap()[2]["lastUpdateTime"], "0");
}

#[test]
fn empty_playlists_and_server_failures_are_not_hidden_by_a_read_cache() {
    let server = PlaylistServer::new(3, |index, _| match index {
        0 => full_page("123", Some(json!(1800000000))),
        1 => {
            json!({"ret":0,"diss_id":"123","total_num":0,"lastUpdateTime":1800000001,"has_more":0,"song_list":[]})
        }
        _ => json!({"ret":-1,"msg":"private-response-must-not-leak"}),
    });
    let mut provider = authenticated_provider(&server.base);
    read_page(&mut provider, "123", 0, None).unwrap();
    let empty = read_page(&mut provider, "123", 0, None).unwrap();
    assert!(empty.items.is_empty());
    assert!(!empty.has_more);
    let error = read_page(&mut provider, "123", 0, None).unwrap_err();
    assert_eq!(error.code, "SERVER");
    assert!(!error.message.contains("private-response"));
    server.thread.join().unwrap();
}
