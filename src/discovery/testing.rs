use super::*;
use std::{
    io::Write,
    net::TcpListener,
    sync::{Arc, Mutex},
};
pub struct Server {
    pub base: String,
    pub requests: Arc<Mutex<Vec<String>>>,
    pub thread: std::thread::JoinHandle<()>,
}
impl Server {
    pub fn new(replies: Vec<(&'static str, Vec<u8>, &'static str)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let thread = std::thread::spawn(move || {
            for (status, body, extra) in replies {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0; 4096];
                loop {
                    let n = socket.read(&mut buf).unwrap();
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let len = headers
                            .lines()
                            .find_map(|l| {
                                l.strip_prefix("content-length:")
                                    .and_then(|s| s.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + len {
                            break;
                        }
                    }
                }
                captured
                    .lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&bytes).into_owned());
                write!(
                    socket,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
                    body.len()
                )
                .unwrap();
                // Oversized/redirect responses may be rejected before all bytes are read.
                let _ = socket.write_all(&body);
            }
        });
        Self {
            base,
            requests,
            thread,
        }
    }
}
